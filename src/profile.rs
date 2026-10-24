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

pub const LANG_ENGLISH: u16 = 0x09;
pub const LANG_RUSSIAN: u16 = 0x19;

pub const MIN_WORD_LEN: usize = 4;
pub const ABSOLUTE_THRESHOLD: i32 = 18;
pub const MARGIN_THRESHOLD: i32 = 45;
pub const CURRENT_MAX_PLAUSIBLE_SCORE: i32 = -5;

const OOV_IDX: u8 = 0xFF;
const FLOOR_SCORE: i32 = -200;

static EN_BYTES: &[u8] = include_bytes!("../assets/profiles/en.bin");
static RU_BYTES: &[u8] = include_bytes!("../assets/profiles/ru.bin");

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
            Err(_) => self.default_trigram_score as i32,
        }
    }

    /// Score a single rendered candidate token against this language profile.
    /// Returns a length-normalized score (roughly `-200..+100`).
    pub fn score_word(&self, rendered: &str) -> i32 {
        let trimmed = rendered.trim();
        let mut chars: Vec<char> = trimmed
            .chars()
            .flat_map(|c| c.to_lowercase())
            .collect();

        if chars.len() < MIN_WORD_LEN {
            return FLOOR_SCORE;
        }

        // Allow stripping at most one trailing sentence punctuation mark if it is not in this alphabet
        // (e.g. "hello," in EN), with a small penalty in case another layout maps that key to a letter.
        let mut stripped_trailing_punct = false;
        if let Some(&last_ch) = chars.last() {
            if is_sentence_punct(last_ch) && self.char_index(last_ch).is_none() && chars.len() > MIN_WORD_LEN {
                chars.pop();
                stripped_trailing_punct = true;
            }
        }

        let n = chars.len();
        if n < MIN_WORD_LEN {
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
        if stripped_trailing_punct {
            total -= 18;
        }
        if vowel_count == 0 {
            // A 4+ letter alphabetic word with zero vowels (e.g. "ghbdtn") is strongly implausible.
            total -= (cap as i32) * 42;
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
    matches!(ch, '.' | ',' | '!' | '?' | ':' | ';')
}

/// Returns `true` if the token is immune from detection (URLs, paths, CLI flags, numbers, snake_case).
pub fn is_immune_word(rendered: &str) -> bool {
    if layout::is_immune_token(rendered) {
        return true;
    }
    rendered
        .chars()
        .any(|c| c.is_ascii_digit() || c == '_' || c == '@' || c == '/' || c == '\\')
}

/// Get the static `LanguageProfile` for a Win32 primary language ID (`LANG_ENGLISH`, `LANG_RUSSIAN`).
pub fn profile_for_primary_lang(primary_lang: u16) -> Option<&'static LanguageProfile> {
    static EN_PROFILE: OnceLock<LanguageProfile> = OnceLock::new();
    static RU_PROFILE: OnceLock<LanguageProfile> = OnceLock::new();

    match primary_lang {
        LANG_ENGLISH => Some(EN_PROFILE.get_or_init(|| {
            LanguageProfile::from_bytes(EN_BYTES).expect("valid embedded en.bin")
        })),
        LANG_RUSSIAN => Some(RU_PROFILE.get_or_init(|| {
            LanguageProfile::from_bytes(RU_BYTES).expect("valid embedded ru.bin")
        })),
        _ => None,
    }
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

    let margin_vs_current = best_score - current_score;
    let margin_vs_runner_up = best_score - runner_up_score;
    let margin = margin_vs_current.min(margin_vs_runner_up);

    if current_score <= CURRENT_MAX_PLAUSIBLE_SCORE
        && best_score >= ABSOLUTE_THRESHOLD
        && margin >= MARGIN_THRESHOLD
    {
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
    if word_keys.len() < MIN_WORD_LEN || active_hkls.len() < 2 {
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
