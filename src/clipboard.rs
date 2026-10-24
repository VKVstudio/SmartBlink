//! Selection conversion (`Ctrl+Alt+Z`), byte-for-byte multi-format clipboard save/restore,
//! and stubborn-user RAM session whitelist (M8, ARCHITECTURE.md §3.6, §3.7).
//! Architectural invariants (AGENTS.md #2, #4, #5, #9):
//! - Clipboard preserved byte-for-byte across all HGLOBAL formats, restored in original order even on error paths.
//! - Saved clipboard byte buffers and session whitelist entries are volatile-zeroed on reset/drop; nothing written to disk.
//! - Conversion uses dynamic `LayoutLayerTable` scan-code reverse lookup + language profile scoring (no manual char tables).

use crate::fix;
use crate::hooks;
use crate::layout::{LayoutCache, LayoutLayerTable};
use crate::profile;
use crate::ring::{InputKind, InputRecord};
use std::mem::size_of;
use std::ptr;
use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, HKL, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY,
};

pub const CF_BITMAP: u32 = 2;
pub const CF_METAFILEPICT: u32 = 3;
pub const CF_PALETTE: u32 = 9;
pub const CF_UNICODETEXT: u32 = 13;
pub const CF_ENHMETAFILE: u32 = 14;
const CF_OWNERDISPLAY: u32 = 0x0080;
const CF_DSPTEXT: u32 = 0x0081;
const CF_DSPBITMAP: u32 = 0x0082;
const CF_DSPMETAFILEPICT: u32 = 0x0083;
const CF_DSPENHMETAFILE: u32 = 0x008E;
const CF_GDIOBJFIRST: u32 = 0x0300;
const CF_GDIOBJLAST: u32 = 0x03FF;

const MAX_WHITELIST_ENTRIES: usize = 256;
pub const WHITELIST_MIN_IGNORES: u8 = 3;

const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const VK_C: u16 = 0x43;
const VK_Z: u16 = 0x5A;
const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;
const VK_LSHIFT: u16 = 0xA0;
const VK_RSHIFT: u16 = 0xA1;
const VK_LCONTROL: u16 = 0xA2;
const VK_RCONTROL: u16 = 0xA3;
const VK_LMENU: u16 = 0xA4;
const VK_RMENU: u16 = 0xA5;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct WhitelistEntry {
    word_hash: u64,
    hkl: isize,
    count: u8,
}

impl WhitelistEntry {
    const fn zeroed() -> Self {
        Self {
            word_hash: 0,
            hkl: 0,
            count: 0,
        }
    }
}

/// RAM-only session whitelist ("stubborn user", ARCHITECTURE.md §3.7).
/// Stores 64-bit FNV-1a hashes of lowercased words + HKL so no plaintext words remain in RAM.
pub struct SessionWhitelist {
    entries: [WhitelistEntry; MAX_WHITELIST_ENTRIES],
    len: usize,
    cursor: usize,
}

impl Default for SessionWhitelist {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SessionWhitelist {
    fn drop(&mut self) {
        self.clear();
    }
}

impl SessionWhitelist {
    pub const fn new() -> Self {
        Self {
            entries: [WhitelistEntry::zeroed(); MAX_WHITELIST_ENTRIES],
            len: 0,
            cursor: 0,
        }
    }

    pub fn clear(&mut self) {
        for entry in &mut self.entries {
            unsafe {
                ptr::write_volatile(entry, WhitelistEntry::zeroed());
            }
        }
        self.len = 0;
        self.cursor = 0;
    }

    /// Immediately whitelist `word` (and its prefixes of length `>= MIN_WORD_LEN`) for `hkl`.
    #[allow(dead_code)]
    pub fn add_word(&mut self, word: &str, hkl: HKL) {
        for _ in 0..WHITELIST_MIN_IGNORES {
            self.record_ignored_word(word, hkl);
        }
    }

    /// Record one ignored alert for `word` under `hkl`. Once ignored `WHITELIST_MIN_IGNORES` (3) times
    /// in the session without fixing, the word and its prefixes become whitelisted for the session.
    pub fn record_ignored_word(&mut self, word: &str, hkl: HKL) {
        let chars: Vec<char> = word
            .trim()
            .chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(|c| c.to_lowercase())
            .collect();
        if chars.len() < profile::MIN_WORD_LEN {
            return;
        }
        for prefix_len in profile::MIN_WORD_LEN..=chars.len() {
            let hash = hash_char_slice(&chars[..prefix_len]);
            self.increment_entry(hash, hkl.0 as isize);
        }
    }

    /// Returns `true` if `word` under `hkl` has reached the stubborn-user whitelist threshold.
    pub fn is_whitelisted(&self, word: &str, hkl: HKL) -> bool {
        let chars: Vec<char> = word
            .trim()
            .chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(|c| c.to_lowercase())
            .collect();
        if chars.len() < profile::MIN_WORD_LEN {
            return false;
        }
        let hash = hash_char_slice(&chars);
        let raw_hkl = hkl.0 as isize;
        self.entries[..self.len]
            .iter()
            .any(|e| e.word_hash == hash && e.hkl == raw_hkl && e.count >= WHITELIST_MIN_IGNORES)
    }

    fn increment_entry(&mut self, word_hash: u64, hkl: isize) {
        for e in &mut self.entries[..self.len] {
            if e.word_hash == word_hash && e.hkl == hkl {
                e.count = e.count.saturating_add(1);
                return;
            }
        }
        self.entries[self.cursor] = WhitelistEntry {
            word_hash,
            hkl,
            count: 1,
        };
        self.cursor = (self.cursor + 1) % MAX_WHITELIST_ENTRIES;
        if self.len < MAX_WHITELIST_ENTRIES {
            self.len += 1;
        }
    }
}

fn hash_char_slice(chars: &[char]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &c in chars {
        let v = c as u32;
        for b in v.to_le_bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    if hash == 0 {
        1
    } else {
        hash
    }
}

/// One saved HGLOBAL clipboard format payload. Volatile-zeroed on drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedClipboardFormat {
    pub format: u32,
    pub bytes: Vec<u8>,
}

impl Drop for SavedClipboardFormat {
    fn drop(&mut self) {
        for b in &mut self.bytes {
            unsafe {
                ptr::write_volatile(b, 0);
            }
        }
    }
}

/// Snapshot of all HGLOBAL-backed clipboard formats in exact enumeration order.
#[derive(Debug, Default, Clone)]
pub struct ClipboardSnapshot {
    pub formats: Vec<SavedClipboardFormat>,
}

impl ClipboardSnapshot {
    /// Open clipboard and capture all HGLOBAL-backed formats byte-for-byte in enumeration order.
    pub fn capture() -> windows::core::Result<Self> {
        open_clipboard_with_retry()?;
        let mut formats = Vec::new();
        unsafe {
            let mut fmt = EnumClipboardFormats(0);
            while fmt != 0 {
                if !is_non_hglobal_format(fmt) {
                    if let Ok(handle) = GetClipboardData(fmt) {
                        if !handle.0.is_null() {
                            let hglobal = HGLOBAL(handle.0);
                            let size = GlobalSize(hglobal);
                            if size > 0 {
                                let ptr = GlobalLock(hglobal) as *const u8;
                                if !ptr.is_null() {
                                    let mut data = vec![0u8; size];
                                    ptr::copy_nonoverlapping(ptr, data.as_mut_ptr(), size);
                                    let _ = GlobalUnlock(hglobal);
                                    formats.push(SavedClipboardFormat {
                                        format: fmt,
                                        bytes: data,
                                    });
                                }
                            }
                        }
                    }
                }
                fmt = EnumClipboardFormats(fmt);
            }
            let _ = CloseClipboard();
        }
        Ok(Self { formats })
    }

    /// Restore all captured clipboard formats byte-for-byte in original order.
    pub fn restore(&self) -> windows::core::Result<()> {
        open_clipboard_with_retry()?;
        unsafe {
            let _ = EmptyClipboard();
            for item in &self.formats {
                if item.bytes.is_empty() {
                    continue;
                }
                if let Ok(hglobal) = GlobalAlloc(GMEM_MOVEABLE, item.bytes.len()) {
                    let dst = GlobalLock(hglobal) as *mut u8;
                    if !dst.is_null() {
                        ptr::copy_nonoverlapping(item.bytes.as_ptr(), dst, item.bytes.len());
                        let _ = GlobalUnlock(hglobal);
                        if SetClipboardData(item.format, HANDLE(hglobal.0)).is_err() {
                            let _ = GlobalFree(hglobal);
                        }
                    } else {
                        let _ = GlobalFree(hglobal);
                    }
                }
            }
            let _ = CloseClipboard();
        }
        Ok(())
    }
}

/// RAII guard that guarantees the captured clipboard is restored even on early return or error.
pub struct ClipboardRestoreGuard {
    snapshot: ClipboardSnapshot,
}

impl ClipboardRestoreGuard {
    pub fn new(snapshot: ClipboardSnapshot) -> Self {
        Self { snapshot }
    }
}

impl Drop for ClipboardRestoreGuard {
    fn drop(&mut self) {
        let _ = self.snapshot.restore();
    }
}

fn is_non_hglobal_format(fmt: u32) -> bool {
    matches!(
        fmt,
        CF_BITMAP
            | CF_METAFILEPICT
            | CF_PALETTE
            | CF_ENHMETAFILE
            | CF_OWNERDISPLAY
            | CF_DSPTEXT
            | CF_DSPBITMAP
            | CF_DSPMETAFILEPICT
            | CF_DSPENHMETAFILE
            | CF_GDIOBJFIRST..=CF_GDIOBJLAST
    )
}

fn open_clipboard_with_retry() -> windows::core::Result<()> {
    let mut last_err = None;
    for _ in 0..8 {
        match unsafe { OpenClipboard(HWND::default()) } {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
    Err(last_err.unwrap_or_else(windows::core::Error::from_win32))
}

/// Read `CF_UNICODETEXT` from the clipboard as a Rust `String`.
pub fn read_clipboard_unicode_text() -> windows::core::Result<Option<String>> {
    open_clipboard_with_retry()?;
    let mut out = None;
    unsafe {
        if let Ok(handle) = GetClipboardData(CF_UNICODETEXT) {
            if !handle.0.is_null() {
                let hglobal = HGLOBAL(handle.0);
                let byte_size = GlobalSize(hglobal);
                let ptr = GlobalLock(hglobal) as *const u16;
                if !ptr.is_null() && byte_size >= 2 {
                    let max_units = byte_size / 2;
                    let slice = std::slice::from_raw_parts(ptr, max_units);
                    let nul_pos = slice.iter().position(|&u| u == 0).unwrap_or(max_units);
                    let text = String::from_utf16_lossy(&slice[..nul_pos]);
                    let _ = GlobalUnlock(hglobal);
                    if !text.is_empty() {
                        out = Some(text);
                    }
                }
            }
        }
        let _ = CloseClipboard();
    }
    Ok(out)
}

/// Convert `text` across candidate `LayoutLayerTable` pairs using reverse scan-code lookup + profile scoring.
/// Returns `Some((converted_text, target_hkl))` when a valid layout conversion is found.
pub fn convert_selection_text(
    text: &str,
    current_hkl: HKL,
    tables: &[&LayoutLayerTable],
) -> Option<(String, HKL)> {
    if text.trim().is_empty() || tables.len() < 2 {
        return None;
    }

    let mut best_pair: Option<(String, HKL, i32, bool)> = None;
    let mut fallback_pair: Option<(String, HKL, usize, bool)> = None;

    for &src in tables {
        let src_profile = profile::profile_for_primary_lang(src.primary_lang);
        for &dst in tables {
            if src.hkl == dst.hkl {
                continue;
            }
            let Some(dst_profile) = profile::profile_for_primary_lang(dst.primary_lang) else {
                continue;
            };
            let (converted, mapped_count, unmapped_alpha_count) = src.convert_str_to(text, dst);
            if mapped_count == 0 || converted == text {
                continue;
            }
            if unmapped_alpha_count > 0 {
                // Only allow unmapped alphabetic characters if they already belong to `dst`'s alphabet
                // (e.g. mixed selection "Привет ghbdtn"), never when they belong to a 3rd layout like DE "größe".
                let all_unmapped_in_dst = text.chars().all(|ch| {
                    !ch.is_alphabetic()
                        || src.find_char(ch).is_some()
                        || ch
                            .to_lowercase()
                            .all(|lc| dst_profile.char_index(lc).is_some())
                });
                if !all_unmapped_in_dst {
                    continue;
                }
            }

            let involves_current = dst.hkl == current_hkl || src.hkl == current_hkl;

            // Track best fallback by mapped character count in case the selection is too short for n-grams.
            match &fallback_pair {
                None => {
                    fallback_pair = Some((converted.clone(), dst.hkl, mapped_count, involves_current));
                }
                Some((_, _, best_mapped, best_cur)) => {
                    if mapped_count > *best_mapped
                        || (mapped_count == *best_mapped && involves_current && !*best_cur)
                    {
                        fallback_pair =
                            Some((converted.clone(), dst.hkl, mapped_count, involves_current));
                    }
                }
            }

            let src_score = match src_profile {
                Some(sp) => score_phrase(&text, sp),
                None => -40,
            };
            let dst_score = score_phrase(&converted, dst_profile);
            let gain = dst_score - src_score;

            if gain > 0 {
                match &best_pair {
                    None => {
                        best_pair = Some((converted, dst.hkl, gain, involves_current));
                    }
                    Some((_, _, best_gain, best_cur)) => {
                        if gain > *best_gain || (gain == *best_gain && involves_current && !*best_cur)
                        {
                            best_pair = Some((converted, dst.hkl, gain, involves_current));
                        }
                    }
                }
            }
        }
    }

    if let Some((converted, target_hkl, _, _)) = best_pair {
        return Some((converted, target_hkl));
    }
    fallback_pair.map(|(converted, target_hkl, _, _)| (converted, target_hkl))
}

fn score_phrase(text: &str, prof: &profile::LanguageProfile) -> i32 {
    let mut total = 0i32;
    let mut words = 0i32;
    for raw_token in text.split_whitespace() {
        let token = raw_token.trim_matches(|c: char| !c.is_alphabetic());
        if token.chars().count() >= 2 {
            total += prof.score_word(token);
            words += 1;
        }
    }
    if words == 0 {
        prof.score_word(text.trim())
    } else {
        total
    }
}

/// Detector for the `Ctrl+Alt+Z` selection conversion hotkey (`ARCHITECTURE.md` §3.6).
pub struct SelectionHotkeyDetector {
    ctrl_down: bool,
    alt_down: bool,
    shift_down: bool,
    win_down: bool,
    z_held: bool,
}

impl Default for SelectionHotkeyDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectionHotkeyDetector {
    pub const fn new() -> Self {
        Self {
            ctrl_down: false,
            alt_down: false,
            shift_down: false,
            win_down: false,
            z_held: false,
        }
    }

    pub fn reset(&mut self) {
        self.ctrl_down = false;
        self.alt_down = false;
        self.shift_down = false;
        self.win_down = false;
        self.z_held = false;
    }

    /// Feed one `InputRecord`; returns `true` when `Ctrl+Alt+Z` is pressed cleanly.
    pub fn feed(&mut self, rec: InputRecord) -> bool {
        match rec.kind {
            InputKind::Empty | InputKind::MouseClick => false,
            InputKind::KeyUp => {
                match rec.vk {
                    VK_CONTROL | VK_LCONTROL | VK_RCONTROL => self.ctrl_down = false,
                    VK_MENU | VK_LMENU | VK_RMENU => self.alt_down = false,
                    VK_SHIFT | VK_LSHIFT | VK_RSHIFT => self.shift_down = false,
                    VK_LWIN | VK_RWIN => self.win_down = false,
                    VK_Z => self.z_held = false,
                    _ => {}
                }
                false
            }
            InputKind::KeyDown => {
                match rec.vk {
                    VK_CONTROL | VK_LCONTROL | VK_RCONTROL => {
                        self.ctrl_down = true;
                        false
                    }
                    VK_MENU | VK_LMENU | VK_RMENU => {
                        self.alt_down = true;
                        false
                    }
                    VK_SHIFT | VK_LSHIFT | VK_RSHIFT => {
                        self.shift_down = true;
                        false
                    }
                    VK_LWIN | VK_RWIN => {
                        self.win_down = true;
                        false
                    }
                    VK_Z => {
                        if self.ctrl_down
                            && self.alt_down
                            && !self.shift_down
                            && !self.win_down
                            && !self.z_held
                        {
                            self.z_held = true;
                            true
                        } else {
                            self.z_held = true;
                            false
                        }
                    }
                    _ => false,
                }
            }
        }
    }
}

/// Execute selection conversion in `hwnd` while preserving the user's clipboard byte-for-byte.
pub fn convert_selection_in_window(
    hwnd: HWND,
    current_hkl: HKL,
    active_hkls: &[HKL],
    cache: &mut LayoutCache,
) -> windows::core::Result<bool> {
    if active_hkls.len() < 2 {
        return Ok(false);
    }

    for &hkl in active_hkls {
        let _ = cache.get_or_build(hkl);
    }
    let _ = cache.get_or_build(current_hkl);

    // 1. Capture entire existing clipboard and arm RAII restoration guard (AGENTS.md #9).
    let snapshot = ClipboardSnapshot::capture()?;
    let _restore_guard = ClipboardRestoreGuard::new(snapshot);

    // 2. Empty clipboard and synthesize clean Ctrl+C so the focused control copies active selection.
    if open_clipboard_with_retry().is_ok() {
        unsafe {
            let _ = EmptyClipboard();
            let _ = CloseClipboard();
        }
    }

    let seq_before = unsafe { GetClipboardSequenceNumber() };
    send_synthetic_ctrl_c()?;

    // Wait up to 200 ms (40 * 5 ms) for the target app (including Electron/Chromium) to update clipboard.
    for _ in 0..40 {
        let seq_now = unsafe { GetClipboardSequenceNumber() };
        if seq_now != seq_before {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }

    // 3. Read CF_UNICODETEXT and convert between layouts.
    let Some(selected_text) = read_clipboard_unicode_text()? else {
        return Ok(false);
    };

    let mut tables: Vec<&LayoutLayerTable> = active_hkls
        .iter()
        .filter_map(|hkl| cache.get(*hkl))
        .collect();
    if !tables.iter().any(|t| t.hkl == current_hkl) {
        if let Some(cur_t) = cache.get(current_hkl) {
            tables.push(cur_t);
        }
    }
    let Some((converted_text, target_hkl)) =
        convert_selection_text(&selected_text, current_hkl, &tables)
    else {
        return Ok(false);
    };

    // Normalize CRLF -> LF so KEYEVENTF_UNICODE does not inject duplicate Enter strokes on multiline text.
    let normalized_text = converted_text.replace("\r\n", "\n");

    // 4. Release any held Ctrl/Alt/Shift modifiers and replace active selection via KEYEVENTF_UNICODE.
    let mut inputs = fix::build_modifier_up_inputs();
    inputs.extend(fix::build_unicode_inputs(&normalized_text));
    hooks::begin_suppression();
    let send_res = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    hooks::end_suppression();

    if send_res == 0 {
        return Err(windows::core::Error::from_win32());
    }

    fix::switch_window_layout(hwnd, target_hkl);
    // 5. `_restore_guard` drops here and restores all original clipboard formats in original order.
    Ok(true)
}

fn send_synthetic_ctrl_c() -> windows::core::Result<()> {
    let inputs = [
        // Release Alt and Shift temporarily so the target window receives clean Ctrl+C, not Ctrl+Alt+C.
        vk_input(VK_LMENU, true),
        vk_input(VK_RMENU, true),
        vk_input(VK_MENU, true),
        vk_input(VK_LSHIFT, true),
        vk_input(VK_RSHIFT, true),
        vk_input(VK_SHIFT, true),
        vk_input(VK_RCONTROL, true),
        vk_input(VK_LCONTROL, false),
        vk_input(VK_C, false),
        vk_input(VK_C, true),
        vk_input(VK_LCONTROL, true),
        vk_input(VK_CONTROL, true),
    ];
    hooks::begin_suppression();
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    hooks::end_suppression();
    if sent == 0 {
        return Err(windows::core::Error::from_win32());
    }
    Ok(())
}

fn vk_input(vk: u16, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::PCWSTR;
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;

    fn hkl(id: usize) -> HKL {
        HKL(id as *mut core::ffi::c_void)
    }

    fn sample_en_ru_tables() -> (LayoutLayerTable, LayoutLayerTable) {
        let pairs_en: &[(u8, char, char)] = &[
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
            (0x1E, 'a', 'A'),
            (0x1F, 's', 'S'),
            (0x20, 'd', 'D'),
            (0x21, 'f', 'F'),
            (0x22, 'g', 'G'),
            (0x23, 'h', 'H'),
            (0x24, 'j', 'J'),
            (0x25, 'k', 'K'),
            (0x26, 'l', 'L'),
            (0x2C, 'z', 'Z'),
            (0x2D, 'x', 'X'),
            (0x2E, 'c', 'C'),
            (0x2F, 'v', 'V'),
            (0x30, 'b', 'B'),
            (0x31, 'n', 'N'),
            (0x32, 'm', 'M'),
            (0x33, ',', '<'),
            (0x35, '/', '?'),
        ];
        let pairs_ru: &[(u8, char, char)] = &[
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
            (0x1E, 'ф', 'Ф'),
            (0x1F, 'ы', 'Ы'),
            (0x20, 'в', 'В'),
            (0x21, 'а', 'А'),
            (0x22, 'п', 'П'),
            (0x23, 'р', 'Р'),
            (0x24, 'о', 'О'),
            (0x25, 'л', 'Л'),
            (0x26, 'д', 'Д'),
            (0x2C, 'я', 'Я'),
            (0x2D, 'ч', 'Ч'),
            (0x2E, 'с', 'С'),
            (0x2F, 'м', 'М'),
            (0x30, 'и', 'И'),
            (0x31, 'т', 'Т'),
            (0x32, 'ь', 'Ь'),
            (0x33, 'б', 'Б'),
            (0x35, '.', ','),
        ];
        (
            LayoutLayerTable::from_pairs(hkl(0x0409_0409), pairs_en),
            LayoutLayerTable::from_pairs(hkl(0x0419_0419), pairs_ru),
        )
    }

    #[test]
    fn stubborn_user_whitelist_suppresses_word_and_prefixes_per_hkl() {
        let mut wl = SessionWhitelist::new();
        let en = hkl(0x0409_0409);
        let ru = hkl(0x0419_0419);

        assert!(!wl.is_whitelisted("ghbdtn", en));
        wl.add_word("Ghbdtn", en);

        // Full word and intermediate >=4-char prefixes are whitelisted for EN:
        assert!(wl.is_whitelisted("ghbd", en));
        assert!(wl.is_whitelisted("ghbdt", en));
        assert!(wl.is_whitelisted("ghbdtn", en));
        assert!(wl.is_whitelisted("GHBDTN", en));

        // Different HKL is not affected:
        assert!(!wl.is_whitelisted("ghbdtn", ru));

        wl.clear();
        assert!(!wl.is_whitelisted("ghbdtn", en));
    }

    #[test]
    fn selection_conversion_converts_phrases_both_directions() {
        let (en_table, ru_table) = sample_en_ru_tables();
        let tables = [&en_table, &ru_table];

        // "Ghbdtn? vbh" -> "Привет, мир"
        let (conv_ru, target_ru) =
            convert_selection_text("Ghbdtn? vbh", en_table.hkl, &tables).expect("should convert to RU");
        assert_eq!(conv_ru, "Привет, мир");
        assert_eq!(target_ru, ru_table.hkl);

        // "Руддщ цщкдв" -> "Hello world"
        let (conv_en, target_en) =
            convert_selection_text("Руддщ цщкдв", ru_table.hkl, &tables).expect("should convert to EN");
        assert_eq!(conv_en, "Hello world");
        assert_eq!(target_en, en_table.hkl);
    }

    #[test]
    fn selection_hotkey_detector_triggers_on_ctrl_alt_z_only() {
        let mut det = SelectionHotkeyDetector::new();
        let kd = |vk| InputRecord {
            vk,
            scan_code: 0,
            flags: 0,
            ms_tick: 100,
            kind: InputKind::KeyDown,
            _pad: [0; 3],
        };
        let ku = |vk| InputRecord {
            vk,
            scan_code: 0,
            flags: 0,
            ms_tick: 110,
            kind: InputKind::KeyUp,
            _pad: [0; 3],
        };

        assert!(!det.feed(kd(VK_LCONTROL)));
        assert!(!det.feed(kd(VK_Z))); // Ctrl+Z alone must not trigger
        assert!(!det.feed(ku(VK_Z)));

        assert!(!det.feed(kd(VK_LMENU)));
        assert!(det.feed(kd(VK_Z))); // Ctrl+Alt+Z triggers!
        assert!(!det.feed(kd(VK_Z))); // Auto-repeat ignored while Z is held
        assert!(!det.feed(ku(VK_Z)));
        assert!(!det.feed(ku(VK_LMENU)));
        assert!(!det.feed(ku(VK_LCONTROL)));
    }

    #[test]
    fn clipboard_multi_format_byte_for_byte_roundtrip() {
        // Save the user's real clipboard first and restore it at the end of the test!
        let real_user_clipboard = ClipboardSnapshot::capture().expect("capture real clipboard");
        let _real_restore = ClipboardRestoreGuard::new(real_user_clipboard);

        let fmt_name: Vec<u16> = "SmartBlinkRichTestFormat\0".encode_utf16().collect();
        let custom_fmt = unsafe { RegisterClipboardFormatW(PCWSTR(fmt_name.as_ptr())) };
        assert!(custom_fmt != 0);

        let unicode_str = "SmartBlink Clipboard Test Привет\0";
        let unicode_bytes: Vec<u8> = unicode_str
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let custom_payload: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x42, 0xFF, 0x13, 0x37];

        // Populate test clipboard with both CF_UNICODETEXT and custom binary format.
        let test_state = ClipboardSnapshot {
            formats: vec![
                SavedClipboardFormat {
                    format: CF_UNICODETEXT,
                    bytes: unicode_bytes.clone(),
                },
                SavedClipboardFormat {
                    format: custom_fmt,
                    bytes: custom_payload.clone(),
                },
            ],
        };
        test_state.restore().expect("set test clipboard");

        // Snapshot the test clipboard, clobber the clipboard, then restore via ClipboardRestoreGuard.
        {
            let snap = ClipboardSnapshot::capture().expect("capture test clipboard");
            let _guard = ClipboardRestoreGuard::new(snap);

            // Clobber clipboard with empty state to simulate conversion work.
            open_clipboard_with_retry().expect("open");
            unsafe {
                let _ = EmptyClipboard();
                let _ = CloseClipboard();
            }
        }

        // Verify both CF_UNICODETEXT and the custom binary format were restored byte-for-byte.
        let after = ClipboardSnapshot::capture().expect("capture restored clipboard");
        let restored_text = read_clipboard_unicode_text()
            .expect("read unicode")
            .expect("unicode present");
        assert_eq!(restored_text, "SmartBlink Clipboard Test Привет");

        let restored_custom = after
            .formats
            .iter()
            .find(|f| f.format == custom_fmt)
            .expect("custom format preserved");
        assert_eq!(&restored_custom.bytes[..custom_payload.len()], &custom_payload[..]);
    }
}
