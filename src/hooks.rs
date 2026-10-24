//! Dedicated low-level keyboard & mouse hook thread (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`).
//! Architectural invariants (AGENTS.md #3, #4):
//! - Hook callbacks do ONLY: check pause/suppression/injected flags, push fixed record
//!   to lock-free SPSC ring buffer, call `CallNextHookEx`.
//! - Zero allocations, zero locks, zero detection/audio/IO inside hook procedures.

use crate::ring::{global_ring, InputKind, InputRecord};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, KBDLLHOOKSTRUCT, LLKHF_INJECTED,
    LLKHF_LOWER_IL_INJECTED, LLMHF_INJECTED, LLMHF_LOWER_IL_INJECTED, MSLLHOOKSTRUCT, MSG,
    PeekMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    HC_ACTION, PM_NOREMOVE, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_MBUTTONDOWN, WM_QUIT, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

static PAUSED: AtomicBool = AtomicBool::new(false);
static SUPPRESS_DEPTH: AtomicU32 = AtomicU32::new(0);
static REAL_KEY_SEQ: AtomicU32 = AtomicU32::new(0);
static LAST_REAL_KEY_TICK: AtomicU32 = AtomicU32::new(0);

#[inline]
pub fn is_paused() -> bool {
    PAUSED.load(Ordering::Relaxed)
}

pub fn set_paused(paused: bool) {
    PAUSED.store(paused, Ordering::Release);
}

/// Toggle pause state and return the new state (`true` = paused).
pub fn toggle_paused() -> bool {
    let next = !is_paused();
    set_paused(next);
    next
}

#[allow(dead_code)]
#[inline]
pub fn begin_suppression() {
    SUPPRESS_DEPTH.fetch_add(1, Ordering::AcqRel);
}

#[allow(dead_code)]
#[inline]
pub fn end_suppression() {
    SUPPRESS_DEPTH.fetch_sub(1, Ordering::AcqRel);
}

/// Monotonic counter incremented on every real (non-injected) key-down event.
#[inline]
pub fn real_key_seq() -> u32 {
    REAL_KEY_SEQ.load(Ordering::Acquire)
}

/// `kb.time` tick (ms) of the most recent real (non-injected) key-down event.
#[allow(dead_code)]
#[inline]
pub fn last_real_key_tick() -> u32 {
    LAST_REAL_KEY_TICK.load(Ordering::Acquire)
}

unsafe extern "system" fn low_level_keyboard_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if ncode == HC_ACTION as i32 && !PAUSED.load(Ordering::Relaxed) && lparam.0 != 0 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let injected_mask = LLKHF_INJECTED.0 | LLKHF_LOWER_IL_INJECTED.0;
        if (kb.flags.0 & injected_mask) == 0 {
            let msg = wparam.0 as u32;
            let kind = match msg {
                WM_KEYDOWN | WM_SYSKEYDOWN => InputKind::KeyDown,
                WM_KEYUP | WM_SYSKEYUP => InputKind::KeyUp,
                _ => InputKind::Empty,
            };
            if kind != InputKind::Empty {
                if kind == InputKind::KeyDown {
                    REAL_KEY_SEQ.fetch_add(1, Ordering::Release);
                    LAST_REAL_KEY_TICK.store(kb.time, Ordering::Release);
                }
                let _ = global_ring().push(InputRecord {
                    vk: kb.vkCode as u16,
                    scan_code: kb.scanCode as u16,
                    flags: kb.flags.0,
                    ms_tick: kb.time,
                    kind,
                    _pad: [0; 3],
                });
            }
        }
    }
    CallNextHookEx(None, ncode, wparam, lparam)
}

unsafe extern "system" fn low_level_mouse_proc(
    ncode: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if ncode == HC_ACTION as i32 && !PAUSED.load(Ordering::Relaxed) && lparam.0 != 0 {
        let msg = wparam.0 as u32;
        if msg == WM_LBUTTONDOWN || msg == WM_RBUTTONDOWN || msg == WM_MBUTTONDOWN {
            let ms = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            let injected_mask = LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED;
            if (ms.flags & injected_mask) == 0 {
                let _ = global_ring().push(InputRecord {
                    vk: 0,
                    scan_code: 0,
                    flags: ms.flags,
                    ms_tick: ms.time,
                    kind: InputKind::MouseClick,
                    _pad: [0; 3],
                });
            }
        }
    }
    CallNextHookEx(None, ncode, wparam, lparam)
}

pub struct HookGuard {
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        if self.thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
        global_ring().clear_and_zero();
    }
}

/// Spawn the dedicated hook thread and install `WH_KEYBOARD_LL` + `WH_MOUSE_LL`.
pub fn start() -> windows::core::Result<HookGuard> {
    let (tx, rx) = mpsc::channel::<windows::core::Result<u32>>();
    let join = thread::Builder::new()
        .name("smartblink-hooks".into())
        .spawn(move || unsafe {
            let mut msg = MSG::default();
            // Force message queue creation before publishing thread ID.
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            let tid = GetCurrentThreadId();

            let kbd_hook = match SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(low_level_keyboard_proc),
                None,
                0,
            ) {
                Ok(h) => h,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            };

            let mouse_hook =
                match SetWindowsHookExW(WH_MOUSE_LL, Some(low_level_mouse_proc), None, 0) {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = UnhookWindowsHookEx(kbd_hook);
                        let _ = tx.send(Err(e));
                        return;
                    }
                };

            let _ = tx.send(Ok(tid));

            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let _ = UnhookWindowsHookEx(mouse_hook);
            let _ = UnhookWindowsHookEx(kbd_hook);
        })
        .map_err(|_| windows::core::Error::from_win32())?;

    match rx.recv() {
        Ok(Ok(thread_id)) => Ok(HookGuard {
            thread_id,
            join: Some(join),
        }),
        Ok(Err(e)) => {
            let _ = join.join();
            Err(e)
        }
        Err(_) => {
            let _ = join.join();
            Err(windows::core::Error::from_win32())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_and_stop_hooks_and_pause_toggle() {
        set_paused(false);
        assert!(!is_paused());
        assert!(toggle_paused());
        assert!(is_paused());
        assert!(!toggle_paused());
        assert!(!is_paused());

        let guard = start().expect("hooks::start must succeed");
        drop(guard);
    }
}

