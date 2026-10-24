#![allow(dead_code)]
//! Keyboard layout enumeration, per-HKL `ToUnicodeEx` layer cache, and word segmentation (M3).
//! Architectural invariants (AGENTS.md #5, #7; ARCHITECTURE.md §3.1, §3.2):
//! - No N*(N-1) manual character tables.
//! - Extended scan codes reconstructed with `0xE000` prefix when `LLKHF_EXTENDED` is set.
//! - `ToUnicodeEx` always called with flag `0x0004` (`UNICODE_NO_CHANGED_STATE`, Win10 1809+).
//! - Per-HKL 256-entry layer table (`base` + `shift`) lazily cached for O(1) lookup per key.

use crate::worker::KeyStrokeSlot;
use std::collections::HashMap;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, GetKeyboardLayoutList, MapVirtualKeyExW, ToUnicodeEx, HKL,
    MAPVK_VSC_TO_VK_EX,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO, LLKHF_EXTENDED,
};

/// Win10 1607+ flag: do not mutate kernel dead-key state inside `ToUnicodeEx`.
pub const UNICODE_NO_CHANGED_STATE: u32 = 0x0004;

const VK_SHIFT_IDX: usize = 0x10;
const VK_SPACE: u16 = 0x20;

/// Extract the 16-bit LANGID (low word) from an `HKL`.
#[inline]
pub fn hkl_lang_id(hkl: HKL) -> u16 {
    (hkl.0 as usize & 0xFFFF) as u16
}

/// Extract the 10-bit primary language ID (`PRIMARYLANGID(lang_id)`).
#[inline]
pub fn hkl_primary_lang(hkl: HKL) -> u16 {
    hkl_lang_id(hkl) & 0x03FF
}

/// Reconstruct the physical scan code passed to `ToUnicodeEx` / `MapVirtualKeyExW`.
/// `KBDLLHOOKSTRUCT.scanCode` strips the `0xE0` prefix; restore it when `LLKHF_EXTENDED` is set.
#[inline]
pub fn reconstruct_scan_code(scan_code: u16, flags: u32) -> u32 {
    let base = (scan_code & 0xFF) as u32;
    if (flags & LLKHF_EXTENDED.0) != 0 {
        0xE000 | base
    } else {
        base
    }
}

/// Enumerate all active keyboard layouts installed in the current user session.
pub fn active_layouts() -> Vec<HKL> {
    unsafe {
        let count = GetKeyboardLayoutList(None);
        if count <= 0 {
            let current = GetKeyboardLayout(0);
            return if current.0.is_null() {
                Vec::new()
            } else {
                vec![current]
            };
        }
        let mut list = vec![HKL::default(); count as usize];
        let written = GetKeyboardLayoutList(Some(&mut list));
        if written <= 0 {
            return Vec::new();
        }
        list.truncate(written as usize);
        list
    }
}

/// Resolves the actual focused child window and thread ID for a top-level window.
/// In multi-threaded apps (Chromium, Electron, MDI, WinUI), keyboard focus may belong to a child HWND.
pub fn focused_window_and_thread(hwnd: HWND) -> (HWND, u32) {
    if hwnd.0.is_null() {
        return (HWND(core::ptr::null_mut()), 0);
    }
    unsafe {
        let tid = GetWindowThreadProcessId(hwnd, None);
        if tid == 0 {
            return (hwnd, 0);
        }
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(tid, &mut gui).is_ok() && !gui.hwndFocus.0.is_null() {
            let focus_tid = GetWindowThreadProcessId(gui.hwndFocus, None);
            (gui.hwndFocus, if focus_tid != 0 { focus_tid } else { tid })
        } else {
            (hwnd, tid)
        }
    }
}

/// Query the active `HKL` of the thread owning `hwnd` or its focused child control.
pub fn window_layout(hwnd: HWND) -> HKL {
    let (_, tid) = focused_window_and_thread(hwnd);
    unsafe { GetKeyboardLayout(tid) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanTranslation {
    None,
    DeadKey,
    Char(char),
}

///Cached 256-scan-code layer table for a single `HKL`.
#[derive(Clone)]
pub struct LayoutLayerTable {
    pub hkl: HKL,
    pub lang_id: u16,
    pub primary_lang: u16,
    base: [ScanTranslation; 256],
    shift: [ScanTranslation; 256],
}

impl LayoutLayerTable {
    /// Build the 256-entry base + shift layer table for `hkl` via `MapVirtualKeyExW` + `ToUnicodeEx`.
    pub fn build(hkl: HKL) -> Self {
        let mut base = [ScanTranslation::None; 256];
        let mut shift = [ScanTranslation::None; 256];

        for sc in 0u32..256 {
            // Skip extended scan codes in the 0..256 non-extended table; non-extended typing keys live in 0x01..=0x58.
            let vk = unsafe { MapVirtualKeyExW(sc, MAPVK_VSC_TO_VK_EX, hkl) };
            if vk == 0 {
                continue;
            }
            base[sc as usize] = translate_single(vk, sc, false, hkl);
            shift[sc as usize] = translate_single(vk, sc, true, hkl);
        }

        Self {
            hkl,
            lang_id: hkl_lang_id(hkl),
            primary_lang: hkl_primary_lang(hkl),
            base,
            shift,
        }
    }

    /// Construct a layer table directly from `(scan_code, base_char, shift_char)` pairs (used for deterministic tests).
    #[cfg(test)]
    pub fn from_pairs(hkl: HKL, pairs: &[(u8, char, char)]) -> Self {
        let mut base = [ScanTranslation::None; 256];
        let mut shift = [ScanTranslation::None; 256];
        for &(sc, b, s) in pairs {
            base[sc as usize] = ScanTranslation::Char(b);
            shift[sc as usize] = ScanTranslation::Char(s);
        }
        Self {
            hkl,
            lang_id: hkl_lang_id(hkl),
            primary_lang: hkl_primary_lang(hkl),
            base,
            shift,
        }
    }

    /// O(1) lookup for a keystroke slot. Extended keys (`LLKHF_EXTENDED`) bypass the non-extended table.
    #[inline]
    pub fn lookup(&self, slot: &KeyStrokeSlot) -> ScanTranslation {
        let full_sc = reconstruct_scan_code(slot.scan_code, slot.flags);
        if full_sc >= 256 {
            return ScanTranslation::None;
        }
        if slot.shift {
            self.shift[full_sc as usize]
        } else {
            self.base[full_sc as usize]
        }
    }

    /// Render a slice of keystroke slots into a string using this layout's cached layers.
    /// Returns `None` if any keystroke in the slice triggers a dead key (scoring suppressed until resolved).
    pub fn render_keys(&self, keys: &[KeyStrokeSlot]) -> Option<String> {
        let mut out = String::with_capacity(keys.len());
        for slot in keys {
            if slot.vk == VK_SPACE {
                out.push(' ');
                continue;
            }
            match self.lookup(slot) {
                ScanTranslation::Char(c) => out.push(c),
                ScanTranslation::DeadKey => return None,
                ScanTranslation::None => {}
            }
        }
        Some(out)
    }

    /// Reverse lookup: find `(scan_code, shift)` that produces `ch` in this layout.
    pub fn find_char(&self, ch: char) -> Option<(u8, bool)> {
        for sc in 1u8..=255 {
            if self.base[sc as usize] == ScanTranslation::Char(ch) {
                return Some((sc, false));
            }
            if self.shift[sc as usize] == ScanTranslation::Char(ch) {
                return Some((sc, true));
            }
        }
        None
    }

    /// Look up the character produced by `(scan_code, shift)` in this layout.
    #[inline]
    pub fn char_at(&self, sc: u8, shift: bool) -> Option<char> {
        let entry = if shift {
            self.shift[sc as usize]
        } else {
            self.base[sc as usize]
        };
        match entry {
            ScanTranslation::Char(c) => Some(c),
            _ => None,
        }
    }

    /// Convert `text` from `self` (source layout) into `dst` (target layout) using scan-code reverse lookup.
    /// Returns `(converted_text, mapped_count, unmapped_alpha_count)`.
    pub fn convert_str_to(&self, text: &str, dst: &LayoutLayerTable) -> (String, usize, usize) {
        let mut out = String::with_capacity(text.len());
        let mut mapped_count = 0usize;
        let mut unmapped_alpha_count = 0usize;

        for ch in text.chars() {
            if ch.is_whitespace() {
                out.push(ch);
                continue;
            }
            if let Some((sc, shift)) = self.find_char(ch) {
                if let Some(target_ch) = dst.char_at(sc, shift) {
                    out.push(target_ch);
                    mapped_count += 1;
                    continue;
                }
            }
            if ch.is_alphabetic() {
                unmapped_alpha_count += 1;
            }
            out.push(ch);
        }

        (out, mapped_count, unmapped_alpha_count)
    }
}

fn translate_single(vk: u32, scan_code: u32, shift: bool, hkl: HKL) -> ScanTranslation {
    let mut key_state = [0u8; 256];
    if shift {
        key_state[VK_SHIFT_IDX] = 0x80;
    }
    let mut buf = [0u16; 8];
    let rc = unsafe {
        ToUnicodeEx(
            vk,
            scan_code,
            &key_state,
            &mut buf,
            UNICODE_NO_CHANGED_STATE,
            hkl,
        )
    };
    if rc < 0 {
        ScanTranslation::DeadKey
    } else if rc == 0 {
        ScanTranslation::None
    } else {
        let s = String::from_utf16_lossy(&buf[..rc as usize]);
        match s.chars().next() {
            Some(c) if !c.is_control() => ScanTranslation::Char(c),
            _ => ScanTranslation::None,
        }
    }
}

/// Lazily caches `LayoutLayerTable` per `HKL`.
#[derive(Default)]
pub struct LayoutCache {
    tables: HashMap<isize, LayoutLayerTable>,
}

impl LayoutCache {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    pub fn get_or_build(&mut self, hkl: HKL) -> &LayoutLayerTable {
        let key = hkl.0 as isize;
        self.tables
            .entry(key)
            .or_insert_with(|| LayoutLayerTable::build(hkl))
    }

    pub fn get(&self, hkl: HKL) -> Option<&LayoutLayerTable> {
        let key = hkl.0 as isize;
        self.tables.get(&key)
    }
}

/// Extract the active word being typed (after the last `VK_SPACE`), or empty if the last key was `VK_SPACE`.
pub fn active_word_slice(slots: &[KeyStrokeSlot]) -> &[KeyStrokeSlot] {
    match slots.iter().rposition(|k| k.vk == VK_SPACE) {
        Some(idx) => &slots[idx + 1..],
        None => slots,
    }
}

/// Extract the last non-empty word plus any trailing spaces (used by the fix gesture in M6).
pub fn last_word_run_for_fix(slots: &[KeyStrokeSlot]) -> &[KeyStrokeSlot] {
    if slots.is_empty() {
        return slots;
    }
    // Skip trailing spaces to find the end of the last word.
    let Some(last_non_space) = slots.iter().rposition(|k| k.vk != VK_SPACE) else {
        return &[];
    };
    let prefix = &slots[..=last_non_space];
    match prefix.iter().rposition(|k| k.vk == VK_SPACE) {
        Some(space_idx) => &slots[space_idx + 1..],
        None => slots,
    }
}

/// Returns `true` if the rendered token looks like a URL, file path, or CLI flag (`AGENTS.md` #7).
pub fn is_immune_token(rendered: &str) -> bool {
    let trimmed = rendered.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http:")
        || lower.starts_with("https:")
        || lower.starts_with("www.")
        || lower.starts_with("ftp:")
        || lower.starts_with("file:")
        || lower.starts_with("--")
        || lower.starts_with('/')
        || lower.starts_with('\\')
    {
        return true;
    }
    // Path separators or protocol markers inside token.
    trimmed.contains("://") || trimmed.contains('\\')
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::LLKHF_EXTENDED;

    // Standard hardware scan codes for QWERTY / ЙЦУКЕН row keys:
    // G=0x22 (П), H=0x23 (Р), B=0x30 (И), D=0x20 (В), T=0x14 (Е), N=0x31 (Т)
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

    fn slot(vk: u16, scan_code: u16, shift: bool) -> KeyStrokeSlot {
        KeyStrokeSlot {
            vk,
            scan_code,
            flags: 0,
            ms_tick: 100,
            shift,
        }
    }

    #[test]
    fn extended_scan_code_reconstructed_with_0xe000() {
        assert_eq!(reconstruct_scan_code(0x35, 0), 0x0035);
        assert_eq!(reconstruct_scan_code(0x35, LLKHF_EXTENDED.0), 0xE035);
    }

    #[test]
    fn fixture_trace_renders_ghbdtn_and_privet_across_layers() {
        let us = LayoutLayerTable::from_pairs(HKL(0x04090409 as _), US_PAIRS);
        let ru = LayoutLayerTable::from_pairs(HKL(0x04190419 as _), RU_PAIRS);

        // User types "Привет" on physical keys with Shift on first key:
        let trace = [
            slot(0x47, 0x22, true),  // Shift+G / П
            slot(0x48, 0x23, false), // H / р
            slot(0x42, 0x30, false), // B / и
            slot(0x44, 0x20, false), // D / в
            slot(0x54, 0x14, false), // T / е
            slot(0x4E, 0x31, false), // N / т
        ];

        assert_eq!(us.render_keys(&trace).as_deref(), Some("Ghbdtn"));
        assert_eq!(ru.render_keys(&trace).as_deref(), Some("Привет"));
    }

    #[test]
    fn word_segmentation_splits_on_space_and_preserves_trailing_space_for_fix() {
        let trace = [
            slot(0x47, 0x22, false),
            slot(0x48, 0x23, false),
            slot(VK_SPACE, 0x39, false),
            slot(0x42, 0x30, false),
            slot(0x44, 0x20, false),
        ];
        let active = active_word_slice(&trace);
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].scan_code, 0x30);

        // When space was just pressed after the word:
        let with_trailing_space = [
            slot(0x47, 0x22, false),
            slot(VK_SPACE, 0x39, false),
            slot(0x42, 0x30, false),
            slot(0x44, 0x20, false),
            slot(VK_SPACE, 0x39, false),
        ];
        assert!(active_word_slice(&with_trailing_space).is_empty());
        let fix_run = last_word_run_for_fix(&with_trailing_space);
        assert_eq!(fix_run.len(), 3); // 'b', 'd', and trailing space
        assert_eq!(fix_run[0].scan_code, 0x30);
        assert_eq!(fix_run[2].vk, VK_SPACE);
    }

    #[test]
    fn url_and_path_tokens_are_immune() {
        assert!(is_immune_token("https://example.com"));
        assert!(is_immune_token("http://localhost"));
        assert!(is_immune_token("www.google.com"));
        assert!(is_immune_token(r"C:\projects\SmartPunto"));
        assert!(is_immune_token("--release"));
        assert!(is_immune_token("/usr/bin"));
        assert!(!is_immune_token("ghbdtn"));
        assert!(!is_immune_token("привет"));
    }

    #[test]
    fn host_active_layouts_and_tounicodeex_cache_work() {
        let layouts = active_layouts();
        assert!(!layouts.is_empty(), "host must have at least one active HKL");
        let mut cache = LayoutCache::new();
        let table = cache.get_or_build(layouts[0]);
        assert_eq!(table.hkl, layouts[0]);
    }
}
