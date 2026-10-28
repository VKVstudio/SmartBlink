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
const VK_CONTROL_IDX: usize = 0x11;
const VK_MENU_IDX: usize = 0x12;
const VK_LCONTROL_IDX: usize = 0xA2;
const VK_RMENU_IDX: usize = 0xA5;
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
    DeadKey(char),
    Char(char),
    Chars(char, char),
}

/// Compose a dead-key accent character with a following base letter across all 20 built-in languages.
pub fn combine_dead_key(dead: char, base: char) -> Option<char> {
    combine_dead_key_for_lang(0, dead, base)
}

/// Language-aware dead-key composition derived from the 45 Windows 11 `KBD*.DLL` `pDeadKey` tables.
pub fn combine_dead_key_for_lang(primary_lang: u16, dead: char, base: char) -> Option<char> {
    match dead {
        '΅' | '\u{0344}' => match base {
            'ι' => Some('ΐ'),
            'υ' => Some('ΰ'),
            'Ι' => Some('Ϊ'),
            'Υ' => Some('Ϋ'),
            _ => None,
        },
        '´' | '\u{0301}' | '\'' | '΄' | ';' => match base {
            'a' => Some('á'),
            'A' => Some('Á'),
            'e' => Some('é'),
            'E' => Some('É'),
            'i' | 'ı' => Some('í'),
            'I' | 'İ' => Some('Í'),
            'o' => Some('ó'),
            'O' => Some('Ó'),
            'u' => Some('ú'),
            'U' => Some('Ú'),
            'y' => Some('ý'),
            'Y' => Some('Ý'),
            'c' => {
                if dead == '\'' || matches!(primary_lang, 0x0A | 0x09 | 0x16 | 0x0C) {
                    Some('ç')
                } else {
                    Some('ć')
                }
            }
            'C' => {
                if dead == '\'' || matches!(primary_lang, 0x0A | 0x09 | 0x16 | 0x0C) {
                    Some('Ç')
                } else {
                    Some('Ć')
                }
            }
            'l' => Some('ĺ'),
            'L' => Some('Ĺ'),
            'n' => Some('ń'),
            'N' => Some('Ń'),
            'r' => Some('ŕ'),
            'R' => Some('Ŕ'),
            's' => Some('ś'),
            'S' => Some('Ś'),
            'z' => Some('ź'),
            'Z' => Some('Ź'),
            'α' => Some('ά'),
            'Α' => Some('Ά'),
            'ε' => Some('έ'),
            'Ε' => Some('Έ'),
            'η' => Some('ή'),
            'Η' => Some('Ή'),
            'ι' => Some('ί'),
            'Ι' => Some('Ί'),
            'ο' => Some('ό'),
            'Ο' => Some('Ό'),
            'υ' => Some('ύ'),
            'Υ' => Some('Ύ'),
            'ω' => Some('ώ'),
            'Ω' => Some('Ώ'),
            _ => None,
        },
        '`' | '\u{0300}' => match base {
            'a' => Some('à'),
            'A' => Some('À'),
            'e' => Some('è'),
            'E' => Some('È'),
            'i' | 'ı' => Some('ì'),
            'I' | 'İ' => Some('Ì'),
            'o' => Some('ò'),
            'O' => Some('Ò'),
            'u' => Some('ù'),
            'U' => Some('Ù'),
            _ => None,
        },
        '^' | '\u{0302}' => match base {
            'a' => Some('â'),
            'A' => Some('Â'),
            'e' => Some('ê'),
            'E' => Some('Ê'),
            'i' | 'ı' => Some('î'),
            'I' | 'İ' => Some('Î'),
            'o' => Some('ô'),
            'O' => Some('Ô'),
            'u' => Some('û'),
            'U' => Some('Û'),
            'c' => Some('ĉ'),
            'C' => Some('Ĉ'),
            'g' => Some('ĝ'),
            'G' => Some('Ĝ'),
            'h' => Some('ĥ'),
            'H' => Some('Ĥ'),
            'j' => Some('ĵ'),
            'J' => Some('Ĵ'),
            's' => Some('ŝ'),
            'S' => Some('Ŝ'),
            'w' => Some('ŵ'),
            'W' => Some('Ŵ'),
            'y' => Some('ŷ'),
            'Y' => Some('Ŷ'),
            _ => None,
        },
        '~' | '\u{0303}' => {
            if primary_lang == 0x15 {
                // Polish Programmers (`KBDPL1.DLL`): dead `~` (`Shift+\``) produces Polish diacritics.
                match base {
                    'a' => Some('ą'),
                    'A' => Some('Ą'),
                    'c' => Some('ć'),
                    'C' => Some('Ć'),
                    'e' => Some('ę'),
                    'E' => Some('Ę'),
                    'l' => Some('ł'),
                    'L' => Some('Ł'),
                    'n' => Some('ń'),
                    'N' => Some('Ń'),
                    'o' => Some('ó'),
                    'O' => Some('Ó'),
                    's' => Some('ś'),
                    'S' => Some('Ś'),
                    'x' => Some('ź'),
                    'X' => Some('Ź'),
                    'z' => Some('ż'),
                    'Z' => Some('Ż'),
                    _ => None,
                }
            } else {
                match base {
                    'a' => Some('ã'),
                    'A' => Some('Ã'),
                    'n' => Some('ñ'),
                    'N' => Some('Ñ'),
                    'o' => Some('õ'),
                    'O' => Some('Õ'),
                    'e' => Some('ẽ'),
                    'E' => Some('Ẽ'),
                    'i' => Some('ĩ'),
                    'I' => Some('Ĩ'),
                    'u' => Some('ũ'),
                    'U' => Some('Ũ'),
                    'c' => Some('ć'),
                    'C' => Some('Ć'),
                    'l' => Some('ł'),
                    'L' => Some('Ł'),
                    's' => Some('ś'),
                    'S' => Some('Ś'),
                    'x' => Some('ź'),
                    'X' => Some('Ź'),
                    'z' => Some('ż'),
                    'Z' => Some('Ż'),
                    _ => None,
                }
            }
        }
        '¨' | '\u{0308}' | '"' | ':' => match base {
            'a' => Some('ä'),
            'A' => Some('Ä'),
            'e' => Some('ë'),
            'E' => Some('Ë'),
            'i' | 'ı' => Some('ï'),
            'I' | 'İ' => Some('Ï'),
            'o' => Some('ö'),
            'O' => Some('Ö'),
            'u' => Some('ü'),
            'U' => Some('Ü'),
            'y' => Some('ÿ'),
            'Y' => Some('Ÿ'),
            'ι' => Some('ϊ'),
            'Ι' => Some('Ϊ'),
            'υ' => Some('ϋ'),
            'Υ' => Some('Ϋ'),
            _ => None,
        },
        'ˇ' | '\u{030C}' => match base {
            'c' => Some('č'),
            'C' => Some('Č'),
            'd' => Some('ď'),
            'D' => Some('Ď'),
            'e' => Some('ě'),
            'E' => Some('Ě'),
            'l' => Some('ľ'),
            'L' => Some('Ľ'),
            'n' => Some('ň'),
            'N' => Some('Ň'),
            'r' => Some('ř'),
            'R' => Some('Ř'),
            's' => Some('š'),
            'S' => Some('Š'),
            't' => Some('ť'),
            'T' => Some('Ť'),
            'z' => Some('ž'),
            'Z' => Some('Ž'),
            _ => None,
        },
        '˚' | '°' | '\u{030A}' => match base {
            'a' => Some('å'),
            'A' => Some('Å'),
            'u' => Some('ů'),
            'U' => Some('Ů'),
            _ => None,
        },
        '¸' | '\u{0327}' => match base {
            'c' => Some('ç'),
            'C' => Some('Ç'),
            'g' => Some('ģ'),
            'G' => Some('Ģ'),
            'k' => Some('ķ'),
            'K' => Some('Ķ'),
            'l' => Some('ļ'),
            'L' => Some('Ļ'),
            'n' => Some('ņ'),
            'N' => Some('Ņ'),
            'r' => Some('ŗ'),
            'R' => Some('Ŗ'),
            's' => Some('ş'),
            'S' => Some('Ş'),
            't' => Some('ţ'),
            'T' => Some('Ţ'),
            _ => None,
        },
        '˘' | '\u{0306}' => match base {
            'a' => Some('ă'),
            'A' => Some('Ă'),
            'g' => Some('ğ'),
            'G' => Some('Ğ'),
            'u' => Some('ŭ'),
            'U' => Some('Ŭ'),
            _ => None,
        },
        '˙' | '·' | '\u{0307}' => match base {
            'z' => Some('ż'),
            'Z' => Some('Ż'),
            'e' => Some('ė'),
            'E' => Some('Ė'),
            'c' => Some('ċ'),
            'C' => Some('Ċ'),
            'g' => Some('ġ'),
            'G' => Some('Ġ'),
            'i' => Some('ı'),
            'I' => Some('İ'),
            _ => None,
        },
        '˛' | '\u{0328}' => match base {
            'a' => Some('ą'),
            'A' => Some('Ą'),
            'e' => Some('ę'),
            'E' => Some('Ę'),
            'i' => Some('į'),
            'I' => Some('Į'),
            'u' => Some('ų'),
            'U' => Some('Ų'),
            _ => None,
        },
        '˝' | '\u{030B}' => match base {
            'o' => Some('ő'),
            'O' => Some('Ő'),
            'u' => Some('ű'),
            'U' => Some('Ű'),
            _ => None,
        },
        _ => None,
    }
}

#[inline]
fn is_target_script_letter(primary_lang: u16, trans: ScanTranslation) -> bool {
    match trans {
        ScanTranslation::Char(c) => match primary_lang {
            // Georgian (Mkhedruli U+10D0..U+10FF)
            0x37 => ('\u{10A0}'..='\u{10FF}').contains(&c),
            // Hebrew (Alef..Tav U+05D0..U+05EA)
            0x0D => ('\u{05D0}'..='\u{05EA}').contains(&c),
            // Arabic base letters (U+0621..U+064A, excluding combining harakat U+064B..U+0655 and tatweel U+0640)
            0x01 => ('\u{0621}'..='\u{063A}').contains(&c) || ('\u{0641}'..='\u{064A}').contains(&c),
            _ => c.is_alphabetic(),
        },
        ScanTranslation::Chars(c1, _) => match primary_lang {
            0x01 => ('\u{0621}'..='\u{063A}').contains(&c1) || ('\u{0641}'..='\u{064A}').contains(&c1),
            _ => c1.is_alphabetic(),
        },
        _ => false,
    }
}

/// Cached 256-scan-code layer table (`base`, `shift`, `altgr`, `shift_altgr`) for a single `HKL`.
#[derive(Clone)]
pub struct LayoutLayerTable {
    pub hkl: HKL,
    pub lang_id: u16,
    pub primary_lang: u16,
    base: [ScanTranslation; 256],
    shift: [ScanTranslation; 256],
    altgr: [ScanTranslation; 256],
    shift_altgr: [ScanTranslation; 256],
}

impl LayoutLayerTable {
    /// Build the 256-entry base + shift + AltGr layer table for `hkl` via `MapVirtualKeyExW` + `ToUnicodeEx`.
    pub fn build(hkl: HKL) -> Self {
        let mut base = [ScanTranslation::None; 256];
        let mut shift = [ScanTranslation::None; 256];
        let mut altgr = [ScanTranslation::None; 256];
        let mut shift_altgr = [ScanTranslation::None; 256];

        for sc in 0u32..256 {
            // Skip extended scan codes in the 0..256 non-extended table; non-extended typing keys live in 0x01..=0x58.
            let vk = unsafe { MapVirtualKeyExW(sc, MAPVK_VSC_TO_VK_EX, hkl) };
            if vk == 0 {
                continue;
            }
            base[sc as usize] = translate_single(vk, sc, false, false, hkl);
            shift[sc as usize] = translate_single(vk, sc, true, false, hkl);
            altgr[sc as usize] = translate_single(vk, sc, false, true, hkl);
            shift_altgr[sc as usize] = translate_single(vk, sc, true, true, hkl);
        }

        Self {
            hkl,
            lang_id: hkl_lang_id(hkl),
            primary_lang: hkl_primary_lang(hkl),
            base,
            shift,
            altgr,
            shift_altgr,
        }
    }

    /// Construct a layer table directly from `(scan_code, base_char, shift_char)` pairs (used for deterministic tests).
    #[cfg(test)]
    pub fn from_pairs(hkl: HKL, pairs: &[(u8, char, char)]) -> Self {
        let mut base = [ScanTranslation::None; 256];
        let mut shift = [ScanTranslation::None; 256];
        for &(sc, b, s) in pairs {
            if b != '\0' {
                base[sc as usize] = ScanTranslation::Char(b);
            }
            if s != '\0' {
                shift[sc as usize] = ScanTranslation::Char(s);
            }
        }
        Self {
            hkl,
            lang_id: hkl_lang_id(hkl),
            primary_lang: hkl_primary_lang(hkl),
            base,
            shift,
            altgr: [ScanTranslation::None; 256],
            shift_altgr: [ScanTranslation::None; 256],
        }
    }

    #[cfg(test)]
    pub fn with_altgr_pairs(mut self, pairs: &[(u8, char, char)]) -> Self {
        for &(sc, b, s) in pairs {
            self.altgr[sc as usize] = ScanTranslation::Char(b);
            self.shift_altgr[sc as usize] = ScanTranslation::Char(s);
        }
        self
    }

    #[cfg(test)]
    pub fn with_dead_key(mut self, sc: u8, shift: bool, dead: char) -> Self {
        if shift {
            self.shift[sc as usize] = ScanTranslation::DeadKey(dead);
        } else {
            self.base[sc as usize] = ScanTranslation::DeadKey(dead);
        }
        self
    }

    #[cfg(test)]
    pub fn with_ligature(mut self, sc: u8, shift: bool, c1: char, c2: char) -> Self {
        if shift {
            self.shift[sc as usize] = ScanTranslation::Chars(c1, c2);
        } else {
            self.base[sc as usize] = ScanTranslation::Chars(c1, c2);
        }
        self
    }

    /// Language-aware dead-key composition for this layout.
    #[inline]
    pub fn combine_dead(&self, dead: char, base: char) -> Option<char> {
        combine_dead_key_for_lang(self.primary_lang, dead, base)
    }

    /// O(1) lookup for a keystroke slot. Extended keys (`LLKHF_EXTENDED`) bypass the non-extended table.
    #[inline]
    pub fn lookup(&self, slot: &KeyStrokeSlot) -> ScanTranslation {
        let full_sc = reconstruct_scan_code(slot.scan_code, slot.flags);
        if full_sc >= 256 {
            return ScanTranslation::None;
        }
        let idx = full_sc as usize;
        match (slot.shift, slot.altgr) {
            (false, false) => self.base[idx],
            (true, false) => self.shift[idx],
            (false, true) => {
                let entry = self.altgr[idx];
                if entry != ScanTranslation::None {
                    entry
                } else {
                    self.base[idx]
                }
            }
            (true, true) => {
                let entry = self.shift_altgr[idx];
                if entry != ScanTranslation::None {
                    entry
                } else {
                    self.shift[idx]
                }
            }
        }
    }

    /// Lookup for a target/candidate layout: normalizes `Shift` on unicameral scripts (`KA`, `HE`, `AR`)
    /// where `Shift + letter` produces `None` (`KA-QWERTY`), ASCII Latin (`HE`, `KA-MES`), or punctuation (`AR`),
    /// while preserving genuine `Shift` letters (`ჭ, ღ, თ, შ, ჟ, ძ, ჩ` in `KA-QWERTY`; `أ, إ, آ, لأ, لإ, لآ` in `AR`).
    #[inline]
    pub fn lookup_for_target(&self, slot: &KeyStrokeSlot) -> ScanTranslation {
        let full_sc = reconstruct_scan_code(slot.scan_code, slot.flags);
        if full_sc >= 256 {
            return ScanTranslation::None;
        }
        self.translation_for_target(full_sc as u8, slot.shift, slot.altgr)
    }

    /// Render a slice of keystroke slots into a string using this layout's cached layers.
    /// Resolves 2-key dead-key sequences (`´` + `e` -> `é`) and 2-char ligatures (Arabic `لا`).
    /// Returns `None` if an unresolved dead key is still pending at the end of `keys`.
    pub fn render_keys(&self, keys: &[KeyStrokeSlot]) -> Option<String> {
        let (out, pending_dead) = self.render_keys_raw(keys, false);
        if pending_dead.is_some() {
            return None;
        }
        Some(out)
    }

    /// Render a slice of keystroke slots for a target/candidate layout (applying unicameral `Shift` fallback).
    /// Returns `None` if an unresolved dead key is still pending at the end of `keys`.
    pub fn render_keys_for_target(&self, keys: &[KeyStrokeSlot]) -> Option<String> {
        let (out, pending_dead) = self.render_keys_raw(keys, true);
        if pending_dead.is_some() {
            return None;
        }
        Some(out)
    }

    /// Render `keys` returning `(committed_screen_text, pending_trailing_dead_key)`.
    pub fn render_keys_raw(
        &self,
        keys: &[KeyStrokeSlot],
        for_target_unicameral: bool,
    ) -> (String, Option<char>) {
        let mut out = String::with_capacity(keys.len());
        let mut pending_dead: Option<char> = None;

        for slot in keys {
            if slot.vk == VK_SPACE {
                if let Some(d) = pending_dead.take() {
                    out.push(d);
                } else {
                    out.push(' ');
                }
                continue;
            }
            let trans = if for_target_unicameral {
                self.lookup_for_target(slot)
            } else {
                self.lookup(slot)
            };
            match trans {
                ScanTranslation::Char(c) => {
                    if let Some(d) = pending_dead.take() {
                        if let Some(combined) = self.combine_dead(d, c) {
                            out.push(combined);
                        } else {
                            out.push(d);
                            out.push(c);
                        }
                    } else {
                        out.push(c);
                    }
                }
                ScanTranslation::Chars(c1, c2) => {
                    if let Some(d) = pending_dead.take() {
                        out.push(d);
                    }
                    out.push(c1);
                    out.push(c2);
                }
                ScanTranslation::DeadKey(d) => {
                    if let Some(prev_d) = pending_dead.take() {
                        out.push(prev_d);
                    } else {
                        pending_dead = Some(d);
                    }
                }
                ScanTranslation::None => {}
            }
        }

        (out, pending_dead)
    }

    /// Reverse lookup: find `(scan_code, shift)` that produces `ch` in this layout (`base` or `shift`).
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

    /// Extended reverse lookup including `altgr`, `shift_altgr`, and `SGCAPS` uppercase fallback
    /// (e.g. Czech `Č/Ř/Š/Ž/Ě/Ú/Ů`, Swiss `Ü/Ö/Ä`, Italian `È/Ò/À/Ù/Ì` where `base` holds the lowercase vowel).
    pub fn find_char_ext(&self, ch: char) -> Option<(u8, bool, bool)> {
        for sc in 1u8..=255 {
            if self.base[sc as usize] == ScanTranslation::Char(ch) {
                return Some((sc, false, false));
            }
            if self.shift[sc as usize] == ScanTranslation::Char(ch) {
                return Some((sc, true, false));
            }
        }
        for sc in 1u8..=255 {
            if self.altgr[sc as usize] == ScanTranslation::Char(ch) {
                return Some((sc, false, true));
            }
            if self.shift_altgr[sc as usize] == ScanTranslation::Char(ch) {
                return Some((sc, true, true));
            }
        }
        if ch.is_uppercase() {
            let mut lc_iter = ch.to_lowercase();
            if let (Some(lc), None) = (lc_iter.next(), lc_iter.next()) {
                if lc != ch {
                    for sc in 1u8..=255 {
                        if self.base[sc as usize] == ScanTranslation::Char(lc) {
                            return Some((sc, true, false));
                        }
                    }
                }
            }
        }
        None
    }

    /// Reverse dead-key decomposition: find `(dead_slot, base_slot)` in `self` whose composition yields `ch`.
    pub fn find_dead_key_sequence(&self, ch: char) -> Option<((u8, bool, bool), (u8, bool, bool))> {
        if ch.is_ascii() {
            return None;
        }
        for dead_sc in 1u8..=255 {
            for &(dead_sh, dead_ag) in &[(false, false), (true, false), (false, true), (true, true)]
            {
                let ScanTranslation::DeadKey(d) = self.translation_at(dead_sc, dead_sh, dead_ag)
                else {
                    continue;
                };
                for base_sc in 1u8..=255 {
                    for &(base_sh, base_ag) in &[(false, false), (true, false)] {
                        let ScanTranslation::Char(b) =
                            self.translation_at(base_sc, base_sh, base_ag)
                        else {
                            continue;
                        };
                        if self.combine_dead(d, b) == Some(ch) {
                            return Some(((dead_sc, dead_sh, dead_ag), (base_sc, base_sh, base_ag)));
                        }
                    }
                }
            }
        }
        None
    }

    /// Returns `true` if `ch` can be produced in `self` either by a single key or a 2-key dead-key sequence.
    #[inline]
    pub fn can_produce_char(&self, ch: char) -> bool {
        self.find_char_ext(ch).is_some() || self.find_dead_key_sequence(ch).is_some()
    }

    fn find_ligature(&self, c1: char, c2: char) -> Option<(u8, bool, bool)> {
        let target = ScanTranslation::Chars(c1, c2);
        for sc in 1u8..=255 {
            if self.base[sc as usize] == target {
                return Some((sc, false, false));
            }
            if self.shift[sc as usize] == target {
                return Some((sc, true, false));
            }
            if self.altgr[sc as usize] == target {
                return Some((sc, false, true));
            }
            if self.shift_altgr[sc as usize] == target {
                return Some((sc, true, true));
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

    #[inline]
    fn translation_at(&self, sc: u8, shift: bool, altgr: bool) -> ScanTranslation {
        let idx = sc as usize;
        match (shift, altgr) {
            (false, false) => self.base[idx],
            (true, false) => self.shift[idx],
            (false, true) => {
                if self.altgr[idx] != ScanTranslation::None {
                    self.altgr[idx]
                } else {
                    self.base[idx]
                }
            }
            (true, true) => {
                if self.shift_altgr[idx] != ScanTranslation::None {
                    self.shift_altgr[idx]
                } else {
                    self.shift[idx]
                }
            }
        }
    }

    #[inline]
    fn translation_for_target(&self, sc: u8, shift: bool, altgr: bool) -> ScanTranslation {
        let trans = self.translation_at(sc, shift, altgr);
        if shift && !altgr && matches!(self.primary_lang, 0x37 | 0x0D | 0x01) {
            if !is_target_script_letter(self.primary_lang, trans)
                && is_target_script_letter(self.primary_lang, self.base[sc as usize])
            {
                return self.base[sc as usize];
            }
        }
        trans
    }

    /// Convert `text` from `self` (source layout) into `dst` (target layout) using scan-code reverse lookup.
    /// Supports `AltGr`, 2-char ligatures (`لا`, `لأ`, `لإ`, `لآ`), 2-key dead-key composition & decomposition
    /// (`"qu'e" <-> "qué"`, `"kalhm;era" <-> "καλημέρα"`), and unicameral (`KA`, `HE`, `AR`) <-> bicameral `Shift` normalization.
    /// Returns `(converted_text, mapped_count, unmapped_alpha_count)`.
    pub fn convert_str_to(&self, text: &str, dst: &LayoutLayerTable) -> (String, usize, usize) {
        let mut out = String::with_capacity(text.len());
        let mut mapped_count = 0usize;
        let mut unmapped_alpha_count = 0usize;
        let chars: Vec<char> = text.chars().collect();
        let mut idx = 0usize;
        let mut at_word_start = true;
        // Georgian (0x37) is unicameral and uses Shift for 7 ordinary letters (თ, ჭ, ღ, შ, ჟ, ძ, ჩ).
        let src_is_georgian = self.primary_lang == 0x37;
        let dst_is_bicameral = !matches!(dst.primary_lang, 0x37 | 0x0D | 0x01);

        while idx < chars.len() {
            let ch = chars[idx];
            if ch.is_whitespace() {
                out.push(ch);
                at_word_start = true;
                idx += 1;
                continue;
            }

            // 1. Check 2-char ligature in source layout (e.g. Arabic `لا`, `لأ`, `لإ`, `لآ`):
            if idx + 1 < chars.len() {
                if let Some((sc, shift, altgr)) = self.find_ligature(ch, chars[idx + 1]) {
                    let effective_shift = if src_is_georgian && dst_is_bicameral && !at_word_start {
                        false
                    } else {
                        shift
                    };
                    match dst.translation_for_target(sc, effective_shift, altgr) {
                        ScanTranslation::Char(tc) => {
                            out.push(tc);
                            mapped_count += 2;
                            at_word_start = false;
                            idx += 2;
                            continue;
                        }
                        ScanTranslation::Chars(tc1, tc2) => {
                            out.push(tc1);
                            out.push(tc2);
                            mapped_count += 2;
                            at_word_start = false;
                            idx += 2;
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            // 2. Single-character lookup across base, shift, altgr, shift_altgr (+ SGCAPS):
            if let Some((sc, shift, altgr)) = self.find_char_ext(ch) {
                let effective_shift = if src_is_georgian && dst_is_bicameral && !at_word_start {
                    false
                } else {
                    shift
                };
                match dst.translation_for_target(sc, effective_shift, altgr) {
                    ScanTranslation::DeadKey(dead_ch) => {
                        // 2a. 2-key dead-key composition into `dst` (e.g. `"qu'e"` -> `"qué"`, `";a"` -> `"ά"`):
                        if idx + 1 < chars.len() {
                            if let Some((sc2, sh2, ag2)) = self.find_char_ext(chars[idx + 1]) {
                                let eff_sh2 = if src_is_georgian && dst_is_bicameral {
                                    false
                                } else {
                                    sh2
                                };
                                if let ScanTranslation::Char(base_ch) =
                                    dst.translation_for_target(sc2, eff_sh2, ag2)
                                {
                                    if let Some(composed) = dst.combine_dead(dead_ch, base_ch) {
                                        out.push(composed);
                                        mapped_count += 2;
                                        at_word_start = false;
                                        idx += 2;
                                        continue;
                                    }
                                }
                            }
                        }
                        out.push(dead_ch);
                        mapped_count += 1;
                        at_word_start = false;
                        idx += 1;
                        continue;
                    }
                    ScanTranslation::Char(target_ch) => {
                        out.push(target_ch);
                        mapped_count += 1;
                        at_word_start = false;
                        idx += 1;
                        continue;
                    }
                    ScanTranslation::Chars(tc1, tc2) => {
                        out.push(tc1);
                        out.push(tc2);
                        mapped_count += 1;
                        at_word_start = false;
                        idx += 1;
                        continue;
                    }
                    ScanTranslation::None => {}
                }
            }

            // 3. Precomposed dead-key decomposition in `self` (e.g. `"qué"` on ES -> `"qu'e"` on EN, `"καλημέρα"` on EL -> `"kalhm;era"` on EN):
            if let Some(((d_sc, d_sh, d_ag), (b_sc, b_sh, b_ag))) = self.find_dead_key_sequence(ch)
            {
                let t1 = dst.translation_for_target(d_sc, d_sh, d_ag);
                let t2 = dst.translation_for_target(b_sc, b_sh, b_ag);
                if let (ScanTranslation::Char(c1), ScanTranslation::Char(c2)) = (t1, t2) {
                    out.push(c1);
                    out.push(c2);
                    mapped_count += 1;
                    at_word_start = false;
                    idx += 1;
                    continue;
                }
            }

            if ch.is_alphabetic() {
                unmapped_alpha_count += 1;
            }
            out.push(ch);
            at_word_start = false;
            idx += 1;
        }

        (out, mapped_count, unmapped_alpha_count)
    }
}

fn translate_single(
    vk: u32,
    scan_code: u32,
    shift: bool,
    altgr: bool,
    hkl: HKL,
) -> ScanTranslation {
    let mut key_state = [0u8; 256];
    if shift {
        key_state[VK_SHIFT_IDX] = 0x80;
    }
    if altgr {
        key_state[VK_CONTROL_IDX] = 0x80;
        key_state[VK_LCONTROL_IDX] = 0x80;
        key_state[VK_MENU_IDX] = 0x80;
        key_state[VK_RMENU_IDX] = 0x80;
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
        let dead_ch = char::from_u32(buf[0] as u32).unwrap_or('´');
        ScanTranslation::DeadKey(dead_ch)
    } else if rc == 0 {
        ScanTranslation::None
    } else {
        let s = String::from_utf16_lossy(&buf[..rc as usize]);
        let mut chars = s.chars().filter(|c| !c.is_control());
        match (chars.next(), chars.next()) {
            (Some(c1), Some(c2)) => ScanTranslation::Chars(c1, c2),
            (Some(c1), None) => ScanTranslation::Char(c1),
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

/// Returns `true` if the rendered token is unconditionally immune (URL, protocol, UNC/drive path, CLI flag).
pub fn is_unconditionally_immune_token(rendered: &str) -> bool {
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
        || lower.starts_with(r"\\")
        || trimmed.contains("://")
    {
        return true;
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        return true;
    }
    false
}

/// Returns `true` if the rendered token looks like a URL, file path, or CLI flag (`AGENTS.md` #7).
pub fn is_immune_token(rendered: &str) -> bool {
    let trimmed = rendered.trim();
    if is_unconditionally_immune_token(trimmed) {
        return true;
    }
    if trimmed.starts_with('\\') {
        return true;
    }
    // Leading '/' is a Unix path or slash-command ONLY when followed by ASCII characters
    // (on Hebrew layout, physical 'Q' is '/', so English "question" typed on Hebrew starts with '/' followed by Hebrew!).
    if let Some(rest) = trimmed.strip_prefix('/') {
        if rest.is_empty() || rest.chars().all(|c| c.is_ascii()) {
            return true;
        }
    }
    // Internal backslash is a path separator (whereas a single trailing '\' on US QWERTY is scan code 0x2B = Italian 'ù').
    if let Some(without_trailing_bs) = trimmed.strip_suffix('\\') {
        if without_trailing_bs.contains('\\') {
            return true;
        }
        return false;
    }
    trimmed.contains('\\')
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
            altgr: false,
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
        assert!(is_immune_token(r"C:\projects\SmartBlink"));
        assert!(is_immune_token("--release"));
        assert!(is_immune_token("/usr/bin"));
        assert!(!is_immune_token("/וקדאןםמ")); // Hebrew "question" where 'q' is '/'
        assert!(!is_immune_token(r"pi\")); // Italian "più" where 'ù' is '\'
        assert!(!is_immune_token("ghbdtn"));
        assert!(!is_immune_token("привет"));
    }

    #[test]
    fn dead_key_ligature_and_altgr_rendering_work() {
        // 1. Dead key acute ('´' on 0x1A) + 'e' (0x12) -> "é":
        let es = LayoutLayerTable::from_pairs(
            HKL(0x040A040A as _),
            &[(0x2E, 'c', 'C'), (0x1E, 'a', 'A'), (0x21, 'f', 'F'), (0x12, 'e', 'E')],
        )
        .with_dead_key(0x1A, false, '´');

        let cafe_keys = [
            slot(0x43, 0x2E, false),
            slot(0x41, 0x1E, false),
            slot(0x46, 0x21, false),
            slot(0xDB, 0x1A, false), // dead key '´'
            slot(0x45, 0x12, false), // 'e' -> 'é'
        ];
        assert_eq!(es.render_keys(&cafe_keys[..4]), None); // unresolved dead key at end returns None
        assert_eq!(es.render_keys(&cafe_keys).as_deref(), Some("café"));

        // 2. Arabic 2-char ligature 'لا' on scan code 0x30 ('B'):
        let ar = LayoutLayerTable::from_pairs(HKL(0x04010401 as _), &[(0x1F, 'س', 'س'), (0x32, 'م', 'م')])
            .with_ligature(0x30, false, 'ل', 'ا');
        let us = LayoutLayerTable::from_pairs(
            HKL(0x04090409 as _),
            &[(0x1F, 's', 'S'), (0x30, 'b', 'B'), (0x32, 'm', 'M')],
        );
        let salam_keys = [
            slot(0x53, 0x1F, false),
            slot(0x42, 0x30, false),
            slot(0x4D, 0x32, false),
        ];
        assert_eq!(ar.render_keys(&salam_keys).as_deref(), Some("سلام"));
        assert_eq!(ar.convert_str_to("سلام", &us).0, "sbm");
        assert_eq!(us.convert_str_to("sbm", &ar).0, "سلام");

        // 3. Polish AltGr+Z -> 'ż':
        let pl = LayoutLayerTable::from_pairs(
            HKL(0x04150415 as _),
            &[(0x32, 'm', 'M'), (0x18, 'o', 'O'), (0x2C, 'z', 'Z'), (0x12, 'e', 'E')],
        )
        .with_altgr_pairs(&[(0x2C, 'ż', 'Ż')]);
        let moze_keys = [
            slot(0x4D, 0x32, false),
            slot(0x4F, 0x18, false),
            KeyStrokeSlot {
                vk: 0x5A,
                scan_code: 0x2C,
                flags: 0,
                ms_tick: 120,
                shift: false,
                altgr: true,
            },
            slot(0x45, 0x12, false),
        ];
        assert_eq!(pl.render_keys(&moze_keys).as_deref(), Some("może"));
    }

    #[test]
    fn host_active_layouts_and_tounicodeex_cache_work() {
        let layouts = active_layouts();
        assert!(!layouts.is_empty(), "host must have at least one active HKL");
        let mut cache = LayoutCache::new();
        let table = cache.get_or_build(layouts[0]);
        assert_eq!(table.hkl, layouts[0]);
    }

    #[test]
    fn m13_dead_keys_ligatures_sgcaps_and_unicameral_shift_suite() {
        // 1. Polish Programmer (~ dead key -> ą/ć/ę/ł/ń/ó/ś/ź/ż, ´ + c -> ć) vs Spanish (´ + c -> ç, ~ + n -> ñ):
        assert_eq!(combine_dead_key_for_lang(0x15, '~', 'a'), Some('ą'));
        assert_eq!(combine_dead_key_for_lang(0x15, '~', 'l'), Some('ł'));
        assert_eq!(combine_dead_key_for_lang(0x15, '~', 'x'), Some('ź'));
        assert_eq!(combine_dead_key_for_lang(0x15, '~', 'z'), Some('ż'));
        assert_eq!(combine_dead_key_for_lang(0x15, '´', 'c'), Some('ć'));
        assert_eq!(combine_dead_key_for_lang(0x0A, '´', 'c'), Some('ç'));
        assert_eq!(combine_dead_key_for_lang(0x0A, '~', 'n'), Some('ñ'));

        // 2. Greek dialytika + tonos ('΅' on Shift+';' 0x27) -> 'ΐ' / 'ΰ':
        assert_eq!(combine_dead_key_for_lang(0x08, '΅', 'ι'), Some('ΐ'));
        assert_eq!(combine_dead_key_for_lang(0x08, '΅', 'υ'), Some('ΰ'));

        // 3. Dead-key composition & decomposition in convert_str_to (Spanish "qu'e" <-> "qué"):
        let us = LayoutLayerTable::from_pairs(
            HKL(0x04090409 as _),
            &[
                (0x10, 'q', 'Q'),
                (0x16, 'u', 'U'),
                (0x28, '\'', '"'),
                (0x12, 'e', 'E'),
            ],
        );
        let es = LayoutLayerTable::from_pairs(
            HKL(0x040A040A as _),
            &[(0x10, 'q', 'Q'), (0x16, 'u', 'U'), (0x12, 'e', 'E')],
        )
        .with_dead_key(0x28, false, '´');

        assert_eq!(us.convert_str_to("qu'e", &es).0, "qué");
        assert_eq!(es.convert_str_to("qué", &us).0, "qu'e");

        // 4. SGCAPS uppercase fallback (Czech 'Č' where 0x05 is normal='č', shift='4'):
        let cs = LayoutLayerTable::from_pairs(
            HKL(0x04050405 as _),
            &[(0x05, 'č', '4'), (0x1E, 'a', 'A'), (0x1F, 's', 'S')],
        );
        let us_num = LayoutLayerTable::from_pairs(
            HKL(0x04090409 as _),
            &[(0x05, '4', '$'), (0x1E, 'a', 'A'), (0x1F, 's', 'S')],
        );
        assert_eq!(cs.convert_str_to("Čas", &us_num).0, "$as");

        // 5. Georgian KA-QWERTY Shift->None normalization & Hebrew Shift->Latin normalization:
        let ka_qw = LayoutLayerTable::from_pairs(
            HKL(0x04370437 as _),
            &[
                (0x23, 'ჰ', '\0'), // Shift+H is None in kbdgeoqw.dll!
                (0x12, 'ე', '\0'),
                (0x25, 'ლ', '\0'),
                (0x18, 'ო', '\0'),
                (0x13, 'რ', 'ღ'), // Genuine Shift letter in KA-QWERTY
                (0x20, 'დ', '\0'),
                (0x1F, 'ს', 'შ'),
            ],
        );

        let us_hello = LayoutLayerTable::from_pairs(
            HKL(0x04090409 as _),
            &[
                (0x23, 'h', 'H'),
                (0x12, 'e', 'E'),
                (0x25, 'l', 'L'),
                (0x18, 'o', 'O'),
                (0x13, 'r', 'R'),
                (0x20, 'd', 'D'),
                (0x1F, 's', 'S'),
            ],
        );
        assert_eq!(us_hello.convert_str_to("Hello", &ka_qw).0, "ჰელლო");
        assert_eq!(
            us_hello.convert_str_to("dRes", &ka_qw).0,
            "დღეს" // 'ღ' preserved on Shift+R
        );
    }
}
