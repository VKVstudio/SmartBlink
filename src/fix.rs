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
const VK_RSHIFT: u16 = 0xA1;
const VK_LCONTROL: u16 = 0xA2;
const VK_RCONTROL: u16 = 0xA3;
const VK_LMENU: u16 = 0xA4;
const VK_RMENU: u16 = 0xA5;
const SCAN_LSHIFT: u16 = 0x2A;
const INPUTLANGCHANGE_SYSCHARSET: usize = 0x0001;

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

    let (original_probe, probe_dead) = current_table.render_keys_raw(run, false);
    if original_probe.trim().is_empty() && probe_dead.is_none() {
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
            let Some(rendered_word) = table.render_keys_for_target(word_only) else {
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

    let run = expand_erroneous_burst_run(word_buf_slots, current_table, target_table);
    let (original_text, _) = current_table.render_keys_raw(run, false);
    let replacement_text =
        render_replacement_preserving_top_row_punct(run, current_table, target_table)?;
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

/// Render `run` into `target_table` while preserving trailing `?` (`Shift+7`), `:` (`Shift+6`),
/// `;` (`Shift+4`), `.` (`/`), and `,` (`Shift+/`) when typed in Russian layout at the end of a word
/// (so `акшутв?`, `ш ерштл,`, and `руддщ.` convert to `friend?`, `i think,`, and `hello.`).
fn render_replacement_preserving_top_row_punct(
    run: &[KeyStrokeSlot],
    current_table: &LayoutLayerTable,
    target_table: &LayoutLayerTable,
) -> Option<String> {
    let mut out = String::with_capacity(run.len());
    let mut pending_dead: Option<char> = None;

    for (idx, slot) in run.iter().enumerate() {
        if slot.vk == VK_SPACE {
            if let Some(d) = pending_dead.take() {
                out.push(d);
            } else {
                out.push(' ');
            }
            continue;
        }
        let cur_ch = match current_table.lookup(slot) {
            layout::ScanTranslation::Char(c) => Some(c),
            layout::ScanTranslation::Chars(c1, _) => Some(c1),
            layout::ScanTranslation::DeadKey(_) => None,
            layout::ScanTranslation::None => None,
        };
        match target_table.lookup_for_target(slot) {
            layout::ScanTranslation::Char(dst_ch) => {
                if let Some(d) = pending_dead.take() {
                    if let Some(combined) = target_table.combine_dead(d, dst_ch) {
                        out.push(combined);
                    } else {
                        out.push(d);
                        out.push(dst_ch);
                    }
                    continue;
                }
                let at_word_end = idx > 0 && (idx + 1 == run.len() || run[idx + 1].vk == VK_SPACE);
                let mapped = if let Some(c_ch) = cur_ch {
                    if at_word_end
                        && matches!(
                            (c_ch, dst_ch),
                            ('?', '&') | (':', '^') | (';', '$') | ('.', '/') | (',', '?')
                        )
                    {
                        c_ch
                    } else {
                        dst_ch
                    }
                } else {
                    dst_ch
                };
                out.push(mapped);
            }
            layout::ScanTranslation::Chars(c1, c2) => {
                if let Some(d) = pending_dead.take() {
                    out.push(d);
                }
                out.push(c1);
                out.push(c2);
            }
            layout::ScanTranslation::DeadKey(d) => {
                if let Some(prev_d) = pending_dead.take() {
                    out.push(prev_d);
                } else {
                    pending_dead = Some(d);
                }
            }
            layout::ScanTranslation::None => {}
        }
    }
    if pending_dead.is_some() {
        return None;
    }
    Some(out)
}

#[inline]
fn is_direct_wrong_layout_token(cur_s: i32, dst_s: i32) -> bool {
    (cur_s <= profile::CURRENT_MAX_PLAUSIBLE_SCORE && dst_s > cur_s)
        || (dst_s >= 15 && (dst_s - cur_s) >= 20)
        || (cur_s <= -50 && dst_s >= -35)
}

fn has_earlier_wrong_layout_word(
    slots: &[KeyStrokeSlot],
    current_table: &LayoutLayerTable,
    target_table: &LayoutLayerTable,
    cur_prof: Option<&'static profile::LanguageProfile>,
    dst_prof: Option<&'static profile::LanguageProfile>,
) -> bool {
    let mut end = slots.len();
    while end > 0 {
        while end > 0 && slots[end - 1].vk == VK_SPACE {
            end -= 1;
        }
        if end == 0 {
            break;
        }
        let mut start = end;
        while start > 0 && slots[start - 1].vk != VK_SPACE {
            start -= 1;
        }
        let word = &slots[start..end];
        let (mut cur_str, cur_dead) = current_table.render_keys_raw(word, false);
        if let Some(d) = cur_dead {
            cur_str.push(d);
        }
        if let Some(dst_str) = target_table.render_keys_for_target(word) {
            let cur_s = cur_prof.map(|p| p.score_word(&cur_str)).unwrap_or(0);
            let dst_s = dst_prof.map(|p| p.score_word(&dst_str)).unwrap_or(0);
            if is_direct_wrong_layout_token(cur_s, dst_s) {
                return true;
            }
        }
        end = start;
    }
    false
}

/// Walk backwards from the last word in `word_buf_slots` to include any immediately preceding
/// words in the same burst that were also typed in the wrong layout, while stopping at any word
/// that is genuinely valid in `current_table` and gibberish in `target_table` (e.g. `"Hello "` in `"Hello ghbdtn"`).
fn expand_erroneous_burst_run<'a>(
    word_buf_slots: &'a [KeyStrokeSlot],
    current_table: &LayoutLayerTable,
    target_table: &LayoutLayerTable,
) -> &'a [KeyStrokeSlot] {
    let base_run = layout::last_word_run_for_fix(word_buf_slots);
    if base_run.is_empty() {
        return base_run;
    }
    let mut start_idx = word_buf_slots.len().saturating_sub(base_run.len());
    let cur_prof = profile::profile_for_primary_lang(current_table.primary_lang);
    let dst_prof = profile::profile_for_primary_lang(target_table.primary_lang);

    while start_idx > 0 {
        // Skip spaces immediately preceding `start_idx`.
        let mut word_end = start_idx;
        while word_end > 0 && word_buf_slots[word_end - 1].vk == VK_SPACE {
            word_end -= 1;
        }
        if word_end == 0 {
            break;
        }
        let mut word_start = word_end;
        while word_start > 0 && word_buf_slots[word_start - 1].vk != VK_SPACE {
            word_start -= 1;
        }
        let prev_word_slots = &word_buf_slots[word_start..word_end];
        let (mut cur_str, cur_dead) = current_table.render_keys_raw(prev_word_slots, false);
        if let Some(d) = cur_dead {
            cur_str.push(d);
        }
        let Some(dst_str) = target_table.render_keys_for_target(prev_word_slots) else {
            break;
        };

        let cur_s = cur_prof.map(|p| p.score_word(&cur_str)).unwrap_or(0);
        let dst_s = dst_prof.map(|p| p.score_word(&dst_str)).unwrap_or(0);

        // Hard stop if the word is genuinely valid in the current layout and gibberish in the target layout.
        let is_genuine_current_lang = cur_s >= 5 && dst_s <= -15;
        if is_genuine_current_lang {
            break;
        }

        let is_also_wrong_layout = is_direct_wrong_layout_token(cur_s, dst_s)
            || (dst_s >= -15
                && has_earlier_wrong_layout_word(
                    &word_buf_slots[..word_start],
                    current_table,
                    target_table,
                    cur_prof,
                    dst_prof,
                ));

        if is_also_wrong_layout {
            start_idx = word_start;
        } else {
            break;
        }
    }

    &word_buf_slots[start_idx..]
}

/// Plan a fix using `LayoutCache` for the session's active `HKL`s.
pub fn plan_word_fix_cached(
    word_buf_slots: &[KeyStrokeSlot],
    current_hkl: HKL,
    active_hkls: &[HKL],
    cache: &mut LayoutCache,
) -> Option<FixPlan> {
    for &hkl in active_hkls {
        let _ = cache.get_or_build(hkl);
    }
    let _ = cache.get_or_build(current_hkl);
    let mut tables: Vec<&LayoutLayerTable> = active_hkls
        .iter()
        .filter_map(|hkl| cache.get(*hkl))
        .collect();
    if !tables.iter().any(|t| t.hkl == current_hkl) {
        if let Some(cur_t) = cache.get(current_hkl) {
            tables.push(cur_t);
        }
    }
    if tables.len() < 2 {
        return None;
    }
    plan_word_fix(word_buf_slots, current_hkl, &tables)
}

pub fn build_modifier_up_inputs() -> Vec<INPUT> {
    const MODIFIER_VKS: &[u16] = &[
        VK_LSHIFT,
        VK_RSHIFT,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LMENU,
        VK_RMENU,
    ];
    let mut inputs = Vec::with_capacity(MODIFIER_VKS.len());
    for &vk in MODIFIER_VKS {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }
    inputs
}

pub fn build_backspace_inputs(backspace_count: usize) -> Vec<INPUT> {
    let mut inputs = Vec::with_capacity(backspace_count * 2);
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
    inputs
}

pub fn build_unicode_inputs(replacement_text: &str) -> Vec<INPUT> {
    let utf16_len = replacement_text.encode_utf16().count();
    let mut inputs = Vec::with_capacity(utf16_len * 2);
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

/// Build the exact `SendInput` sequence: `backspace_count` `VK_BACK` key taps followed by
/// `KEYEVENTF_UNICODE` UTF-16 code units for `replacement_text`.
pub fn build_fix_inputs(backspace_count: usize, replacement_text: &str) -> Vec<INPUT> {
    let mut inputs = build_backspace_inputs(backspace_count);
    inputs.extend(build_unicode_inputs(replacement_text));
    inputs
}

/// Request keyboard layout switch in the focused target control and top-level window (`ARCHITECTURE.md` §3.1).
pub fn switch_window_layout(hwnd: HWND, target_hkl: HKL) {
    let (focused_hwnd, _) = layout::focused_window_and_thread(hwnd);
    unsafe {
        let _ = ActivateKeyboardLayout(target_hkl, ACTIVATE_KEYBOARD_LAYOUT_FLAGS(0));
        if !focused_hwnd.0.is_null() {
            let _ = PostMessageW(
                focused_hwnd,
                WM_INPUTLANGCHANGEREQUEST,
                WPARAM(INPUTLANGCHANGE_SYSCHARSET),
                LPARAM(target_hkl.0 as isize),
            );
        }
        if focused_hwnd != hwnd && !hwnd.0.is_null() {
            let _ = PostMessageW(
                hwnd,
                WM_INPUTLANGCHANGEREQUEST,
                WPARAM(INPUTLANGCHANGE_SYSCHARSET),
                LPARAM(target_hkl.0 as isize),
            );
        }
    }
}

/// Execute a `FixPlan`: release lingering modifiers, inject backspaces, yield briefly,
/// inject `KEYEVENTF_UNICODE` replacement characters, and switch the focused window to `plan.target_hkl`.
pub fn execute_fix(plan: &FixPlan, target_hwnd: HWND) -> bool {
    if plan.backspace_count == 0 && plan.replacement_text.is_empty() {
        return false;
    }

    hooks::begin_suppression();

    // 1. Release any lingering Shift/Ctrl/Alt modifiers so VK_BACK never triggers Ctrl+Backspace (delete whole word).
    let mod_ups = build_modifier_up_inputs();
    unsafe { SendInput(&mod_ups, size_of::<INPUT>() as i32) };

    // 2. Send backspaces first and yield 5 ms so Chromium/Electron/RichEdit settles caret state.
    if plan.backspace_count > 0 {
        let bs_inputs = build_backspace_inputs(plan.backspace_count);
        let sent = unsafe { SendInput(&bs_inputs, size_of::<INPUT>() as i32) };
        if sent != bs_inputs.len() as u32 {
            hooks::end_suppression();
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // 3. Inject replacement UTF-16 units via KEYEVENTF_UNICODE.
    let mut ok = true;
    if !plan.replacement_text.is_empty() {
        let text_inputs = build_unicode_inputs(&plan.replacement_text);
        let sent = unsafe { SendInput(&text_inputs, size_of::<INPUT>() as i32) };
        ok = sent == text_inputs.len() as u32;
    }

    hooks::end_suppression();

    switch_window_layout(target_hwnd, plan.target_hkl);
    ok
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
            KeyStrokeSlot { vk: 0x47, scan_code: 0x22, flags: 0, ms_tick: 10, shift: true, altgr: false },
            KeyStrokeSlot { vk: 0x48, scan_code: 0x23, flags: 0, ms_tick: 20, shift: false, altgr: false },
            KeyStrokeSlot { vk: 0x42, scan_code: 0x30, flags: 0, ms_tick: 30, shift: false, altgr: false },
            KeyStrokeSlot { vk: 0x44, scan_code: 0x20, flags: 0, ms_tick: 40, shift: false, altgr: false },
            KeyStrokeSlot { vk: 0x54, scan_code: 0x14, flags: 0, ms_tick: 50, shift: false, altgr: false },
            KeyStrokeSlot { vk: 0x4E, scan_code: 0x31, flags: 0, ms_tick: 60, shift: false, altgr: false },
            KeyStrokeSlot { vk: VK_SPACE, scan_code: 0x39, flags: 0, ms_tick: 70, shift: false, altgr: false },
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

    #[test]
    fn plan_word_fix_expands_multiword_burst_with_short_words_and_preserves_valid_prefix() {
        const FULL_US: &[(u8, char, char)] = &[
            (0x08, '7', '&'),
            (0x10, 'q', 'Q'),
            (0x11, 'w', 'W'),
            (0x12, 'e', 'E'),
            (0x13, 'r', 'R'),
            (0x14, 't', 'T'),
            (0x15, 'y', 'Y'),
            (0x16, 'u', 'U'),
            (0x17, 'i', 'I'),
            (0x18, 'o', 'O'),
            (0x19, 'p', 'P'),
            (0x1A, '[', '{'),
            (0x1B, ']', '}'),
            (0x1E, 'a', 'A'),
            (0x1F, 's', 'S'),
            (0x20, 'd', 'D'),
            (0x21, 'f', 'F'),
            (0x22, 'g', 'G'),
            (0x23, 'h', 'H'),
            (0x24, 'j', 'J'),
            (0x25, 'k', 'K'),
            (0x26, 'l', 'L'),
            (0x27, ';', ':'),
            (0x28, '\'', '"'),
            (0x2C, 'z', 'Z'),
            (0x2D, 'x', 'X'),
            (0x2E, 'c', 'C'),
            (0x2F, 'v', 'V'),
            (0x30, 'b', 'B'),
            (0x31, 'n', 'N'),
            (0x32, 'm', 'M'),
            (0x33, ',', '<'),
            (0x34, '.', '>'),
            (0x35, '/', '?'),
        ];
        const FULL_RU: &[(u8, char, char)] = &[
            (0x08, '7', '?'),
            (0x10, 'й', 'Й'),
            (0x11, 'ц', 'Ц'),
            (0x12, 'у', 'У'),
            (0x13, 'к', 'К'),
            (0x14, 'е', 'Е'),
            (0x15, 'н', 'Н'),
            (0x16, 'г', 'Г'),
            (0x17, 'ш', 'Ш'),
            (0x18, 'щ', 'Щ'),
            (0x19, 'з', 'З'),
            (0x1A, 'х', 'Х'),
            (0x1B, 'ъ', 'Ъ'),
            (0x1E, 'ф', 'Ф'),
            (0x1F, 'ы', 'Ы'),
            (0x20, 'в', 'В'),
            (0x21, 'а', 'А'),
            (0x22, 'п', 'П'),
            (0x23, 'р', 'Р'),
            (0x24, 'о', 'О'),
            (0x25, 'л', 'Л'),
            (0x26, 'д', 'Д'),
            (0x27, 'ж', 'Ж'),
            (0x28, 'э', 'Э'),
            (0x2C, 'я', 'Я'),
            (0x2D, 'ч', 'Ч'),
            (0x2E, 'с', 'С'),
            (0x2F, 'м', 'М'),
            (0x30, 'и', 'И'),
            (0x31, 'т', 'Т'),
            (0x32, 'ь', 'Ь'),
            (0x33, 'б', 'Б'),
            (0x34, 'ю', 'Ю'),
            (0x35, '.', ','),
        ];

        let us = LayoutLayerTable::from_pairs(US_HKL, FULL_US);
        let ru = LayoutLayerTable::from_pairs(RU_HKL, FULL_RU);
        let tables = [&us, &ru];

        let type_str = |s: &str, pairs: &[(u8, char, char)]| -> Vec<KeyStrokeSlot> {
            s.chars()
                .enumerate()
                .map(|(i, ch)| {
                    if ch == ' ' {
                        KeyStrokeSlot {
                            vk: VK_SPACE,
                            scan_code: 0x39,
                            flags: 0,
                            ms_tick: (i as u32) * 30,
                            shift: false,
                            altgr: false,
                        }
                    } else {
                        let (sc, shift) = pairs
                            .iter()
                            .find_map(|&(sc, b, sh)| {
                                if ch == b {
                                    Some((sc, false))
                                } else if ch == sh {
                                    Some((sc, true))
                                } else {
                                    None
                                }
                            })
                            .unwrap();
                        KeyStrokeSlot {
                            vk: 0x41,
                            scan_code: sc as u16,
                            flags: 0,
                            ms_tick: (i as u32) * 30,
                            shift,
                            altgr: false,
                        }
                    }
                })
                .collect()
        };

        // 1. Full wrong-layout burst with 2-letter word "yt" ("не") and trailing "?":
        let burst1 = type_str("ghbdtn yfthyjt yt hf,jnftn?", FULL_US);
        let plan1 = plan_word_fix(&burst1, US_HKL, &tables).expect("plan1");
        assert_eq!(plan1.original_text, "ghbdtn yfthyjt yt hf,jnftn?");
        assert_eq!(plan1.replacement_text, "привет наерное не работает,");

        // 2. Burst starting with 3-letter word "xnj" ("что") and ending with double punctuation "&/":
        let burst2 = type_str("xnj ljk;yj ghjbc[jlbnm&/", FULL_US);
        let plan2 = plan_word_fix(&burst2, US_HKL, &tables).expect("plan2");
        assert_eq!(plan2.original_text, "xnj ljk;yj ghjbc[jlbnm&/");
        assert_eq!(plan2.replacement_text, "что должно происходить?.");

        // 3. Valid English prefix "Hello to " before "ghbdtn" must NOT corrupt "to":
        let burst3 = type_str("Hello to ghbdtn", FULL_US);
        let plan3 = plan_word_fix(&burst3, US_HKL, &tables).expect("plan3");
        assert_eq!(plan3.original_text, "ghbdtn");
        assert_eq!(plan3.replacement_text, "привет");

        // 4. Full 9-word sentence containing homograph "herb" ("руки") in the middle must convert completely:
        let burst4 = type_str(
            "bylbrfnjh chf,fnsdftn vuyjdtyyj? tuj bppf herb lf;t yt dblyj",
            FULL_US,
        );
        let plan4 = plan_word_fix(&burst4, US_HKL, &tables).expect("plan4");
        assert_eq!(
            plan4.replacement_text,
            "индикатор срабатывает мгновенно, его изза руки даже не видно"
        );

        // 5. Full 6-word sentence with short words and expressive diminutive:
        let burst5 = type_str("rfr nj dct hfdyj t,fytymrj hf,jnftn", FULL_US);
        let plan5 = plan_word_fix(&burst5, US_HKL, &tables).expect("plan5");
        assert_eq!(
            plan5.replacement_text,
            "как то все равно ебаненько работает"
        );

        // 6. Full 11-word RU->EN sentence with Russian `?` (Shift+7) on `акшутв?` and `,` (Shift+/) on `ерштл,`:
        let burst6 = type_str(
            "рщц фку нщг ьн акшутв? ш ерштл, црфе ше цщкл сщккуседн?",
            FULL_RU,
        );
        let plan6 = plan_word_fix(&burst6, RU_HKL, &tables).expect("plan6");
        assert_eq!(
            plan6.replacement_text,
            "how are you my friend? i think, what it work correctly?"
        );

        // 7. Capitalized "Hello" typed on KA-QWERTY (Shift+H is None -> commits 4 chars "ელლო"):
        const KA_HKL: HKL = HKL(0x04370437 as _);
        let ka_qw = LayoutLayerTable::from_pairs(
            KA_HKL,
            &[
                (0x23, 'ჰ', '\0'),
                (0x12, 'ე', '\0'),
                (0x26, 'ლ', '\0'),
                (0x18, 'ო', '\0'),
            ],
        );
        let hello_slots = type_str("Hello", FULL_US);
        let plan_ka = plan_word_fix(&hello_slots, KA_HKL, &[&ka_qw, &us]).expect("plan_ka");
        assert_eq!(plan_ka.backspace_count, 4); // 4 committed chars ("ელლო"), not 5!
        assert_eq!(plan_ka.replacement_text, "Hello");

        // 8. Russian "этих" typed on FR AZERTY (final key 0x1A is dead key '^' -> commits 3 chars "ùnb"):
        const FR_HKL: HKL = HKL(0x040C040C as _);
        let fr_azerty = LayoutLayerTable::from_pairs(
            FR_HKL,
            &[
                (0x28, 'ù', '%'),
                (0x31, 'n', 'N'),
                (0x30, 'b', 'B'),
            ],
        )
        .with_dead_key(0x1A, false, '^');
        let etikh_slots = type_str("этих", FULL_RU);
        let plan_fr = plan_word_fix(&etikh_slots, FR_HKL, &[&fr_azerty, &ru]).expect("plan_fr");
        assert_eq!(plan_fr.backspace_count, 3); // 3 committed chars ("ùnb"), pending '^' not committed!
        assert_eq!(plan_fr.replacement_text, "этих");
    }
}
