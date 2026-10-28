//! Detector worker thread (30 Hz tick) and RAM-only word scan-code buffer.
//! Architectural invariants (AGENTS.md #2, #7):
//! - Keystrokes never leave RAM; `WordBuffer` volatile-zeroes all slots on reset and drop.
//! - Hard reset on mouse click, navigation keys, modifier chords, idle timeout (3.5 s),
//!   focus change, or immune foreground process.

use crate::clipboard::{self, SelectionHotkeyDetector, SessionWhitelist};
use crate::config::{FixTrigger, LedMode, NotifyMode};
use crate::fix::{self, FixGestureDetector};
use crate::hooks;
use crate::immunity::FocusCache;
use crate::layout::{self, LayoutCache};
use crate::profile::{self, DetectionVerdict};
use crate::ring::{global_ring, InputKind, InputRecord};
use crate::signal::{self, SignalEngine};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const MAX_WORD_KEYS: usize = 256;
pub const IDLE_RESET_MS: u32 = 60_000;
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
const VK_SPACE: u16 = 0x20;
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
    pub shift: bool,
    pub altgr: bool,
}

impl KeyStrokeSlot {
    pub const fn zeroed() -> Self {
        Self {
            vk: 0,
            scan_code: 0,
            flags: 0,
            ms_tick: 0,
            shift: false,
            altgr: false,
        }
    }
}

const LLKHF_EXTENDED_FLAG: u32 = 0x01;
const ALTGR_SYNTHETIC_WINDOW_MS: u32 = 50;

pub struct WordBuffer {
    slots: [KeyStrokeSlot; MAX_WORD_KEYS],
    len: usize,
    last_tick: Option<u32>,
    lshift_down: bool,
    rshift_down: bool,
    lctrl_down: bool,
    rctrl_down: bool,
    lalt_down: bool,
    ralt_down: bool,
    win_down: bool,
    lctrl_down_tick: Option<u32>,
    altgr_backup: [KeyStrokeSlot; MAX_WORD_KEYS],
    altgr_backup_len: usize,
    altgr_backup_tick: Option<u32>,
}

impl Default for WordBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WordBuffer {
    fn drop(&mut self) {
        self.reset();
        self.clear_altgr_backup();
    }
}

impl WordBuffer {
    pub const fn new() -> Self {
        Self {
            slots: [KeyStrokeSlot::zeroed(); MAX_WORD_KEYS],
            len: 0,
            last_tick: None,
            lshift_down: false,
            rshift_down: false,
            lctrl_down: false,
            rctrl_down: false,
            lalt_down: false,
            ralt_down: false,
            win_down: false,
            lctrl_down_tick: None,
            altgr_backup: [KeyStrokeSlot::zeroed(); MAX_WORD_KEYS],
            altgr_backup_len: 0,
            altgr_backup_tick: None,
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

    fn clear_altgr_backup(&mut self) {
        for slot in &mut self.altgr_backup[..self.altgr_backup_len] {
            unsafe {
                ptr::write_volatile(slot, KeyStrokeSlot::zeroed());
            }
        }
        self.altgr_backup_len = 0;
        self.altgr_backup_tick = None;
        self.lctrl_down_tick = None;
    }

    /// Clear tracked modifier flags (called on focus change, mouse click, or synthetic injection).
    pub fn clear_modifiers(&mut self) {
        self.lshift_down = false;
        self.rshift_down = false;
        self.lctrl_down = false;
        self.rctrl_down = false;
        self.lalt_down = false;
        self.ralt_down = false;
        self.win_down = false;
        self.clear_altgr_backup();
    }

    /// Drop the oldest word (up to the first `VK_SPACE`) so a long multi-word sentence never wipes
    /// the current word mid-typing when `MAX_WORD_KEYS` is reached.
    fn drop_oldest_word_or_reset(&mut self) {
        if let Some(space_pos) = self.slots[..self.len].iter().position(|s| s.vk == VK_SPACE) {
            let drop_count = (space_pos + 1).min(self.len);
            let new_len = self.len - drop_count;
            for i in 0..new_len {
                self.slots[i] = self.slots[i + drop_count];
            }
            for i in new_len..self.len {
                unsafe {
                    ptr::write_volatile(&mut self.slots[i], KeyStrokeSlot::zeroed());
                }
            }
            self.len = new_len;
        } else {
            self.reset();
        }
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
                self.clear_modifiers();
            }
            InputKind::KeyUp => {
                match rec.vk {
                    VK_LSHIFT => self.lshift_down = false,
                    VK_RSHIFT => self.rshift_down = false,
                    VK_SHIFT => {
                        self.lshift_down = false;
                        self.rshift_down = false;
                    }
                    VK_LCONTROL => {
                        self.lctrl_down = false;
                        self.clear_altgr_backup();
                    }
                    VK_RCONTROL => self.rctrl_down = false,
                    VK_CONTROL => {
                        if (rec.flags & LLKHF_EXTENDED_FLAG) != 0 {
                            self.rctrl_down = false;
                        } else {
                            self.lctrl_down = false;
                            self.clear_altgr_backup();
                        }
                    }
                    VK_LMENU => self.lalt_down = false,
                    VK_RMENU => self.ralt_down = false,
                    VK_MENU => {
                        if (rec.flags & LLKHF_EXTENDED_FLAG) != 0 {
                            self.ralt_down = false;
                        } else {
                            self.lalt_down = false;
                        }
                    }
                    VK_LWIN | VK_RWIN => self.win_down = false,
                    _ => {}
                }
            }
            InputKind::KeyDown => {
                match rec.vk {
                    VK_LSHIFT => {
                        self.lshift_down = true;
                        return;
                    }
                    VK_RSHIFT => {
                        self.rshift_down = true;
                        return;
                    }
                    VK_SHIFT => {
                        self.lshift_down = true;
                        return;
                    }
                    VK_RMENU => {
                        if !self.ralt_down {
                            self.ralt_down = true;
                            if self.altgr_backup_len > 0 {
                                let within_window = self
                                    .lctrl_down_tick
                                    .map(|t| rec.ms_tick.wrapping_sub(t) <= ALTGR_SYNTHETIC_WINDOW_MS)
                                    .unwrap_or(false);
                                if within_window {
                                    let n = self.altgr_backup_len;
                                    self.slots[..n].copy_from_slice(&self.altgr_backup[..n]);
                                    self.len = n;
                                    self.last_tick = self.altgr_backup_tick;
                                }
                                self.clear_altgr_backup();
                            }
                        }
                        return;
                    }
                    VK_MENU if (rec.flags & LLKHF_EXTENDED_FLAG) != 0 => {
                        if !self.ralt_down {
                            self.ralt_down = true;
                            if self.altgr_backup_len > 0 {
                                let within_window = self
                                    .lctrl_down_tick
                                    .map(|t| rec.ms_tick.wrapping_sub(t) <= ALTGR_SYNTHETIC_WINDOW_MS)
                                    .unwrap_or(false);
                                if within_window {
                                    let n = self.altgr_backup_len;
                                    self.slots[..n].copy_from_slice(&self.altgr_backup[..n]);
                                    self.len = n;
                                    self.last_tick = self.altgr_backup_tick;
                                }
                                self.clear_altgr_backup();
                            }
                        }
                        return;
                    }
                    VK_LCONTROL => {
                        if self.ralt_down {
                            self.lctrl_down = true;
                            return;
                        }
                        if !self.lctrl_down
                            && !self.rctrl_down
                            && !self.lalt_down
                            && !self.win_down
                            && self.len > 0
                        {
                            self.altgr_backup[..self.len].copy_from_slice(&self.slots[..self.len]);
                            self.altgr_backup_len = self.len;
                            self.altgr_backup_tick = self.last_tick;
                            self.lctrl_down_tick = Some(rec.ms_tick);
                        }
                        self.lctrl_down = true;
                        self.reset();
                        return;
                    }
                    VK_CONTROL if (rec.flags & LLKHF_EXTENDED_FLAG) == 0 => {
                        if self.ralt_down {
                            self.lctrl_down = true;
                            return;
                        }
                        if !self.lctrl_down
                            && !self.rctrl_down
                            && !self.lalt_down
                            && !self.win_down
                            && self.len > 0
                        {
                            self.altgr_backup[..self.len].copy_from_slice(&self.slots[..self.len]);
                            self.altgr_backup_len = self.len;
                            self.altgr_backup_tick = self.last_tick;
                            self.lctrl_down_tick = Some(rec.ms_tick);
                        }
                        self.lctrl_down = true;
                        self.reset();
                        return;
                    }
                    VK_RCONTROL | VK_CONTROL => {
                        self.rctrl_down = true;
                        self.clear_altgr_backup();
                        self.reset();
                        return;
                    }
                    VK_LMENU | VK_MENU => {
                        self.lalt_down = true;
                        self.clear_altgr_backup();
                        self.reset();
                        return;
                    }
                    VK_LWIN | VK_RWIN => {
                        self.win_down = true;
                        self.clear_altgr_backup();
                        self.reset();
                        return;
                    }
                    _ => {}
                }

                self.clear_altgr_backup();

                // Ignore pure lock/pause keys for word buffer content.
                if is_non_text_toggle_or_shift(rec.vk) {
                    return;
                }

                // Idle timeout check before checking stuck modifiers or recording.
                if let Some(prev) = self.last_tick {
                    if rec.ms_tick.wrapping_sub(prev) > IDLE_RESET_MS {
                        self.reset();
                        self.clear_modifiers();
                    }
                }

                let altgr_active = self.ralt_down && !self.lalt_down && !self.rctrl_down && !self.win_down;
                let non_altgr_mod = (self.lctrl_down && !self.ralt_down)
                    || self.rctrl_down
                    || self.lalt_down
                    || self.win_down;

                // Any chord while non-AltGr Ctrl, Left Alt, or Win is held resets and stays ignored.
                if non_altgr_mod {
                    self.reset();
                    return;
                }

                // Hard reset on navigation, escape, enter, tab, function keys.
                if is_navigation_or_reset_vk(rec.vk) {
                    self.reset();
                    return;
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

                // Append to word buffer; if capacity is reached, drop oldest word so current sentence continues.
                if self.len >= MAX_WORD_KEYS {
                    self.drop_oldest_word_or_reset();
                }
                self.slots[self.len] = KeyStrokeSlot {
                    vk: rec.vk,
                    scan_code: rec.scan_code,
                    flags: rec.flags,
                    ms_tick: rec.ms_tick,
                    shift: self.lshift_down || self.rshift_down,
                    altgr: altgr_active,
                };
                self.len += 1;
                self.last_tick = Some(rec.ms_tick);
            }
        }
    }
}

fn is_ctrl_vk(vk: u16) -> bool {
    matches!(vk, VK_CONTROL | VK_LCONTROL | VK_RCONTROL)
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
            | 0x2C // VK_SNAPSHOT (PrintScreen)
            | VK_INSERT
            | VK_DELETE
            | 0x5D // VK_APPS (Context Menu)
            | 0x70..=0x87 // F1..F24
            | 0xA6..=0xB7 // Browser & Media keys
    )
}

fn zero_and_clear_vec(v: &mut Vec<KeyStrokeSlot>) {
    for slot in v.iter_mut() {
        unsafe {
            ptr::write_volatile(slot, KeyStrokeSlot::zeroed());
        }
    }
    v.clear();
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

pub fn start(
    mut blacklist: Vec<String>,
    notify_mode: NotifyMode,
    led_mode: LedMode,
    mut fix_trigger: FixTrigger,
) -> windows::core::Result<WorkerGuard> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);

    let join = thread::Builder::new()
        .name("smartblink-worker".into())
        .spawn(move || {
            let mut focus = FocusCache::new();
            let mut word_buf = WordBuffer::new();
            let mut ctrl_snapshot: Vec<KeyStrokeSlot> = Vec::new();
            let mut fix_detector = FixGestureDetector::new(fix_trigger);
            let mut selection_detector = SelectionHotkeyDetector::new();
            let mut whitelist = SessionWhitelist::new();
            let mut layout_cache = LayoutCache::new();
            let mut active_hkls = layout::active_layouts();
            for &hkl in &active_hkls {
                let _ = layout_cache.get_or_build(hkl);
            }
            let mut signal = SignalEngine::start(notify_mode, led_mode);
            let mut last_verdict: Option<DetectionVerdict> = None;
            let mut last_undo_fix: Option<(fix::FixPlan, isize, Instant)> = None;
            let mut word_blinked = false;
            let mut pending_caps_blink = false;
            let mut word_alerted = false;
            let mut last_key_instant: Option<Instant> = None;
            let mut last_typing_hwnd: isize = 0;
            let mut seen_cfg_rev = crate::config::config_revision();

            while !stop_flag.load(Ordering::Acquire) {
                let cur_rev = crate::config::config_revision();
                if cur_rev != seen_cfg_rev {
                    seen_cfg_rev = cur_rev;
                    let live = crate::config::get_runtime_config();
                    blacklist = live.blacklist;
                    fix_trigger = live.fix_trigger;
                    fix_detector.set_trigger(fix_trigger);
                    signal.set_notify_mode(live.notify_mode);
                    signal.set_led_mode(live.led_mode);
                    focus.invalidate();
                }
                if hooks::is_paused() {
                    if !word_buf.is_empty() {
                        word_buf.reset();
                    }
                    word_buf.clear_modifiers();
                    zero_and_clear_vec(&mut ctrl_snapshot);
                    fix_detector.reset();
                    selection_detector.reset();
                    if last_verdict.is_some() {
                        last_verdict = None;
                    }
                    last_undo_fix = None;
                    word_blinked = false;
                    pending_caps_blink = false;
                    word_alerted = false;
                    last_key_instant = None;
                    last_typing_hwnd = 0;
                    global_ring().clear_and_zero();
                    thread::sleep(Duration::from_millis(WORKER_TICK_MS));
                    continue;
                }

                // Active idle timeout: wipe stale buffer if user has not typed for IDLE_RESET_MS.
                if let Some(t_last) = last_key_instant {
                    if t_last.elapsed() > Duration::from_millis(IDLE_RESET_MS as u64) {
                        word_buf.reset();
                        word_buf.clear_modifiers();
                        zero_and_clear_vec(&mut ctrl_snapshot);
                        last_verdict = None;
                        last_undo_fix = None;
                        word_blinked = false;
                        pending_caps_blink = false;
                        word_alerted = false;
                        last_key_instant = None;
                    }
                }

                let mut status = focus.update(&blacklist);
                if status.hwnd_changed {
                    let refreshed = layout::active_layouts();
                    if !refreshed.is_empty() {
                        for &hkl in &refreshed {
                            let _ = layout_cache.get_or_build(hkl);
                        }
                        active_hkls = refreshed;
                    }
                }

                let mut had_typing_key = false;
                while let Some(rec) = global_ring().pop() {
                    status = focus.update(&blacklist);
                    let cur_hwnd_raw = focus.current_hwnd().0 as isize;
                    if status.is_immune
                        || (cur_hwnd_raw != 0
                            && last_typing_hwnd != 0
                            && cur_hwnd_raw != last_typing_hwnd)
                    {
                        if !word_buf.is_empty() {
                            word_buf.reset();
                        }
                        word_buf.clear_modifiers();
                        zero_and_clear_vec(&mut ctrl_snapshot);
                        fix_detector.reset();
                        selection_detector.reset();
                        last_verdict = None;
                        last_undo_fix = None;
                        word_blinked = false;
                        pending_caps_blink = false;
                        word_alerted = false;
                        last_key_instant = None;
                    }
                    if cur_hwnd_raw != 0 && !status.is_immune {
                        last_typing_hwnd = cur_hwnd_raw;
                    }

                    if rec.kind == InputKind::MouseClick
                        || (rec.kind == InputKind::KeyDown && rec.vk == VK_TAB)
                    {
                        // Focus within the same HWND may have moved into or out of a password box.
                        focus.invalidate();
                        last_undo_fix = None;
                    }
                    if status.is_immune {
                        continue;
                    }

                    // Stubborn user whitelist (ARCHITECTURE.md §3.7): if an alert was raised for the
                    // current word and the user presses Space or Enter without fixing, increment ignore count.
                    if word_alerted
                        && rec.kind == InputKind::KeyDown
                        && (rec.vk == VK_SPACE || rec.vk == VK_RETURN)
                    {
                        let active_word = layout::active_word_slice(word_buf.as_slice());
                        if active_word.len() >= 2 {
                            let cur_hkl = layout::window_layout(focus.current_hwnd());
                            let (mut rendered, trailing_dead) = layout_cache
                                .get_or_build(cur_hkl)
                                .render_keys_raw(active_word, false);
                            if let Some(d) = trailing_dead {
                                rendered.push(d);
                            }
                            if !rendered.is_empty() {
                                whitelist.record_ignored_word(&rendered, cur_hkl);
                            }
                        }
                        word_alerted = false;
                        word_blinked = false;
                        last_verdict = None;
                    }

                    // Preserve a snapshot before bare Left Ctrl down when DoubleLeftCtrl is the trigger.
                    if fix_trigger == FixTrigger::DoubleLeftCtrl {
                        if rec.kind == InputKind::KeyDown && is_ctrl_vk(rec.vk) {
                            if !word_buf.is_empty() {
                                zero_and_clear_vec(&mut ctrl_snapshot);
                                ctrl_snapshot.extend_from_slice(word_buf.as_slice());
                            }
                        } else if rec.kind == InputKind::KeyDown && !is_ctrl_vk(rec.vk) {
                            zero_and_clear_vec(&mut ctrl_snapshot);
                        }
                    }

                    let selection_triggered = selection_detector.feed(rec);
                    let fix_triggered = fix_detector.feed(rec);
                    let prev_len = word_buf.len();
                    word_buf.feed(rec);

                    if selection_triggered && !focus.refresh_password_immunity() {
                        let hwnd = focus.current_hwnd();
                        let cur_hkl = layout::window_layout(hwnd);
                        let _ = clipboard::convert_selection_in_window(
                            hwnd,
                            cur_hkl,
                            &active_hkls,
                            &mut layout_cache,
                        );
                        word_buf.reset();
                        word_buf.clear_modifiers();
                        selection_detector.reset();
                        zero_and_clear_vec(&mut ctrl_snapshot);
                        last_verdict = None;
                        last_undo_fix = None;
                        word_blinked = false;
                        pending_caps_blink = false;
                        word_alerted = false;
                        last_key_instant = None;
                        had_typing_key = false;
                        continue;
                    }

                    if fix_triggered && !focus.refresh_password_immunity() {
                        let slots = if !word_buf.is_empty() {
                            word_buf.as_slice()
                        } else {
                            ctrl_snapshot.as_slice()
                        };
                        let hwnd = focus.current_hwnd();
                        let cur_hkl = layout::window_layout(hwnd);
                        if !slots.is_empty() {
                            if let Some(plan) = fix::plan_word_fix_cached(
                                slots,
                                cur_hkl,
                                &active_hkls,
                                &mut layout_cache,
                            ) {
                                if fix::execute_fix(&plan, hwnd) {
                                    let undo_plan = fix::FixPlan {
                                        current_hkl: plan.target_hkl,
                                        target_hkl: plan.current_hkl,
                                        backspace_count: plan.replacement_text.chars().count(),
                                        original_text: plan.replacement_text,
                                        replacement_text: plan.original_text,
                                    };
                                    last_undo_fix = Some((undo_plan, hwnd.0 as isize, Instant::now()));
                                }
                            }
                        } else if let Some((undo_plan, undo_hwnd, undo_time)) = last_undo_fix.take() {
                            if undo_hwnd == hwnd.0 as isize
                                && undo_time.elapsed() <= Duration::from_secs(10)
                            {
                                let _ = fix::execute_fix(&undo_plan, hwnd);
                            }
                        }
                        word_buf.reset();
                        word_buf.clear_modifiers();
                        selection_detector.reset();
                        zero_and_clear_vec(&mut ctrl_snapshot);
                        last_verdict = None;
                        word_blinked = false;
                        pending_caps_blink = false;
                        word_alerted = false;
                        last_key_instant = None;
                        had_typing_key = false;
                        continue;
                    }

                    if word_buf.is_empty() {
                        word_blinked = false;
                        pending_caps_blink = false;
                        word_alerted = false;
                    } else if rec.kind == InputKind::KeyDown && word_buf.len() != prev_len {
                        had_typing_key = true;
                        last_key_instant = Some(Instant::now());
                        last_undo_fix = None;
                    }
                }

                if had_typing_key && !status.is_immune {
                    let active_word = layout::active_word_slice(word_buf.as_slice());
                    if active_word.is_empty() {
                        last_verdict = None;
                        word_blinked = false;
                        word_alerted = false;
                    } else if active_word.len() >= 2 {
                        let cur_hkl = layout::window_layout(focus.current_hwnd());
                        let (mut cur_rendered, cur_dead) = layout_cache
                            .get_or_build(cur_hkl)
                            .render_keys_raw(active_word, false);
                        if let Some(d) = cur_dead {
                            cur_rendered.push(d);
                        }
                        let is_whitelisted =
                            !cur_rendered.is_empty() && whitelist.is_whitelisted(&cur_rendered, cur_hkl);
                        if is_whitelisted {
                            last_verdict = None;
                        } else {
                            last_verdict = profile::evaluate_active_word(
                                active_word,
                                cur_hkl,
                                &active_hkls,
                                &mut layout_cache,
                            );
                        }
                        if last_verdict.is_some() {
                            if focus.refresh_password_immunity() {
                                word_buf.reset();
                                last_verdict = None;
                                word_blinked = false;
                                pending_caps_blink = false;
                                word_alerted = false;
                            } else {
                                word_alerted = true;
                                signal.trigger_click();
                                if !word_blinked {
                                    if signal.led_mode() == LedMode::ScrollLock {
                                        signal.trigger_blink();
                                        word_blinked = true;
                                    } else {
                                        pending_caps_blink = true;
                                    }
                                }
                            }
                        }
                    } else {
                        last_verdict = None;
                    }
                }

                // CapsLockGuarded mode: trigger pending blink when in a typing gap >= 80 ms
                // (even if the user just pressed Space at the end of the word).
                if pending_caps_blink
                    && signal.led_mode() == LedMode::CapsLockGuarded
                    && !status.is_immune
                {
                    if let Some(t_last) = last_key_instant {
                        if t_last.elapsed() >= Duration::from_millis(signal::CAPS_MIN_GAP_MS as u64)
                            && !focus.refresh_password_immunity()
                        {
                            signal.trigger_blink();
                            word_blinked = true;
                            pending_caps_blink = false;
                        }
                    }
                }

                thread::sleep(Duration::from_millis(WORKER_TICK_MS));
            }

            word_buf.reset();
            zero_and_clear_vec(&mut ctrl_snapshot);
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

        // Gap > IDLE_RESET_MS (60_000 ms) -> resets old word and starts fresh.
        buf.feed(key_down(0x43, 0x2E, 1100 + IDLE_RESET_MS + 100));
        assert_eq!(buf.len(), 1);
        assert_eq!(buf.as_slice()[0].vk, 0x43);
    }

    #[test]
    fn altgr_preserves_word_and_marks_altgr_slot() {
        let mut buf = WordBuffer::new();
        // Type "mo"
        buf.feed(key_down(0x4D, 0x32, 100));
        buf.feed(key_down(0x4F, 0x18, 120));
        assert_eq!(buf.len(), 2);

        // Windows AltGr sequence: synthetic VK_LCONTROL followed immediately by VK_RMENU
        buf.feed(key_down(VK_LCONTROL, 0x1D, 140));
        buf.feed(InputRecord {
            vk: VK_RMENU,
            scan_code: 0x38,
            flags: LLKHF_EXTENDED_FLAG,
            ms_tick: 141,
            kind: InputKind::KeyDown,
            _pad: [0; 3],
        });
        assert_eq!(buf.len(), 2, "AltGr must restore the word buffer");

        // Press 'Z' while AltGr is held (Polish 'ż')
        buf.feed(key_down(0x5A, 0x2C, 160));
        assert_eq!(buf.len(), 3);
        assert!(buf.as_slice()[2].altgr);

        // Release AltGr and type 'e'
        buf.feed(key_up(VK_LCONTROL, 0x1D, 180));
        buf.feed(InputRecord {
            vk: VK_RMENU,
            scan_code: 0x38,
            flags: LLKHF_EXTENDED_FLAG,
            ms_tick: 181,
            kind: InputKind::KeyUp,
            _pad: [0; 3],
        });
        buf.feed(key_down(0x45, 0x12, 200));
        assert_eq!(buf.len(), 4);
        assert!(!buf.as_slice()[3].altgr);
    }
}
