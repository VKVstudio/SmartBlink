#![allow(dead_code)]
//! Language profile loader (`SMBP` v1) and hypothesis scoring engine (M4).
//! Architectural invariants (AGENTS.md #5, #6, #7; ARCHITECTURE.md §3.2, §3.3):
//! - Embedded binary profiles (`assets/profiles/*.bin`) with sorted `u16` bigram and `u32` trigram tables.
//! - Winner hypothesis must differ from current window layout, exceed `ABSOLUTE_THRESHOLD`,
//!   and beat both current layout and runner-up by at least `MARGIN_THRESHOLD`.
//! - False positives are worse than silence: short words (< 4 chars), immune tokens, and
//!   ambiguous bilingual homographs stay silent.

use crate::layout::{self, LayoutCache, LayoutLayerTable};
use crate::worker::KeyStrokeSlot;
use std::sync::OnceLock;
use windows::Win32::UI::Input::KeyboardAndMouse::HKL;

pub const LANG_ARABIC: u16 = 0x01;
pub const LANG_CZECH: u16 = 0x05;
pub const LANG_GERMAN: u16 = 0x07;
pub const LANG_GREEK: u16 = 0x08;
pub const LANG_ENGLISH: u16 = 0x09;
pub const LANG_SPANISH: u16 = 0x0A;
pub const LANG_FRENCH: u16 = 0x0C;
pub const LANG_HEBREW: u16 = 0x0D;
pub const LANG_ITALIAN: u16 = 0x10;
pub const LANG_DUTCH: u16 = 0x13;
pub const LANG_POLISH: u16 = 0x15;
pub const LANG_PORTUGUESE: u16 = 0x16;
pub const LANG_RUSSIAN: u16 = 0x19;
pub const LANG_SWEDISH: u16 = 0x1D;
pub const LANG_TURKISH: u16 = 0x1F;
pub const LANG_UKRAINIAN: u16 = 0x22;
pub const LANG_BELARUSIAN: u16 = 0x23;
pub const LANG_ARMENIAN: u16 = 0x2B;
pub const LANG_GEORGIAN: u16 = 0x37;
pub const LANG_KAZAKH: u16 = 0x3F;

pub const MIN_WORD_LEN: usize = 3;
pub const ABSOLUTE_THRESHOLD: i32 = 18;
pub const MARGIN_THRESHOLD: i32 = 45;
pub const CURRENT_MAX_PLAUSIBLE_SCORE: i32 = -5;

const OOV_IDX: u8 = 0xFF;
const FLOOR_SCORE: i32 = -200;

pub static EMBEDDED_PROFILE_BLOBS: &[(u16, &[u8])] = &[
    (LANG_ENGLISH, include_bytes!("../assets/profiles/en.bin")),
    (LANG_RUSSIAN, include_bytes!("../assets/profiles/ru.bin")),
    (LANG_UKRAINIAN, include_bytes!("../assets/profiles/uk.bin")),
    (LANG_BELARUSIAN, include_bytes!("../assets/profiles/be.bin")),
    (LANG_KAZAKH, include_bytes!("../assets/profiles/kk.bin")),
    (LANG_GERMAN, include_bytes!("../assets/profiles/de.bin")),
    (LANG_FRENCH, include_bytes!("../assets/profiles/fr.bin")),
    (LANG_SPANISH, include_bytes!("../assets/profiles/es.bin")),
    (LANG_PORTUGUESE, include_bytes!("../assets/profiles/pt.bin")),
    (LANG_ITALIAN, include_bytes!("../assets/profiles/it.bin")),
    (LANG_TURKISH, include_bytes!("../assets/profiles/tr.bin")),
    (LANG_POLISH, include_bytes!("../assets/profiles/pl.bin")),
    (LANG_DUTCH, include_bytes!("../assets/profiles/nl.bin")),
    (LANG_SWEDISH, include_bytes!("../assets/profiles/sv.bin")),
    (LANG_CZECH, include_bytes!("../assets/profiles/cs.bin")),
    (LANG_GREEK, include_bytes!("../assets/profiles/el.bin")),
    (LANG_HEBREW, include_bytes!("../assets/profiles/he.bin")),
    (LANG_ARABIC, include_bytes!("../assets/profiles/ar.bin")),
    (LANG_ARMENIAN, include_bytes!("../assets/profiles/hy.bin")),
    (LANG_GEORGIAN, include_bytes!("../assets/profiles/ka.bin")),
];

#[derive(Debug, Clone)]
pub struct LanguageProfile {
    pub script: u8,
    pub primary_lang: u16,
    pub vowel_mask: u64,
    pub default_bigram_score: i8,
    pub default_trigram_score: i8,
    pub oov_char_penalty: i8,
    alphabet: Vec<u16>,
    bigrams: Vec<(u16, i8)>,
    trigrams: Vec<(u32, i8)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectionVerdict {
    pub current_hkl: HKL,
    pub target_hkl: HKL,
    pub current_score: i32,
    pub best_score: i32,
    pub margin: i32,
    pub rendered_target: String,
}

impl LanguageProfile {
    /// Parse an `SMBP` v1 binary profile blob.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 26 || &bytes[0..4] != b"SMBP" || bytes[4] != 1 {
            return None;
        }
        let script = bytes[5];
        let primary_lang = u16::from_le_bytes([bytes[6], bytes[7]]);
        let alpha_count = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
        let vowel_mask = u64::from_le_bytes([
            bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17],
        ]);
        let bigram_count = u16::from_le_bytes([bytes[18], bytes[19]]) as usize;
        let trigram_count = u16::from_le_bytes([bytes[20], bytes[21]]) as usize;
        let default_bigram_score = bytes[22] as i8;
        let default_trigram_score = bytes[23] as i8;
        let oov_char_penalty = bytes[24] as i8;

        let expected_len = 26 + alpha_count * 2 + bigram_count * 3 + trigram_count * 5;
        if bytes.len() != expected_len {
            return None;
        }

        let mut offset = 26;
        let mut alphabet = Vec::with_capacity(alpha_count);
        for _ in 0..alpha_count {
            let code = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
            alphabet.push(code);
            offset += 2;
        }

        let mut bigrams = Vec::with_capacity(bigram_count);
        for _ in 0..bigram_count {
            let key = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
            let score = bytes[offset + 2] as i8;
            bigrams.push((key, score));
            offset += 3;
        }

        let mut trigrams = Vec::with_capacity(trigram_count);
        for _ in 0..trigram_count {
            let key = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            let score = bytes[offset + 4] as i8;
            trigrams.push((key, score));
            offset += 5;
        }

        Some(Self {
            script,
            primary_lang,
            vowel_mask,
            default_bigram_score,
            default_trigram_score,
            oov_char_penalty,
            alphabet,
            bigrams,
            trigrams,
        })
    }

    /// Look up 1-based alphabet index (`1..=alpha_count`) for a lowercase BMP character.
    #[inline]
    pub fn char_index(&self, ch: char) -> Option<u8> {
        let code = ch as u32;
        if code > 0xFFFF {
            return None;
        }
        let code16 = code as u16;
        self.alphabet
            .binary_search(&code16)
            .ok()
            .map(|pos| (pos + 1) as u8)
    }

    #[inline]
    fn is_vowel_idx(&self, idx: u8) -> bool {
        idx > 0 && idx < 64 && ((self.vowel_mask >> idx) & 1) != 0
    }

    #[inline]
    fn lookup_bigram(&self, a: u8, b: u8) -> i32 {
        if a == OOV_IDX || b == OOV_IDX {
            return self.oov_char_penalty as i32;
        }
        let key = ((a as u16) << 8) | (b as u16);
        match self.bigrams.binary_search_by_key(&key, |&(k, _)| k) {
            Ok(pos) => self.bigrams[pos].1 as i32,
            Err(_) => self.default_bigram_score as i32,
        }
    }

    #[inline]
    fn lookup_trigram(&self, a: u8, b: u8, c: u8) -> i32 {
        if a == OOV_IDX || b == OOV_IDX || c == OOV_IDX {
            return (self.oov_char_penalty as i32 * 3) / 4;
        }
        let key = ((a as u32) << 16) | ((b as u32) << 8) | (c as u32);
        match self.trigrams.binary_search_by_key(&key, |&(k, _)| k) {
            Ok(pos) => self.trigrams[pos].1 as i32,
            Err(_) => {
                // Bigram backoff: only when both constituent bigrams (a, b) and (b, c) are strongly
                // attested (score >= 15) in this language, soften the missing-trigram penalty.
                let bg1 = self.lookup_bigram(a, b);
                let bg2 = self.lookup_bigram(b, c);
                if bg1 >= 15 && bg2 >= 15 {
                    -15
                } else {
                    self.default_trigram_score as i32
                }
            }
        }
    }

    /// Score a single rendered candidate token against this language profile.
    /// Returns a length-normalized score (roughly `-200..+100`).
    /// Works on tokens of any length `>= 1` so manual fix / burst expansion can score short words too.
    pub fn score_word(&self, rendered: &str) -> i32 {
        let trimmed = rendered.trim();
        let mut chars: Vec<char> = trimmed
            .chars()
            .flat_map(|c| c.to_lowercase())
            .collect();

        if chars.is_empty() {
            return FLOOR_SCORE;
        }

        // Strip leading and trailing sentence punctuation marks that are not letters in this alphabet
        // (e.g. "hello,", "привет?.", "все,"), with a small penalty in case another layout maps that key to a letter.
        let mut stripped_punct_count = 0usize;
        while let Some(&first_ch) = chars.first() {
            if is_sentence_punct(first_ch) && self.char_index(first_ch).is_none() && chars.len() > 1 {
                chars.remove(0);
                stripped_punct_count += 1;
            } else {
                break;
            }
        }
        while let Some(&last_ch) = chars.last() {
            if is_sentence_punct(last_ch) && self.char_index(last_ch).is_none() && chars.len() > 1 {
                chars.pop();
                stripped_punct_count += 1;
            } else {
                break;
            }
        }

        // Remove internal apostrophes when not part of this alphabet (e.g. English contractions "don't", "it's"),
        // while keeping them if they ARE letters in the target alphabet (e.g. 'э' in RU was already mapped).
        if self.char_index('\'').is_none() && chars.len() >= 3 {
            chars.retain(|&c| c != '\'' && c != '’');
        }

        let n = chars.len();
        if n == 0 {
            return FLOOR_SCORE;
        }

        let mut indices = [0u8; 68];
        let cap = n.min(64);
        indices[0] = 0; // start boundary '^'

        let mut in_alpha_count = 0usize;
        let mut oov_count = 0usize;
        let mut vowel_count = 0usize;
        let mut max_consonant_run = 0usize;
        let mut cur_consonant_run = 0usize;
        let mut max_vowel_run = 0usize;
        let mut cur_vowel_run = 0usize;
        let mut max_repeat_run = 1usize;
        let mut cur_repeat_run = 1usize;

        for (i, &ch) in chars[..cap].iter().enumerate() {
            if i > 0 {
                if ch == chars[i - 1] {
                    cur_repeat_run += 1;
                    if cur_repeat_run > max_repeat_run {
                        max_repeat_run = cur_repeat_run;
                    }
                } else {
                    cur_repeat_run = 1;
                }
            }

            match self.char_index(ch) {
                Some(idx) => {
                    indices[i + 1] = idx;
                    in_alpha_count += 1;
                    if self.is_vowel_idx(idx) {
                        vowel_count += 1;
                        cur_vowel_run += 1;
                        cur_consonant_run = 0;
                        if cur_vowel_run > max_vowel_run {
                            max_vowel_run = cur_vowel_run;
                        }
                    } else {
                        cur_consonant_run += 1;
                        cur_vowel_run = 0;
                        if cur_consonant_run > max_consonant_run {
                            max_consonant_run = cur_consonant_run;
                        }
                    }
                }
                None => {
                    indices[i + 1] = OOV_IDX;
                    oov_count += 1;
                    cur_consonant_run = 0;
                    cur_vowel_run = 0;
                }
            }
        }
        indices[cap + 1] = 0; // end boundary '$'

        if in_alpha_count == 0 {
            return FLOOR_SCORE;
        }

        let seq = &indices[..=cap + 1];
        let mut total: i32 = 0;

        for win in seq.windows(2) {
            total += self.lookup_bigram(win[0], win[1]);
        }
        for win in seq.windows(3) {
            total += self.lookup_trigram(win[0], win[1], win[2]);
        }

        // Phonotactic & orthographic structure penalties:
        if oov_count > 0 {
            total -= (oov_count as i32) * 85;
        }
        if stripped_punct_count > 0 {
            total -= (stripped_punct_count.min(2) as i32) * 14;
        }
        if vowel_count == 0 {
            if cap >= 3 {
                // A 3+ letter alphabetic word with zero vowels (e.g. "ghbdtn", "rfr", "xnj") is strongly implausible.
                total -= (cap as i32) * 42;
            } else if cap == 2 {
                total -= 55;
            } else if cap == 1 && self.lookup_trigram(0, indices[1], 0) < 0 {
                // Single consonant that is not a standalone 1-letter word in this language (e.g. "b", "d", "z" in EN).
                total -= 65;
            }
        }
        if max_consonant_run >= 5 {
            total -= ((max_consonant_run - 4) as i32) * 55;
        }
        if max_vowel_run >= 3 {
            total -= ((max_vowel_run - 2) as i32) * 55;
        }
        if max_repeat_run >= 3 {
            total -= 90;
        }

        let steps = (cap + 1) as i32;
        (total / steps).clamp(FLOOR_SCORE, 120)
    }
}

#[inline]
fn is_sentence_punct(ch: char) -> bool {
    matches!(
        ch,
        '.' | ',' | '!' | '?' | ':' | ';' | '"' | '\'' | '(' | ')' | '«' | '»'
    )
}

/// Returns `true` if the token is immune from detection (URLs, paths, CLI flags, numbers, snake_case).
pub fn is_immune_word(rendered: &str) -> bool {
    let trimmed = rendered.trim();
    if layout::is_immune_token(trimmed) {
        return true;
    }
    // Strip trailing punctuation before checking for path/code characters so a trailing '/'
    // (which is '.' on the Russian layout!) does not falsely mark a sentence-ending word as a Unix path.
    let core = trimmed.trim_end_matches(|c: char| !c.is_alphabetic() && !c.is_ascii_digit());
    if core.is_empty() {
        return false;
    }
    core.chars()
        .any(|c| c.is_ascii_digit() || c == '_' || c == '@' || c == '\\')
}

/// Return all 20 parsed built-in `LanguageProfile` instances.
pub fn all_profiles() -> &'static [LanguageProfile] {
    static ALL_PROFILES: OnceLock<Vec<LanguageProfile>> = OnceLock::new();
    ALL_PROFILES.get_or_init(|| {
        EMBEDDED_PROFILE_BLOBS
            .iter()
            .map(|&(lang, bytes)| {
                let p = LanguageProfile::from_bytes(bytes)
                    .expect("valid embedded SMBP v1 profile binary");
                debug_assert_eq!(p.primary_lang, lang);
                p
            })
            .collect()
    })
}

/// Get the static `LanguageProfile` for a Win32 primary language ID.
pub fn profile_for_primary_lang(primary_lang: u16) -> Option<&'static LanguageProfile> {
    all_profiles()
        .iter()
        .find(|p| p.primary_lang == primary_lang)
}

/// Evaluate a slice of keystroke slots against pre-built `LayoutLayerTable`s.
pub fn evaluate_tables(
    word_keys: &[KeyStrokeSlot],
    current_hkl: HKL,
    tables: &[&LayoutLayerTable],
) -> Option<DetectionVerdict> {
    if word_keys.len() < MIN_WORD_LEN || tables.len() < 2 {
        return None;
    }

    // Find current layout table and check immunity in the current layout first.
    let current_table = tables
        .iter()
        .copied()
        .find(|t| t.hkl == current_hkl)
        .or_else(|| {
            let cur_lang = layout::hkl_primary_lang(current_hkl);
            tables.iter().copied().find(|t| t.primary_lang == cur_lang)
        })?;

    let current_rendered = current_table.render_keys(word_keys)?;
    if is_immune_word(&current_rendered) {
        return None;
    }

    let current_profile = profile_for_primary_lang(current_table.primary_lang)?;
    let current_score = current_profile.score_word(&current_rendered);

    let mut best_hkl = current_table.hkl;
    let mut best_lang = current_table.primary_lang;
    let mut best_score = current_score;
    let mut best_rendered = current_rendered;
    let mut runner_up_score = FLOOR_SCORE;

    for &table in tables {
        if table.hkl == current_table.hkl {
            continue;
        }
        let Some(profile) = profile_for_primary_lang(table.primary_lang) else {
            continue;
        };
        let Some(rendered) = table.render_keys(word_keys) else {
            continue;
        };
        let score = profile.score_word(&rendered);

        if score > best_score {
            runner_up_score = best_score;
            best_score = score;
            best_hkl = table.hkl;
            best_lang = table.primary_lang;
            best_rendered = rendered;
        } else if score > runner_up_score {
            runner_up_score = score;
        }
    }

    if best_hkl == current_table.hkl || best_lang == current_table.primary_lang {
        return None;
    }

    let target_alpha_len = best_rendered.chars().filter(|c| c.is_alphabetic()).count();
    if target_alpha_len < MIN_WORD_LEN {
        return None;
    }

    let margin_vs_current = best_score - current_score;
    let margin_vs_runner_up = best_score - runner_up_score;
    let margin = margin_vs_current.min(margin_vs_runner_up);

    let passes_gates = if target_alpha_len == 3 {
        // Strict high-confidence gate for 3-letter words (e.g. "rfr"->"как", "xnj"->"что", "dct"->"все", "'nj"->"это"):
        // must be strongly implausible in current layout and a high-frequency 3-letter word in target layout.
        current_score <= -30 && best_score >= 34 && margin >= 80
    } else {
        (current_score <= CURRENT_MAX_PLAUSIBLE_SCORE
            && best_score >= ABSOLUTE_THRESHOLD
            && margin >= MARGIN_THRESHOLD)
            || (current_score <= -55 && best_score >= 5 && margin >= 70)
    };

    if passes_gates {
        Some(DetectionVerdict {
            current_hkl,
            target_hkl: best_hkl,
            current_score,
            best_score,
            margin,
            rendered_target: best_rendered,
        })
    } else {
        None
    }
}

/// Evaluate the active word slice using `LayoutCache` for the session's active `HKL`s.
pub fn evaluate_active_word(
    word_keys: &[KeyStrokeSlot],
    current_hkl: HKL,
    active_hkls: &[HKL],
    cache: &mut LayoutCache,
) -> Option<DetectionVerdict> {
    if word_keys.len() < MIN_WORD_LEN {
        return None;
    }
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
    evaluate_tables(word_keys, current_hkl, &tables)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    const US_HKL: HKL = HKL(0x04090409 as _);
    const RU_HKL: HKL = HKL(0x04190419 as _);

    // Full standard US QWERTY and Russian ЙЦУКЕН scan-code tables for deterministic testing.
    const US_LAYOUT: &[(u8, char, char)] = &[
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
        (0x29, '`', '~'),
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

    const RU_LAYOUT: &[(u8, char, char)] = &[
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
        (0x29, 'ё', 'Ё'),
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

    fn make_tables() -> (LayoutLayerTable, LayoutLayerTable) {
        (
            LayoutLayerTable::from_pairs(US_HKL, US_LAYOUT),
            LayoutLayerTable::from_pairs(RU_HKL, RU_LAYOUT),
        )
    }

    /// Convert a text string as typed on `source_layout` into physical `KeyStrokeSlot`s.
    fn type_on_layout(text: &str, layout_pairs: &[(u8, char, char)]) -> Vec<KeyStrokeSlot> {
        let mut out = Vec::with_capacity(text.chars().count());
        for (idx, ch) in text.chars().enumerate() {
            let found = layout_pairs
                .iter()
                .find_map(|&(sc, base, shift)| {
                    if ch == base {
                        Some((sc, false))
                    } else if ch == shift {
                        Some((sc, true))
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| panic!("character {:?} not found in test layout", ch));
            out.push(KeyStrokeSlot {
                vk: 0x41,
                scan_code: found.0 as u16,
                flags: 0,
                ms_tick: (idx as u32) * 40,
                shift: found.1,
            });
        }
        out
    }

    #[test]
    fn embedded_en_and_ru_profiles_parse_validly() {
        let profiles = all_profiles();
        assert_eq!(profiles.len(), 20, "all 20 built-in language profiles must load");
        let total_bytes: usize = EMBEDDED_PROFILE_BLOBS.iter().map(|(_, b)| b.len()).sum();
        assert!(
            total_bytes < 600 * 1024,
            "total embedded profile size {} exceeds 600 KB budget",
            total_bytes
        );

        let en = profile_for_primary_lang(LANG_ENGLISH).expect("EN profile loads");
        let ru = profile_for_primary_lang(LANG_RUSSIAN).expect("RU profile loads");
        assert_eq!(en.primary_lang, LANG_ENGLISH);
        assert_eq!(ru.primary_lang, LANG_RUSSIAN);
        assert_eq!(en.alphabet.len(), 26);
        assert_eq!(ru.alphabet.len(), 33);
        assert!(!en.bigrams.is_empty() && !en.trigrams.is_empty());
        assert!(!ru.bigrams.is_empty() && !ru.trigrams.is_empty());
    }

    #[test]
    fn intra_latin_and_multi_script_conflict_suite_de_fr_tr_pl_uk() {
        const DE_HKL: HKL = HKL(0x04070407 as _);
        const TR_HKL: HKL = HKL(0x041F041F as _);
        const UK_HKL: HKL = HKL(0x04220422 as _);

        // German QWERTZ: Z/Y swapped (`0x15` = 'z', `0x2C` = 'y'), `0x1A` = 'ü', `0x27` = 'ö', `0x28` = 'ä', `0x0C` = 'ß'
        let mut de_pairs = US_LAYOUT.to_vec();
        for entry in &mut de_pairs {
            match entry.0 {
                0x15 => *entry = (0x15, 'z', 'Z'),
                0x2C => *entry = (0x2C, 'y', 'Y'),
                0x1A => *entry = (0x1A, 'ü', 'Ü'),
                0x27 => *entry = (0x27, 'ö', 'Ö'),
                0x28 => *entry = (0x28, 'ä', 'Ä'),
                _ => {}
            }
        }
        de_pairs.push((0x0C, 'ß', '?'));
        let mut us_with_dash = US_LAYOUT.to_vec();
        us_with_dash.push((0x0C, '-', '_'));

        // Turkish QWERTY: `0x1A` = 'ğ', `0x1B` = 'ü', `0x27` = 'ş', `0x28` = 'i', `0x17` = 'ı', `0x33` = 'ö', `0x34` = 'ç'
        let mut tr_pairs = US_LAYOUT.to_vec();
        for entry in &mut tr_pairs {
            match entry.0 {
                0x17 => *entry = (0x17, 'ı', 'I'),
                0x1A => *entry = (0x1A, 'ğ', 'Ğ'),
                0x1B => *entry = (0x1B, 'ü', 'Ü'),
                0x27 => *entry = (0x27, 'ş', 'Ş'),
                0x28 => *entry = (0x28, 'i', 'İ'),
                0x33 => *entry = (0x33, 'ö', 'Ö'),
                0x34 => *entry = (0x34, 'ç', 'Ç'),
                _ => {}
            }
        }

        // Ukrainian Cyrillic (`0x1F` = 'і', `0x1B` = 'ї', `0x28` = 'є')
        let mut uk_pairs = RU_LAYOUT.to_vec();
        for entry in &mut uk_pairs {
            match entry.0 {
                0x1F => *entry = (0x1F, 'і', 'І'),
                0x1B => *entry = (0x1B, 'ї', 'Ї'),
                0x28 => *entry = (0x28, 'є', 'Є'),
                _ => {}
            }
        }

        let us = LayoutLayerTable::from_pairs(US_HKL, &us_with_dash);
        let de = LayoutLayerTable::from_pairs(DE_HKL, &de_pairs);
        let tr = LayoutLayerTable::from_pairs(TR_HKL, &tr_pairs);
        let uk = LayoutLayerTable::from_pairs(UK_HKL, &uk_pairs);
        let tables = [&us, &de, &tr, &uk];

        // 1. German diacritic/QWERTZ words typed while in US layout -> detected as DE:
        for &de_word in &["zurück", "natürlich", "größe", "schön"] {
            let keys = type_on_layout(de_word, &de_pairs);
            let v = evaluate_tables(&keys, US_HKL, &tables)
                .unwrap_or_else(|| panic!("expected DE detection for {:?}", de_word));
            assert_eq!(v.target_hkl, DE_HKL);
            assert_eq!(v.rendered_target, de_word);
        }

        // 2. Turkish words typed while in US layout -> detected as TR:
        for &tr_word in &["teşekkür", "çalışmak"] {
            let keys = type_on_layout(tr_word, &tr_pairs);
            let v = evaluate_tables(&keys, US_HKL, &tables)
                .unwrap_or_else(|| panic!("expected TR detection for {:?}", tr_word));
            assert_eq!(v.target_hkl, TR_HKL);
            assert_eq!(v.rendered_target, tr_word);
        }

        // 3. Ukrainian words typed while in US layout -> detected as UK:
        for &uk_word in &["привіт", "повідомлення"] {
            let keys = type_on_layout(uk_word, &uk_pairs);
            let v = evaluate_tables(&keys, US_HKL, &tables)
                .unwrap_or_else(|| panic!("expected UK detection for {:?}", uk_word));
            assert_eq!(v.target_hkl, UK_HKL);
            assert_eq!(v.rendered_target, uk_word);
        }

        // 4. Shared/valid English words typed in US layout alongside DE, TR, UK must NEVER false-positive:
        for &en_word in &["system", "program", "function", "message", "hello", "world"] {
            let keys = type_on_layout(en_word, &us_with_dash);
            assert_eq!(
                evaluate_tables(&keys, US_HKL, &tables),
                None,
                "intra-Latin false positive on {:?}",
                en_word
            );
        }
    }

    #[test]
    fn detects_russian_typed_on_us_qwerty_ghbdtn_and_cjjotybt() {
        let (us, ru) = make_tables();
        let tables = [&us, &ru];

        // User intends to type Russian words, but active window layout is US_HKL:
        for &(intended_ru, expected_us_garbled) in &[
            ("Привет", "Ghbdtn"),
            ("Сообщение", "Cjj,otybt"),
            ("здравствуйте", "plhfdcndeqnt"),
            ("хорошо", "[jhjij"),
            ("сегодня", "ctujlyz"),
            ("пожалуйста", "gj;fkeqcnf"),
            ("система", "cbcnevf"),
        ] {
            let keys = type_on_layout(intended_ru, RU_LAYOUT);
            if intended_ru == "Привет" || intended_ru == "Сообщение" {
                assert_eq!(us.render_keys(&keys).as_deref(), Some(expected_us_garbled));
            }
            let verdict = evaluate_tables(&keys, US_HKL, &tables)
                .unwrap_or_else(|| panic!("expected RU detection for {:?}", intended_ru));
            assert_eq!(verdict.target_hkl, RU_HKL);
            assert_eq!(verdict.rendered_target, intended_ru);
            assert!(verdict.best_score >= ABSOLUTE_THRESHOLD);
            assert!(verdict.margin >= MARGIN_THRESHOLD);
        }
    }

    #[test]
    fn detects_english_typed_on_ru_jcuken_hello_world_system() {
        let (us, ru) = make_tables();
        let tables = [&us, &ru];

        // User intends to type English words, but active window layout is RU_HKL:
        for &intended_en in &[
            "Hello",
            "world",
            "system",
            "function",
            "message",
            "keyboard",
            "windows",
            "google",
            "youtube",
            "telegram",
            "discord",
            "github",
            "test",
        ] {
            let keys = type_on_layout(intended_en, US_LAYOUT);
            let verdict = evaluate_tables(&keys, RU_HKL, &tables)
                .unwrap_or_else(|| panic!("expected EN detection for {:?}", intended_en));
            assert_eq!(verdict.target_hkl, US_HKL);
            assert_eq!(verdict.rendered_target, intended_en);
            assert!(verdict.best_score >= ABSOLUTE_THRESHOLD);
            assert!(verdict.margin >= MARGIN_THRESHOLD);
        }
    }

    #[test]
    fn real_user_feedback_words_and_punctuated_russian_tokens_detected() {
        let (us, ru) = make_tables();
        let tables = [&us, &ru];

        // Exact words from user's live feedback sentence typed on US layout while intending RU:
        for &(garbled_us, expected_ru) in &[
            ("ghbdtn", "привет"),
            ("yfthyjt", "наерное"), // real user typo in "наверное"
            ("hf,jnftn?", "работает,"),
            ("rfgckjr", "капслок"),
            ("dhjlt", "вроде"),
            ("rfr", "как"),
            ("vbufyekf", "миганула"),
            ("jlby", "один"),
            ("hfp", "раз"),
            ("dct?", "все,"),
            ("rjulf", "когда"),
            ("yfcnhjqrf[", "настройках"),
            ("jnvtnbk?", "отметил,"),
            ("'nj", "это"),
            ("djj,ot", "вообще"),
            ("xnj", "что"),
            ("nfrjt&", "такое?"),
            ("ljk;yj", "должно"),
            ("ghjbc[jlbnm&/", "происходить?."),
        ] {
            let keys = type_on_layout(garbled_us, US_LAYOUT);
            let verdict = evaluate_tables(&keys, US_HKL, &tables).unwrap_or_else(|| {
                panic!(
                    "expected RU detection for garbled {:?} -> {:?}",
                    garbled_us, expected_ru
                )
            });
            assert_eq!(verdict.target_hkl, RU_HKL);
            assert_eq!(verdict.rendered_target, expected_ru);
        }

        // Exact words from user's live feedback typed on RU layout while intending EN:
        for &(garbled_ru, expected_en) in &[
            ("цштвщцы", "windows"),
            ("цштвЩцы", "windOws"),
            ("пщщпду", "google"),
        ] {
            let keys = type_on_layout(garbled_ru, RU_LAYOUT);
            let verdict = evaluate_tables(&keys, RU_HKL, &tables).unwrap_or_else(|| {
                panic!(
                    "expected EN detection for garbled {:?} -> {:?}",
                    garbled_ru, expected_en
                )
            });
            assert_eq!(verdict.target_hkl, US_HKL);
            assert_eq!(verdict.rendered_target, expected_en);
        }
    }

    #[test]
    fn false_positive_suite_stays_silent_on_valid_words_homographs_and_tech_tokens() {
        let (us, ru) = make_tables();
        let tables = [&us, &ru];

        // 1. Valid English words + technical tokens + bilingual homograph "here" + keyboard rows in US_HKL:
        for &valid_en in &[
            "hello",
            "hello,",
            "world",
            "system",
            "function",
            "here", // maps to "руку" on RU — margin rule must keep it silent!
            "tree",
            "free",
            "test",
            "clang",
            "windres",
            "smartblink",
            "struct",
            "option",
            "result",
            "qwerty",
            "asdfgh",
            "zxcvbn",
            "cfg", // < 4 chars
            "git", // < 4 chars
            "src", // < 4 chars
        ] {
            let keys = type_on_layout(valid_en, US_LAYOUT);
            assert_eq!(
                evaluate_tables(&keys, US_HKL, &tables),
                None,
                "false positive on valid EN input {:?}",
                valid_en
            );
        }

        // 2. Valid Russian words + bilingual homograph "руку" + short words in RU_HKL:
        for &valid_ru in &[
            "привет",
            "сообщение",
            "система",
            "функция",
            "клавиатура",
            "хорошо",
            "сегодня",
            "руку", // maps to "here" on EN — margin rule must keep it silent!
            "хлеб",
            "гриб",
            "мир", // < 4 chars
            "дом", // < 4 chars
            "как", // < 4 chars
        ] {
            let keys = type_on_layout(valid_ru, RU_LAYOUT);
            assert_eq!(
                evaluate_tables(&keys, RU_HKL, &tables),
                None,
                "false positive on valid RU input {:?}",
                valid_ru
            );
        }
    }

    #[test]
    fn scoring_benchmark_well_under_100_microseconds_per_word() {
        let (us, ru) = make_tables();
        let tables = [&us, &ru];
        let keys = type_on_layout("Сообщение", RU_LAYOUT);

        let iterations = 1000u32;
        let start = Instant::now();
        for _ in 0..iterations {
            let v = evaluate_tables(&keys, US_HKL, &tables);
            assert!(v.is_some());
        }
        let elapsed = start.elapsed();
        let avg_us = elapsed.as_secs_f64() * 1_000_000.0 / (iterations as f64);
        assert!(
            avg_us < 100.0,
            "average scoring time {:.2} us exceeded 100 us (0.1 ms) budget",
            avg_us
        );
    }
}
