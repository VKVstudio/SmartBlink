#![allow(dead_code)]
//! Manual one-gesture fix detector and executor (M6).
//! Architectural invariants (AGENTS.md #1, #4; ARCHITECTURE.md §3.1, §3.5):
//! - Never mutate user text without an explicit fix gesture (`DoubleLeftShift`, `DoubleLeftCtrl`, `PauseBreak`).
//! - Double-tap window <= 300 ms; the two taps must be pure modifier presses with no other key between them.
//! - Key auto-repeat while holding a modifier never counts as a second tap.
//! - Backspace count uses character length of captured source text; replacement is sent via `KEYEVENTF_UNICODE`
//!   inside a `hooks::begin_suppression()` / `end_suppression()` window, followed by target layout switch.

use crate::config::FixTrigger;
use crate::hooks;
use crate::layout::{self, LayoutCache, LayoutLayerTable};
use crate::profile;
use crate::ring::{InputKind, InputRecord};
use crate::worker::KeyStrokeSlot;
use std::mem::size_of;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ActivateKeyboardLayout, SendInput, ACTIVATE_KEYBOARD_LAYOUT_FLAGS, HKL, INPUT, INPUT_0,
    INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, LLKHF_EXTENDED, WM_INPUTLANGCHANGEREQUEST,
};

pub const DOUBLE_TAP_WINDOW_MS: u32 = 300;

const VK_BACK: u16 = 0x08;
const SCAN_BACK: u16 = 0x0E;
const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_PAUSE: u16 = 0x13;
const VK_SPACE: u16 = 0x20;
const VK_LSHIFT: u16 = 0xA0;
const VK_LCONTROL: u16 = 0xA2;
const SCAN_LSHIFT: u16 = 0x2A;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixPlan {
    pub current_hkl: HKL,
    pub target_hkl: HKL,
    pub original_text: String,
    pub replacement_text: String,
    pub backspace_count: usize,
}

/// Stateful detector for the configured manual fix gesture (`ARCHITECTURE.md` §3.5).
#[derive(Debug, Clone)]
pub struct FixGestureDetector {
    trigger: FixTrigger,
    mod_down: bool,
    tainted_while_down: bool,
    first_tap_up_tick: Option<u32>,
}

impl FixGestureDetector {
    pub const fn new(trigger: FixTrigger) -> Self {
        Self {
            trigger,
            mod_down: false,
            tainted_while_down: false,
            first_tap_up_tick: None,
        }
    }

    pub fn reset(&mut self) {
        self.mod_down = false;
        self.tainted_while_down = false;
        self.first_tap_up_tick = None;
    }

    pub fn set_trigger(&mut self, trigger: FixTrigger) {
        if self.trigger != trigger {
            self.trigger = trigger;
            self.reset();
        }
    }

    #[inline]
    fn matches_target_modifier(&self, rec: &InputRecord) -> bool {
        match self.trigger {
            FixTrigger::DoubleLeftShift => {
                rec.vk == VK_LSHIFT || (rec.vk == VK_SHIFT && (rec.scan_code & 0xFF) == SCAN_LSHIFT)
            }
            FixTrigger::DoubleLeftCtrl => {
                rec.vk == VK_LCONTROL
                    || (rec.vk == VK_CONTROL && (rec.flags & LLKHF_EXTENDED.0) == 0)
            }
            FixTrigger::PauseBreak => false,
        }
    }

    /// Feed one input event; returns `true` at the moment the fix gesture completes.
    pub fn feed(&mut self, rec: InputRecord) -> bool {
        match rec.kind {
            InputKind::Empty => false,
            InputKind::MouseClick => {
                self.reset();
                false
            }
            InputKind::KeyDown => {
                if self.trigger == FixTrigger::PauseBreak {
                    if rec.vk == VK_PAUSE {
                        self.reset();
                        return true;
                    }
                    return false;
                }

                if self.matches_target_modifier(&rec) {
                    // Ignore key auto-repeat while the modifier is already held down.
                    if self.mod_down {
                        return false;
                    }
                    self.mod_down = true;
                    self.tainted_while_down = false;
                    if let Some(prev_up) = self.first_tap_up_tick {
                        if rec.ms_tick.wrapping_sub(prev_up) > DOUBLE_TAP_WINDOW_MS {
                            // Previous tap expired; this press starts a fresh first tap.
                            self.first_tap_up_tick = None;
                        }
                    }
                    false
                } else {
                    // Any other key pressed while modifier is held or between taps cancels double-tap.
                    self.tainted_while_down = true;
                    self.first_tap_up_tick = None;
                    false
                }
            }
            InputKind::KeyUp => {
                if self.trigger == FixTrigger::PauseBreak {
                    return false;
                }

                if self.matches_target_modifier(&rec) {
                    if !self.mod_down {
                        return false;
                    }
                    self.mod_down = false;
                    if self.tainted_while_down {
                        self.tainted_while_down = false;
                        self.first_tap_up_tick = None;
                        return false;
                    }

                    if let Some(prev_up) = self.first_tap_up_tick {
                        if rec.ms_tick.wrapping_sub(prev_up) <= DOUBLE_TAP_WINDOW_MS {
                            self.first_tap_up_tick = None;
                            return true;
                        }
                    }

                    self.first_tap_up_tick = Some(rec.ms_tick);
                    false
                } else {
                    false
                }
            }
        }
    }
}

/// Compute a `FixPlan` for the last word run in `word_buf_slots` across `tables`.
pub fn plan_word_fix(
    word_buf_slots: &[KeyStrokeSlot],
    current_hkl: HKL,
    tables: &[&LayoutLayerTable],
) -> Option<FixPlan> {
    if tables.len() < 2 {
        return None;
    }
    let run = layout::last_word_run_for_fix(word_buf_slots);
    if run.is_empty() {
        return None;
    }

    let current_table = tables
        .iter()
        .copied()
        .find(|t| t.hkl == current_hkl)
        .or_else(|| {
            let cur_lang = layout::hkl_primary_lang(current_hkl);
            tables.iter().copied().find(|t| t.primary_lang == cur_lang)
        })?;

    let original_text = current_table.render_keys(run)?;
    if original_text.trim().is_empty() {
        return None;
    }

    // Strip trailing spaces for language scoring while keeping the full `run` for replacement.
    let word_end = run
        .iter()
        .rposition(|k| k.vk != VK_SPACE)
        .map(|i| i + 1)
        .unwrap_or(0);
    let word_only = &run[..word_end];
    if word_only.is_empty() {
        return None;
    }

    // 1. Prefer the full M4 scorer verdict if available.
    // 2. Otherwise (e.g. user explicitly fixes a short 1..3 char token), pick the highest-scoring
    //    alternative layout in the user's installed set.
    let target_table = if let Some(verdict) =
        profile::evaluate_tables(word_only, current_table.hkl, tables)
    {
        tables
            .iter()
            .copied()
            .find(|t| t.hkl == verdict.target_hkl)?
    } else {
        let mut best_alt: Option<(&LayoutLayerTable, i32)> = None;
        for &table in tables {
            if table.hkl == current_table.hkl {
                continue;
            }
            let Some(rendered_word) = table.render_keys(word_only) else {
                continue;
            };
            let score = profile::profile_for_primary_lang(table.primary_lang)
                .map(|p| p.score_word(&rendered_word))
                .unwrap_or(0);
            if best_alt.map_or(true, |(_, b_score)| score > b_score) {
                best_alt = Some((table, score));
            }
        }
        best_alt?.0
    };

    let replacement_text = target_table.render_keys(run)?;
    if replacement_text.is_empty() || replacement_text == original_text {
        return None;
    }

    let backspace_count = original_text.chars().count();
    Some(FixPlan {
        current_hkl: current_table.hkl,
        target_hkl: target_table.hkl,
        original_text,
        replacement_text,
        backspace_count,
    })
}

/// Plan a fix using `LayoutCache` for the session's active `HKL`s.
pub fn plan_word_fix_cached(
    word_buf_slots: &[KeyStrokeSlot],
    current_hkl: HKL,
    active_hkls: &[HKL],
    cache: &mut LayoutCache,
) -> Option<FixPlan> {
    if active_hkls.len() < 2 {
        return None;
    }
    for &hkl in active_hkls {
        let _ = cache.get_or_build(hkl);
    }
    let _ = cache.get_or_build(current_hkl);
    let tables: Vec<&LayoutLayerTable> = active_hkls
        .iter()
        .filter_map(|hkl| cache.get(*hkl))
        .collect();
    plan_word_fix(word_buf_slots, current_hkl, &tables)
}

/// Build the exact `SendInput` sequence: `backspace_count` `VK_BACK` key taps followed by
/// `KEYEVENTF_UNICODE` UTF-16 code units for `replacement_text`.
pub fn build_fix_inputs(backspace_count: usize, replacement_text: &str) -> Vec<INPUT> {
    let utf16_len = replacement_text.encode_utf16().count();
    let mut inputs = Vec::with_capacity((backspace_count + utf16_len) * 2);

    for _ in 0..backspace_count {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(VK_BACK),
                    wScan: SCAN_BACK,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(VK_BACK),
                    wScan: SCAN_BACK,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }

    for unit in replacement_text.encode_utf16() {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }

    inputs
}

/// Request keyboard layout switch in the target window and current thread (`ARCHITECTURE.md` §3.1).
pub fn switch_window_layout(hwnd: HWND, target_hkl: HKL) {
    unsafe {
        let _ = ActivateKeyboardLayout(target_hkl, ACTIVATE_KEYBOARD_LAYOUT_FLAGS(0));
        if !hwnd.0.is_null() {
            let _ = PostMessageW(
                hwnd,
                WM_INPUTLANGCHANGEREQUEST,
                WPARAM(0),
                LPARAM(target_hkl.0 as isize),
            );
        }
    }
}

/// Execute a `FixPlan`: inject backspaces + `KEYEVENTF_UNICODE` characters inside a hook
/// suppression window and switch the focused window to `plan.target_hkl`.
pub fn execute_fix(plan: &FixPlan, target_hwnd: HWND) -> bool {
    let inputs = build_fix_inputs(plan.backspace_count, &plan.replacement_text);
    if inputs.is_empty() {
        return false;
    }

    hooks::begin_suppression();
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    hooks::end_suppression();

    switch_window_layout(target_hwnd, plan.target_hkl);
    sent == inputs.len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const US_HKL: HKL = HKL(0x04090409 as _);
    const RU_HKL: HKL = HKL(0x04190419 as _);

    const US_PAIRS: &[(u8, char, char)] = &[
        (0x22, 'g', 'G'),
        (0x23, 'h', 'H'),
        (0x30, 'b', 'B'),
        (0x20, 'd', 'D'),
        (0x14, 't', 'T'),
        (0x31, 'n', 'N'),
    ];
    const RU_PAIRS: &[(u8, char, char)] = &[
        (0x22, 'п', 'П'),
        (0x23, 'р', 'Р'),
        (0x30, 'и', 'И'),
        (0x20, 'в', 'В'),
        (0x14, 'е', 'Е'),
        (0x31, 'т', 'Т'),
    ];

    fn rec(vk: u16, scan: u16, ms: u32, kind: InputKind) -> InputRecord {
        InputRecord {
            vk,
            scan_code: scan,
            flags: 0,
            ms_tick: ms,
            kind,
            _pad: [0; 3],
        }
    }

    #[test]
    fn double_left_shift_triggers_within_300ms_and_rejects_intervening_keys_or_slow_taps() {
        let mut det = FixGestureDetector::new(FixTrigger::DoubleLeftShift);

        // Valid fast double-tap (120 ms gap):
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 100, InputKind::KeyDown)));
        // Auto-repeat while held must not break or trigger early:
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 130, InputKind::KeyDown)));
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 150, InputKind::KeyUp)));
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 260, InputKind::KeyDown)));
        assert!(det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 290, InputKind::KeyUp)));

        // Intervening letter key ('G') between taps cancels the double-tap:
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 500, InputKind::KeyDown)));
        assert!(!det.feed(rec(0x47, 0x22, 520, InputKind::KeyDown))); // Shift+G capital letter!
        assert!(!det.feed(rec(0x47, 0x22, 540, InputKind::KeyUp)));
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 550, InputKind::KeyUp)));
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 650, InputKind::KeyDown)));
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 680, InputKind::KeyUp)));

        // Slow second tap (> 300 ms after first tap up) does not trigger on second tap:
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 1000, InputKind::KeyDown)));
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 1040, InputKind::KeyUp)));
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 1400, InputKind::KeyDown))); // 360 ms > 300 ms
        assert!(!det.feed(rec(VK_LSHIFT, SCAN_LSHIFT, 1430, InputKind::KeyUp)));
    }

    #[test]
    fn double_left_ctrl_and_pause_break_triggers_work() {
        let mut ctrl_det = FixGestureDetector::new(FixTrigger::DoubleLeftCtrl);
        assert!(!ctrl_det.feed(rec(VK_LCONTROL, 0x1D, 100, InputKind::KeyDown)));
        assert!(!ctrl_det.feed(rec(VK_LCONTROL, 0x1D, 140, InputKind::KeyUp)));
        assert!(!ctrl_det.feed(rec(VK_LCONTROL, 0x1D, 280, InputKind::KeyDown)));
        assert!(ctrl_det.feed(rec(VK_LCONTROL, 0x1D, 320, InputKind::KeyUp)));

        let mut pause_det = FixGestureDetector::new(FixTrigger::PauseBreak);
        assert!(pause_det.feed(rec(VK_PAUSE, 0x45, 500, InputKind::KeyDown)));
        assert!(!pause_det.feed(rec(VK_PAUSE, 0x45, 530, InputKind::KeyUp)));
    }

    #[test]
    fn plan_word_fix_and_build_inputs_preserve_trailing_space_and_unicode_units() {
        let us = LayoutLayerTable::from_pairs(US_HKL, US_PAIRS);
        let ru = LayoutLayerTable::from_pairs(RU_HKL, RU_PAIRS);
        let tables = [&us, &ru];

        // "Ghbdtn " (6 letters + 1 trailing space = 7 chars):
        let trace = [
            KeyStrokeSlot { vk: 0x47, scan_code: 0x22, flags: 0, ms_tick: 10, shift: true },
            KeyStrokeSlot { vk: 0x48, scan_code: 0x23, flags: 0, ms_tick: 20, shift: false },
            KeyStrokeSlot { vk: 0x42, scan_code: 0x30, flags: 0, ms_tick: 30, shift: false },
            KeyStrokeSlot { vk: 0x44, scan_code: 0x20, flags: 0, ms_tick: 40, shift: false },
            KeyStrokeSlot { vk: 0x54, scan_code: 0x14, flags: 0, ms_tick: 50, shift: false },
            KeyStrokeSlot { vk: 0x4E, scan_code: 0x31, flags: 0, ms_tick: 60, shift: false },
            KeyStrokeSlot { vk: VK_SPACE, scan_code: 0x39, flags: 0, ms_tick: 70, shift: false },
        ];

        let plan = plan_word_fix(&trace, US_HKL, &tables).expect("FixPlan must be produced");
        assert_eq!(plan.original_text, "Ghbdtn ");
        assert_eq!(plan.replacement_text, "Привет ");
        assert_eq!(plan.backspace_count, 7);
        assert_eq!(plan.target_hkl, RU_HKL);

        let inputs = build_fix_inputs(plan.backspace_count, &plan.replacement_text);
        // 7 backspaces * 2 (down+up) + 7 UTF-16 code units * 2 (down+up) = 28 INPUT structs:
        assert_eq!(inputs.len(), 28);
        unsafe {
            assert_eq!(inputs[0].Anonymous.ki.wVk, VIRTUAL_KEY(VK_BACK));
            assert_eq!(inputs[14].Anonymous.ki.wScan, 'П' as u16);
            assert_eq!(inputs[14].Anonymous.ki.dwFlags, KEYEVENTF_UNICODE);
        }
    }
}
