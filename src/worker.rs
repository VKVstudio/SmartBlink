//! Detector worker thread (30 Hz tick) and RAM-only word scan-code buffer.
//! Architectural invariants (AGENTS.md #2, #7):
//! - Keystrokes never leave RAM; `WordBuffer` volatile-zeroes all slots on reset and drop.
//! - Hard reset on mouse click, navigation keys, modifier chords, idle timeout (3.5 s),
//!   focus change, or immune foreground process.

use crate::hooks;
use crate::immunity::FocusCache;
use crate::ring::{global_ring, InputKind, InputRecord};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub const MAX_WORD_KEYS: usize = 64;
pub const IDLE_RESET_MS: u32 = 3500;
const WORKER_TICK_MS: u64 = 33; // ~30 Hz

// Win32 virtual key codes used for buffer lifecycle rules.
const VK_BACK: u16 = 0x08;
const VK_TAB: u16 = 0x09;
const VK_RETURN: u16 = 0x0D;
const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12; // Alt
const VK_PAUSE: u16 = 0x13;
const VK_CAPITAL: u16 = 0x14;
const VK_ESCAPE: u16 = 0x1B;
const VK_PRIOR: u16 = 0x21; // Page Up
const VK_NEXT: u16 = 0x22; // Page Down
const VK_END: u16 = 0x23;
const VK_HOME: u16 = 0x24;
const VK_LEFT: u16 = 0x25;
const VK_UP: u16 = 0x26;
const VK_RIGHT: u16 = 0x27;
const VK_DOWN: u16 = 0x28;
const VK_INSERT: u16 = 0x2D;
const VK_DELETE: u16 = 0x2E;
const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;
const VK_NUMMLOCK: u16 = 0x90;
const VK_SCROLL: u16 = 0x91;
const VK_LSHIFT: u16 = 0xA0;
const VK_RSHIFT: u16 = 0xA1;
const VK_LCONTROL: u16 = 0xA2;
const VK_RCONTROL: u16 = 0xA3;
const VK_LMENU: u16 = 0xA4;
const VK_RMENU: u16 = 0xA5;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyStrokeSlot {
    pub vk: u16,
    pub scan_code: u16,
    pub flags: u32,
    pub ms_tick: u32,
}

impl KeyStrokeSlot {
    pub const fn zeroed() -> Self {
        Self {
            vk: 0,
            scan_code: 0,
            flags: 0,
            ms_tick: 0,
        }
    }
}

pub struct WordBuffer {
    slots: [KeyStrokeSlot; MAX_WORD_KEYS],
    len: usize,
    last_tick: Option<u32>,
    ctrl_down: bool,
    alt_down: bool,
    win_down: bool,
}

impl Default for WordBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WordBuffer {
    fn drop(&mut self) {
        self.reset();
    }
}

impl WordBuffer {
    pub const fn new() -> Self {
        Self {
            slots: [KeyStrokeSlot::zeroed(); MAX_WORD_KEYS],
            len: 0,
            last_tick: None,
            ctrl_down: false,
            alt_down: false,
            win_down: false,
        }
    }

    /// Volatile-zero all keystroke slots in RAM and clear length/timestamp.
    pub fn reset(&mut self) {
        for slot in &mut self.slots {
            unsafe {
                ptr::write_volatile(slot, KeyStrokeSlot::zeroed());
            }
        }
        self.len = 0;
        self.last_tick = None;
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[allow(dead_code)]
    pub fn as_slice(&self) -> &[KeyStrokeSlot] {
        &self.slots[..self.len]
    }

    /// Process one input record according to ARCHITECTURE.md §2 word buffer lifecycle rules.
    pub fn feed(&mut self, rec: InputRecord) {
        match rec.kind {
            InputKind::Empty => {}
            InputKind::MouseClick => {
                self.reset();
            }
            InputKind::KeyUp => {
                self.update_modifier(rec.vk, false);
            }
            InputKind::KeyDown => {
                if self.update_modifier(rec.vk, true) {
                    // Pressing Ctrl, Alt, or Win resets the active word buffer.
                    self.reset();
                    return;
                }

                // Ignore pure shift/lock/pause keys for word buffer content.
                if is_non_text_toggle_or_shift(rec.vk) {
                    return;
                }

                // Any chord while Ctrl, Alt, or Win is held resets and stays ignored.
                if self.ctrl_down || self.alt_down || self.win_down {
                    self.reset();
                    return;
                }

                // Hard reset on navigation, escape, enter, tab, function keys.
                if is_navigation_or_reset_vk(rec.vk) {
                    self.reset();
                    return;
                }

                // Idle timeout check before recording or backspacing.
                if let Some(prev) = self.last_tick {
                    if rec.ms_tick.wrapping_sub(prev) > IDLE_RESET_MS {
                        self.reset();
                    }
                }

                if rec.vk == VK_BACK {
                    if self.len > 0 {
                        self.len -= 1;
                        unsafe {
                            ptr::write_volatile(&mut self.slots[self.len], KeyStrokeSlot::zeroed());
                        }
                        self.last_tick = Some(rec.ms_tick);
                    } else {
                        self.reset();
                    }
                    return;
                }

                // Append to word buffer; if capacity is reached, reset to avoid stale partial tokens.
                if self.len >= MAX_WORD_KEYS {
                    self.reset();
                }
                self.slots[self.len] = KeyStrokeSlot {
                    vk: rec.vk,
                    scan_code: rec.scan_code,
                    flags: rec.flags,
                    ms_tick: rec.ms_tick,
                };
                self.len += 1;
                self.last_tick = Some(rec.ms_tick);
            }
        }
    }

    /// Updates Ctrl/Alt/Win state. Returns `true` if `vk` is a Ctrl/Alt/Win modifier.
    fn update_modifier(&mut self, vk: u16, down: bool) -> bool {
        match vk {
            VK_CONTROL | VK_LCONTROL | VK_RCONTROL => {
                self.ctrl_down = down;
                true
            }
            VK_MENU | VK_LMENU | VK_RMENU => {
                self.alt_down = down;
                true
            }
            VK_LWIN | VK_RWIN => {
                self.win_down = down;
                true
            }
            _ => false,
        }
    }
}

fn is_non_text_toggle_or_shift(vk: u16) -> bool {
    matches!(
        vk,
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT | VK_CAPITAL | VK_SCROLL | VK_NUMMLOCK | VK_PAUSE
    )
}

fn is_navigation_or_reset_vk(vk: u16) -> bool {
    matches!(
        vk,
        VK_TAB
            | VK_RETURN
            | VK_ESCAPE
            | VK_PRIOR
            | VK_NEXT
            | VK_END
            | VK_HOME
            | VK_LEFT
            | VK_UP
            | VK_RIGHT
            | VK_DOWN
            | VK_INSERT
            | VK_DELETE
            | 0x70..=0x87 // F1..F24
    )
}

pub struct WorkerGuard {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
        global_ring().clear_and_zero();
    }
}

pub fn start(blacklist: Vec<String>) -> windows::core::Result<WorkerGuard> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);

    let join = thread::Builder::new()
        .name("smartblink-worker".into())
        .spawn(move || {
            let mut focus = FocusCache::new();
            let mut word_buf = WordBuffer::new();

            while !stop_flag.load(Ordering::Acquire) {
                if hooks::is_paused() {
                    if !word_buf.is_empty() {
                        word_buf.reset();
                    }
                    global_ring().clear_and_zero();
                    thread::sleep(Duration::from_millis(WORKER_TICK_MS));
                    continue;
                }

                let status = focus.update(&blacklist);
                if status.hwnd_changed || status.is_immune {
                    if !word_buf.is_empty() {
                        word_buf.reset();
                    }
                }

                while let Some(rec) = global_ring().pop() {
                    if status.is_immune {
                        continue;
                    }
                    word_buf.feed(rec);
                }

                thread::sleep(Duration::from_millis(WORKER_TICK_MS));
            }

            word_buf.reset();
            global_ring().clear_and_zero();
        })
        .map_err(|_| windows::core::Error::from_win32())?;

    Ok(WorkerGuard {
        stop,
        join: Some(join),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_down(vk: u16, scan: u16, ms: u32) -> InputRecord {
        InputRecord {
            vk,
            scan_code: scan,
            flags: 0,
            ms_tick: ms,
            kind: InputKind::KeyDown,
            _pad: [0; 3],
        }
    }

    fn key_up(vk: u16, scan: u16, ms: u32) -> InputRecord {
        InputRecord {
            vk,
            scan_code: scan,
            flags: 0,
            ms_tick: ms,
            kind: InputKind::KeyUp,
            _pad: [0; 3],
        }
    }

    #[test]
    fn accumulates_keys_and_handles_backspace() {
        let mut buf = WordBuffer::new();
        buf.feed(key_down(0x47, 0x22, 100)); // 'g'
        buf.feed(key_down(0x48, 0x23, 150)); // 'h'
        buf.feed(key_down(0x42, 0x30, 200)); // 'b'
        assert_eq!(buf.len(), 3);

        buf.feed(key_down(VK_BACK, 0x0E, 240));
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.as_slice()[0].vk, 0x47);
        assert_eq!(buf.as_slice()[1].vk, 0x48);
        // Popped slot must be zeroed in RAM.
        let popped_slot = unsafe { ptr::read_volatile(&buf.slots[2]) };
        assert_eq!(popped_slot, KeyStrokeSlot::zeroed());
    }

    #[test]
    fn mouse_click_and_navigation_reset_and_zero() {
        let mut buf = WordBuffer::new();
        buf.feed(key_down(0x41, 0x1E, 100));
        buf.feed(key_down(0x42, 0x30, 120));
        assert_eq!(buf.len(), 2);

        buf.feed(InputRecord {
            vk: 0,
            scan_code: 0,
            flags: 0,
            ms_tick: 150,
            kind: InputKind::MouseClick,
            _pad: [0; 3],
        });
        assert!(buf.is_empty());
        let slot0 = unsafe { ptr::read_volatile(&buf.slots[0]) };
        assert_eq!(slot0, KeyStrokeSlot::zeroed());

        buf.feed(key_down(0x43, 0x2E, 200));
        assert_eq!(buf.len(), 1);
        buf.feed(key_down(VK_LEFT, 0x4B, 220));
        assert!(buf.is_empty());
    }

    #[test]
    fn modifier_chord_resets_and_ignores_chord_keys() {
        let mut buf = WordBuffer::new();
        buf.feed(key_down(0x41, 0x1E, 100));
        assert_eq!(buf.len(), 1);

        // Ctrl down resets immediately and ignores 'C' while held.
        buf.feed(key_down(VK_LCONTROL, 0x1D, 120));
        assert!(buf.is_empty());
        buf.feed(key_down(0x43, 0x2E, 140));
        assert!(buf.is_empty());
        buf.feed(key_up(VK_LCONTROL, 0x1D, 160));

        // Normal typing resumes after Ctrl release.
        buf.feed(key_down(0x44, 0x20, 200));
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.as_slice()[0].vk, 0x44);
    }

    #[test]
    fn idle_timeout_resets_before_new_key() {
        let mut buf = WordBuffer::new();
        buf.feed(key_down(0x41, 0x1E, 1000));
        buf.feed(key_down(0x42, 0x30, 1100));
        assert_eq!(buf.len(), 2);

        // 4000 ms gap > IDLE_RESET_MS (3500 ms) -> resets old word and starts fresh.
        buf.feed(key_down(0x43, 0x2E, 5101));
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.as_slice()[0].vk, 0x43);
    }
}
