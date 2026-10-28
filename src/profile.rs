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
        if trimmed.is_empty() {
            return FLOOR_SCORE;
        }

        // Count mid-word lowercase -> uppercase transitions for bicameral scripts
        // (catches Georgian/Arabic unicameral Shift-letters like "dRes" / "lsHgm" typed on EN layout).
        let is_bicameral = matches!(self.script, 1 | 2 | 3 | 7);
        let mut mid_word_caps_transitions = 0usize;
        if is_bicameral {
            let mut prev_lower = false;
            for ch in trimmed.chars() {
                if ch.is_uppercase() {
                    if prev_lower {
                        mid_word_caps_transitions += 1;
                    }
                    prev_lower = false;
                } else if ch.is_lowercase() {
                    prev_lower = true;
                } else {
                    prev_lower = false;
                }
            }
        }

        // Turkish-aware case folding ('İ' -> 'i', 'I' -> 'ı', strip '\u{0307}').
        let mut chars: Vec<char> = Vec::with_capacity(trimmed.len());
        if self.primary_lang == LANG_TURKISH {
            for ch in trimmed.chars() {
                match ch {
                    'İ' => chars.push('i'),
                    'I' => chars.push('ı'),
                    '\u{0307}' => {}
                    _ => {
                        for lc in ch.to_lowercase() {
                            if lc != '\u{0307}' {
                                chars.push(lc);
                            }
                        }
                    }
                }
            }
        } else {
            for ch in trimmed.chars() {
                for lc in ch.to_lowercase() {
                    if lc != '\u{0307}' {
                        chars.push(lc);
                    }
                }
            }
        }

        if chars.is_empty() {
            return FLOOR_SCORE;
        }

        // Only strip leading opening quotes/brackets (`"`, `'`, `«`, `(`, `[`) at word start
        // so words starting with `,`, `.`, `;`, `/` (e.g. Hebrew `,a,`, Turkish `.ok`, Russian `;bpym`)
        // retain their OOV penalty in the wrong layout.
        let mut stripped_punct_count = 0usize;
        while let Some(&first_ch) = chars.first() {
            if is_leading_quote_or_bracket(first_ch)
                && self.char_index(first_ch).is_none()
                && chars.len() > 1
            {
                chars.remove(0);
                stripped_punct_count += 1;
            } else {
                break;
            }
        }
        // Strip trailing sentence punctuation marks that are not letters in this alphabet
        // (e.g. "hello,", "привет?.", "все,"), with a small penalty in case another layout maps that key to a letter.
        while let Some(&last_ch) = chars.last() {
            if is_sentence_punct(last_ch) && self.char_index(last_ch).is_none() && chars.len() > 1 {
                chars.pop();
                stripped_punct_count += 1;
            } else {
                break;
            }
        }

        // Greek word-final sigma normalization ('σ' -> 'ς' at end of word).
        if self.primary_lang == LANG_GREEK {
            if let Some(last) = chars.last_mut() {
                if *last == 'σ' {
                    *last = 'ς';
                }
            }
        }

        // Strip vocalization / combining diacritics (Arabic tashkeel U+064B..U+0655, Hebrew nikud U+0591..U+05C7)
        // ONLY when scoring Arabic or Hebrew so vocalized words score cleanly while wrong-layout combining marks stay OOV.
        if self.primary_lang == LANG_ARABIC {
            chars.retain(|&c| !(('\u{064B}'..='\u{0655}').contains(&c) || c == '\u{0670}' || c == '\u{0640}'));
        } else if self.primary_lang == LANG_HEBREW {
            chars.retain(|&c| !('\u{0591}'..='\u{05C7}').contains(&c));
        }

        // Allow at most ONE orthographic internal apostrophe (`'`, `’`, `ʼ`) or hyphen (`-`) strictly between letters,
        // and never strip backtick `` ` `` (which is a dead key / letter key on non-US layouts).
        if self.char_index('\'').is_none() && chars.len() >= 3 {
            strip_valid_internal_apostrophe_or_hyphen(&mut chars, self.primary_lang);
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
        if mid_word_caps_transitions > 0 {
            total -= (mid_word_caps_transitions as i32) * 210;
        }
        if vowel_count == 0 {
            // Hebrew (0x0D) and Arabic (0x01) are abjads where short vowels are omitted in standard orthography.
            // Czech (0x05) has genuine vowel-less words with syllabic 'r' or 'l' (čtvrt, smrt, prst, vlk, krk, trh).
            let is_abjad = matches!(self.primary_lang, LANG_HEBREW | LANG_ARABIC);
            let is_czech_syllabic = self.primary_lang == LANG_CZECH
                && chars[..cap].iter().any(|&c| c == 'r' || c == 'l')
                && cap >= 3
                && self.lookup_trigram(0, indices[1], indices[2]) > 0;
            if !is_abjad && !is_czech_syllabic {
                if cap >= 3 {
                    // A 3+ letter alphabetic word with zero vowels (e.g. "ghbdtn", "rfr", "xnj") is strongly implausible.
                    total -= (cap as i32) * 42;
                } else if cap == 2 {
                    total -= 55;
                } else if cap == 1 && self.lookup_trigram(0, indices[1], 0) < 0 {
                    // Single consonant that is not a standalone 1-letter word in this language (e.g. "b", "d", "z" in EN).
                    total -= 65;
                }
            } else if is_abjad && cap >= 7 {
                // Even in unvocalized Hebrew/Arabic, 7+ letters without any mater lectionis or vowel carrier is rare.
                total -= ((cap - 6) as i32) * 25;
            }
        }
        let consonant_run_limit = if matches!(
            self.primary_lang,
            LANG_GEORGIAN | LANG_CZECH | LANG_POLISH | LANG_GERMAN
        ) {
            7
        } else {
            5
        };
        if max_consonant_run >= consonant_run_limit
            && !matches!(self.primary_lang, LANG_HEBREW | LANG_ARABIC)
        {
            total -= ((max_consonant_run + 1 - consonant_run_limit) as i32) * 55;
        }
        // French, Dutch, and German frequently have 3-vowel sequences (-eau, -ieux, nieuw, mooie, Feuer, teuer).
        let vowel_run_limit = if matches!(self.primary_lang, LANG_FRENCH | LANG_DUTCH | LANG_GERMAN)
        {
            4
        } else {
            3
        };
        if max_vowel_run >= vowel_run_limit {
            total -= ((max_vowel_run + 1 - vowel_run_limit) as i32) * 55;
        }
        if max_repeat_run >= 3 {
            total -= 90;
        }

        let steps = (cap + 1) as i32;
        (total / steps).clamp(FLOOR_SCORE, 120)
    }
}

#[inline]
fn is_leading_quote_or_bracket(ch: char) -> bool {
    matches!(ch, '"' | '\'' | '«' | '(' | '[')
}

/// Strip at most one valid orthographic internal apostrophe (`'`, `’`, `ʼ`) or hyphen (`-`)
/// without masking wrong-layout dead keys (`"qu'e"`, `"est'a"`, `"tamb'em"`, `"d`eja"`).
fn strip_valid_internal_apostrophe_or_hyphen(chars: &mut Vec<char>, primary_lang: u16) {
    let len = chars.len();
    if len < 3 {
        return;
    }
    // 1. Check for a single internal hyphen strictly between alphabetic characters on French AZERTY
    // (where '-' is on top-row '6' = 0x07, e.g. "peut-être" typed as "peut6[tre" on US).
    // Do NOT strip '-' in English/German/etc., where '-' (0x0C / 0x35) is 'ß', 'ğ', 'ş', '+' on partner layouts!
    if primary_lang == LANG_FRENCH {
        let hyphen_count = chars.iter().filter(|&&c| c == '-').count();
        if hyphen_count == 1 {
            if let Some(pos) = chars.iter().position(|&c| c == '-') {
                if pos >= 1
                    && pos + 1 < chars.len()
                    && chars[pos - 1].is_alphabetic()
                    && chars[pos + 1].is_alphabetic()
                {
                    chars.remove(pos);
                }
            }
        }
    }

    // 2. Check for a single internal apostrophe in languages that use orthographic internal apostrophes:
    if !matches!(
        primary_lang,
        LANG_ENGLISH
            | LANG_FRENCH
            | LANG_ITALIAN
            | LANG_UKRAINIAN
            | LANG_BELARUSIAN
            | LANG_DUTCH
            | LANG_TURKISH
            | LANG_HEBREW
    ) {
        return;
    }

    let apo_count = chars
        .iter()
        .filter(|&&c| matches!(c, '\'' | '’' | 'ʼ'))
        .count();
    if apo_count != 1 {
        return;
    }
    let Some(pos) = chars.iter().position(|&c| matches!(c, '\'' | '’' | 'ʼ')) else {
        return;
    };
    if pos == 0 || pos + 1 >= chars.len() {
        return;
    }
    if !chars[pos - 1].is_alphabetic() || !chars[pos + 1].is_alphabetic() {
        return;
    }

    if primary_lang == LANG_ENGLISH {
        // In English, internal `'` only occurs before contraction/possessive suffixes (`t`, `s`, `d`, `m`, `ve`, `re`, `ll`, `clock`),
        // NEVER before a single vowel as in Spanish/Portuguese/Italian dead-key sequences (`"qu'e"`, `"est'a"`, `"tamb'em"`, `"n'ao"`).
        let suffix: String = chars[pos + 1..].iter().collect();
        if matches!(
            suffix.as_str(),
            "t" | "s" | "d" | "m" | "ve" | "re" | "ll" | "clock"
        ) {
            chars.remove(pos);
        }
    } else {
        chars.remove(pos);
    }
}

#[inline]
fn is_sentence_punct(ch: char) -> bool {
    matches!(
        ch,
        '.' | ',' | '!' | '?' | ':' | ';' | '"' | '\'' | '(' | ')' | '«' | '»' | '؟' | '،' | '؛' | '՞' | '։'
    )
}

/// Returns `true` if the token is immune from detection (URLs, paths, CLI flags, numbers, snake_case).
pub fn is_immune_word(rendered: &str) -> bool {
    let trimmed = rendered.trim();
    if layout::is_immune_token(trimmed) {
        return true;
    }
    // Strip trailing punctuation before checking for path/code characters so a trailing '/'
    // (which is '.' on the Russian layout!) or trailing '\' (which is 'ù' on Italian layout!)
    // does not falsely mark a sentence-ending word as a file path.
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

/// Check whether `rendered` is a clean alphabetic word (allowing trailing sentence punctuation
/// and at most one internal apostrophe or hyphen strictly between letters, e.g. `"c'est"`, `"п'ять"`, `"peut-être"`).
fn is_pure_alphabetic_candidate(rendered: &str) -> bool {
    let trimmed = rendered.trim();
    let core = trimmed.trim_end_matches(is_sentence_punct);
    let core_chars: Vec<char> = core.chars().collect();
    if core_chars.is_empty() {
        return false;
    }
    let mut alpha_count = 0usize;
    let mut sep_count = 0usize;
    for (i, &ch) in core_chars.iter().enumerate() {
        if ch.is_alphabetic() || ('\u{064B}'..='\u{0655}').contains(&ch) || c_is_hebrew_nikud(ch) {
            if ch.is_alphabetic() {
                alpha_count += 1;
            }
        } else if matches!(ch, '\'' | '’' | 'ʼ' | '-') {
            sep_count += 1;
            if sep_count > 1 || i == 0 || i + 1 >= core_chars.len() {
                return false;
            }
            if !core_chars[i - 1].is_alphabetic() || !core_chars[i + 1].is_alphabetic() {
                return false;
            }
        } else {
            return false;
        }
    }
    alpha_count >= MIN_WORD_LEN
}

#[inline]
fn c_is_hebrew_nikud(ch: char) -> bool {
    ('\u{0591}'..='\u{05C7}').contains(&ch)
}

/// Evaluate a slice of keystroke slots against pre-built `LayoutLayerTable`s.
pub fn evaluate_tables(
    word_keys: &[KeyStrokeSlot],
    current_hkl: HKL,
    tables: &[&LayoutLayerTable],
) -> Option<DetectionVerdict> {
    // Allow 2-keystroke words into evaluation so 2-key / 3-char Arabic ligature words (`بلا`, `لأن`, `إلا`, `ولا`)
    // can reach `target_alpha_len >= MIN_WORD_LEN (3)`.
    if word_keys.len() < 2 || tables.len() < 2 {
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

    let (mut current_rendered, current_trailing_dead) =
        current_table.render_keys_raw(word_keys, false);
    let current_profile = profile_for_primary_lang(current_table.primary_lang)?;
    if let Some(d) = current_trailing_dead {
        // If the last keystroke is a dead key in `current_table` (e.g. `0x1A` `[` on FR/ES when typing Russian `"этих"`,
        // or `0x28` `'` on ES/PT/NL when typing Ukrainian `"працює"`), only evaluate if the stem before the dead key
        // is already implausible in `current_table` (so we never interrupt genuine dead-key typing like Spanish `"caf´"`).
        if !current_rendered.is_empty() && current_profile.score_word(&current_rendered) > -15 {
            return None;
        }
        current_rendered.push(d);
    }
    if current_rendered.is_empty() {
        return None;
    }

    if layout::is_unconditionally_immune_token(&current_rendered) {
        return None;
    }
    if is_immune_word(&current_rendered) {
        // On French AZERTY (é=2, '=4, -=6, è=7, _=8, ç=9, à=0), Czech (ě=2..é=0), Kazakh (ә=2..һ=0),
        // Armenian (1..=, /=պ, \=շ), Ukrainian (\=ґ), Arabic 102 (\=ذ, Shift+A=\), Polish 214 (\=ó/ź),
        // Italian (\=ù), Georgian MES (/=ძ), and Hebrew (/=q), top-row or slash/backslash keys produce
        // digits or slashes in US QWERTY. Allow scoring ONLY if at least one candidate layout renders
        // these exact keystrokes into a valid word!
        let has_pure_alpha_candidate = tables.iter().any(|&t| {
            t.hkl != current_table.hkl
                && t.render_keys_for_target(word_keys)
                    .map(|r| is_pure_alphabetic_candidate(&r))
                    .unwrap_or(false)
        });
        if !has_pure_alpha_candidate {
            return None;
        }
    }

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
        // Never allow a candidate layout to win by dropping mid-word keystrokes (e.g. camelCase "GitHub" on KA-QWERTY where Shift+H is None).
        let drops_mid_word_key = word_keys.iter().skip(1).any(|k| {
            k.vk != 0x20
                && table.lookup_for_target(k) == layout::ScanTranslation::None
                && current_table.lookup(k) != layout::ScanTranslation::None
        });
        if drops_mid_word_key {
            continue;
        }
        let Some(rendered) = table.render_keys_for_target(word_keys) else {
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
                altgr: false,
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
            ("bylbrfnjh", "индикатор"),
            ("chf,fnsdftn", "срабатывает"),
            ("chf,fnSdftn", "срабатЫвает"),
            ("vuyjdtyyj?", "мгновенно,"),
            ("tuj", "его"),
            ("bppf", "изза"),
            ("lf;t", "даже"),
            ("dblyj", "видно"),
            ("hfdyj", "равно"),
            ("t,fytymrj", "ебаненько"),
            ("t,fytymRj", "ебаненьКо"),
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

    #[test]
    fn twenty_layout_skeptic_suite_fr_cs_kk_hy_tr_he_ka() {
        const FR_HKL: HKL = HKL(0x040C040C as _);
        const CS_HKL: HKL = HKL(0x04050405 as _);
        const KK_HKL: HKL = HKL(0x043F043F as _);
        const HY_HKL: HKL = HKL(0x042B042B as _);
        const HE_HKL: HKL = HKL(0x040D040D as _);
        const KA_HKL: HKL = HKL(0x04370437 as _);

        let mut us_with_num = US_LAYOUT.to_vec();
        us_with_num.extend_from_slice(&[
            (0x03, '2', '@'),
            (0x04, '3', '#'),
            (0x05, '4', '$'),
            (0x06, '5', '%'),
            (0x07, '6', '^'),
            (0x09, '8', '*'),
            (0x0A, '9', '('),
            (0x0B, '0', ')'),
        ]);
        let us = LayoutLayerTable::from_pairs(US_HKL, &us_with_num);

        // 1. French AZERTY top-row accented letters ('é' on '2'=0x03, 'è' on '7'=0x08, 'à' on '0'=0x0B):
        // Typing "déjà" on US layout produces "d2j0" (contains 2 digits!) -> must NOT be blocked by digit immunity!
        let mut fr_pairs = us_with_num.clone();
        for e in &mut fr_pairs {
            match e.0 {
                0x03 => *e = (0x03, 'é', '2'),
                0x08 => *e = (0x08, 'è', '7'),
                0x0B => *e = (0x0B, 'à', '0'),
                _ => {}
            }
        }
        let fr = LayoutLayerTable::from_pairs(FR_HKL, &fr_pairs);
        let deja_keys = type_on_layout("déjà", &fr_pairs);
        assert_eq!(us.render_keys(&deja_keys).as_deref(), Some("d2j0"));
        let v_fr = evaluate_tables(&deja_keys, US_HKL, &[&us, &fr])
            .expect("FR 'déjà' ('d2j0' in US) must be detected");
        assert_eq!(v_fr.target_hkl, FR_HKL);
        assert_eq!(v_fr.rendered_target, "déjà");

        // 2. Czech QWERTZ top-row caron letters ('š' on '3'=0x04, 'č' on '4'=0x05):
        // Typing "čeština" on US layout produces "4e3tina" -> must be detected as CS!
        let mut cs_pairs = us_with_num.clone();
        for e in &mut cs_pairs {
            match e.0 {
                0x04 => *e = (0x04, 'š', '3'),
                0x05 => *e = (0x05, 'č', '4'),
                _ => {}
            }
        }
        let cs = LayoutLayerTable::from_pairs(CS_HKL, &cs_pairs);
        let cestina_keys = type_on_layout("čeština", &cs_pairs);
        assert_eq!(us.render_keys(&cestina_keys).as_deref(), Some("4e3tina"));
        let v_cs = evaluate_tables(&cestina_keys, US_HKL, &[&us, &cs])
            .expect("CS 'čeština' ('4e3tina' in US) must be detected");
        assert_eq!(v_cs.target_hkl, CS_HKL);
        assert_eq!(v_cs.rendered_target, "čeština");

        // 3. Kazakh Cyrillic top-row letters ('ә' on '2'=0x03, 'қ' on '8'=0x09):
        let mut kk_pairs = RU_LAYOUT.to_vec();
        kk_pairs.extend_from_slice(&[(0x03, 'ә', 'Ә'), (0x09, 'қ', 'Қ')]);
        let kk = LayoutLayerTable::from_pairs(KK_HKL, &kk_pairs);
        let salem_keys = type_on_layout("сәлем", &kk_pairs);
        assert_eq!(us.render_keys(&salem_keys).as_deref(), Some("c2ktv"));
        let v_kk = evaluate_tables(&salem_keys, US_HKL, &[&us, &kk])
            .expect("KK 'сәлем' ('c2ktv' in US) must be detected");
        assert_eq!(v_kk.target_hkl, KK_HKL);
        assert_eq!(v_kk.rendered_target, "сәлем");

        // 4. Armenian top-row letters ('ր' on '8'=0x09):
        let hy_pairs: &[(u8, char, char)] = &[
            (0x14, 'տ', 'Տ'),
            (0x1E, 'ա', 'Ա'),
            (0x09, 'ր', 'Ր'),
            (0x17, 'ի', 'Ի'),
        ];
        let hy = LayoutLayerTable::from_pairs(HY_HKL, hy_pairs);
        let tari_keys = type_on_layout("տարի", hy_pairs);
        assert_eq!(us.render_keys(&tari_keys).as_deref(), Some("ta8i"));
        let v_hy = evaluate_tables(&tari_keys, US_HKL, &[&us, &hy])
            .expect("HY 'տարի' ('ta8i' in US) must be detected");
        assert_eq!(v_hy.target_hkl, HY_HKL);
        assert_eq!(v_hy.rendered_target, "տարի");

        // 5. Turkish capital 'İ' ('İstanbul') must not suffer combining-dot OOV penalty:
        let tr_prof = profile_for_primary_lang(LANG_TURKISH).unwrap();
        let score_upper = tr_prof.score_word("İstanbul");
        let score_lower = tr_prof.score_word("istanbul");
        assert_eq!(
            score_upper, score_lower,
            "Turkish 'İstanbul' must fold 'İ' cleanly to 'i' without U+0307 OOV penalty"
        );

        // 6. Hebrew ('q' is '/' on 0x10) and Hebrew Vav/Yod vowels:
        let he_pairs: &[(u8, char, char)] = &[
            (0x10, '/', 'Q'),
            (0x11, '\'', 'W'),
            (0x12, 'ק', 'E'),
            (0x13, 'ר', 'R'),
            (0x14, 'א', 'T'),
            (0x15, 'ט', 'Y'),
            (0x16, 'ו', 'U'),
            (0x17, 'ן', 'I'),
            (0x18, 'ם', 'O'),
            (0x19, 'פ', 'P'),
            (0x1E, 'ש', 'A'),
            (0x1F, 'ד', 'S'),
            (0x20, 'ג', 'D'),
            (0x21, 'כ', 'F'),
            (0x22, 'ע', 'G'),
            (0x23, 'י', 'H'),
            (0x24, 'ח', 'J'),
            (0x25, 'ל', 'K'),
            (0x26, 'ך', 'L'),
            (0x2C, 'ז', 'Z'),
            (0x2D, 'ס', 'X'),
            (0x2E, 'ב', 'C'),
            (0x2F, 'ה', 'V'),
            (0x30, 'נ', 'B'),
            (0x31, 'מ', 'N'),
            (0x32, 'צ', 'M'),
            (0x33, 'ת', '<'),
        ];
        let he = LayoutLayerTable::from_pairs(HE_HKL, he_pairs);
        let q_keys = type_on_layout("question", &us_with_num);
        assert_eq!(he.render_keys(&q_keys).as_deref(), Some("/וקדאןםמ"));
        let v_he_to_en = evaluate_tables(&q_keys, HE_HKL, &[&he, &us])
            .expect("EN 'question' ('/וקדאןםמ' on HE) must not be blocked by '/' path immunity");
        assert_eq!(v_he_to_en.target_hkl, US_HKL);
        assert_eq!(v_he_to_en.rendered_target, "question");

        // 7. Georgian unicameral Shift-letters ('ღ' on Shift+R = 0x13):
        let ka_pairs: &[(u8, char, char)] = &[
            (0x20, 'დ', 'D'),
            (0x13, 'რ', 'ღ'),
            (0x12, 'ე', 'E'),
            (0x1F, 'ს', 'შ'),
        ];
        let ka = LayoutLayerTable::from_pairs(KA_HKL, ka_pairs);
        let dges_keys = type_on_layout("დღეს", ka_pairs);
        assert_eq!(us.render_keys(&dges_keys).as_deref(), Some("dRes"));
        let v_ka = evaluate_tables(&dges_keys, US_HKL, &[&us, &ka])
            .expect("KA 'დღეს' ('dRes' in US) must be detected");
        assert_eq!(v_ka.target_hkl, KA_HKL);
        assert_eq!(v_ka.rendered_target, "დღეს");
    }

    #[test]
    fn m13_twenty_language_and_45_kbd_variant_regression_suite() {
        const US_HKL: HKL = HKL(0x04090409 as _);
        const RU_HKL: HKL = HKL(0x04190419 as _);
        const UK_HKL: HKL = HKL(0x04220422 as _);
        const FR_HKL: HKL = HKL(0x040C040C as _);
        const ES_HKL: HKL = HKL(0x040A040A as _);
        const CS_HKL: HKL = HKL(0x04050405 as _);
        const AR_HKL: HKL = HKL(0x04010401 as _);
        const HE_HKL: HKL = HKL(0x040D040D as _);
        const KA_HKL: HKL = HKL(0x04370437 as _);

        let mut us_full = US_LAYOUT.to_vec();
        us_full.extend_from_slice(&[
            (0x03, '2', '@'),
            (0x04, '3', '#'),
            (0x05, '4', '$'),
            (0x06, '5', '%'),
            (0x07, '6', '^'),
            (0x08, '7', '&'),
            (0x09, '8', '*'),
            (0x0B, '0', ')'),
            (0x2B, '\\', '|'),
        ]);
        let us = LayoutLayerTable::from_pairs(US_HKL, &us_full);
        let ru = LayoutLayerTable::from_pairs(RU_HKL, RU_LAYOUT);

        // 1. Arabic (kbd101.dll & kbd102.dll):
        //    - 2-keystroke / 3-letter ligature word "بلا" ('f'=0x21 -> 'ب', 'b'=0x30 -> 'لا')
        //    - Mid-word Shift hamza "مسألة" ('H'=Shift+0x23 -> 'أ')
        //    - AR-102 'ذ' on '\' (0x2B) -> "هذا" ('i'=0x17='ه', '\'=0x2B='ذ', 'h'=0x23='ا')
        let ar_pairs: &[(u8, char, char)] = &[
            (0x17, 'ه', '÷'),
            (0x2B, 'ذ', 'ّ'),
            (0x23, 'ا', 'أ'),
            (0x21, 'ب', '['),
            (0x26, 'م', '/'),
            (0x1F, 'س', 'ٍ'),
            (0x22, 'ل', 'ل'),
            (0x32, 'ة', '\''),
            (0x1E, 'ش', '\\'), // AR-102 Shift+A = '\'
            (0x24, 'ت', 'ـ'),
            (0x25, 'ن', '،'),
        ];
        let ar = LayoutLayerTable::from_pairs(AR_HKL, ar_pairs)
            .with_ligature(0x30, false, 'ل', 'ا')
            .with_ligature(0x22, true, 'ل', 'أ');

        let bila_keys = [
            KeyStrokeSlot {
                vk: 0,
                scan_code: 0x21,
                flags: 0,
                ms_tick: 100,
                shift: false,
                altgr: false,
            },
            KeyStrokeSlot {
                vk: 0,
                scan_code: 0x30,
                flags: 0,
                ms_tick: 120,
                shift: false,
                altgr: false,
            },
        ];
        assert_eq!(us.render_keys(&bila_keys).as_deref(), Some("fb"));
        assert_eq!(ar.render_keys(&bila_keys).as_deref(), Some("بلا"));
        let v_bila = evaluate_tables(&bila_keys, US_HKL, &[&us, &ar])
            .expect("2-keystroke 3-letter Arabic ligature word 'بلا' ('fb' on US) must be detected");
        assert_eq!(v_bila.target_hkl, AR_HKL);
        assert_eq!(v_bila.rendered_target, "بلا");

        let masala_keys = type_on_layout("مسألة", ar_pairs);
        assert_eq!(us.render_keys(&masala_keys).as_deref(), Some("lsHgm"));
        let v_masala = evaluate_tables(&masala_keys, US_HKL, &[&us, &ar])
            .expect("Arabic 'مسألة' with mid-word Shift hamza ('lsHgm' on US) must be detected");
        assert_eq!(v_masala.target_hkl, AR_HKL);
        assert_eq!(v_masala.rendered_target, "مسألة");

        let hadha_keys = type_on_layout("هذا", ar_pairs);
        assert_eq!(us.render_keys(&hadha_keys).as_deref(), Some(r"i\h"));
        let v_hadha = evaluate_tables(&hadha_keys, US_HKL, &[&us, &ar])
            .expect(r"AR-102 'هذا' ('i\h' on US with 'ذ' on '\') must be detected");
        assert_eq!(v_hadha.target_hkl, AR_HKL);
        assert_eq!(v_hadha.rendered_target, "هذا");

        // 2. Hebrew (kbdheb.dll): Capitalized "Hello" typed on HE produces "Hקךךם" (Shift+H='H'):
        let he_pairs: &[(u8, char, char)] = &[
            (0x23, 'י', 'H'),
            (0x12, 'ק', 'E'),
            (0x25, 'ל', 'K'),
            (0x26, 'ך', 'L'),
            (0x18, 'ם', 'O'),
            (0x1E, 'ש', 'A'),
            (0x16, 'ו', 'U'),
        ];
        let he = LayoutLayerTable::from_pairs(HE_HKL, he_pairs);
        let hello_keys = type_on_layout("Hello", &us_full);
        assert_eq!(he.render_keys(&hello_keys).as_deref(), Some("Hקךךם"));
        let v_he_hello = evaluate_tables(&hello_keys, HE_HKL, &[&he, &us])
            .expect("Capitalized 'Hello' typed on HE ('Hקךךם') must be detected as EN");
        assert_eq!(v_he_hello.target_hkl, US_HKL);
        assert_eq!(v_he_hello.rendered_target, "Hello");

        // 3. Georgian (kbdgeoqw.dll & kbdgeome.dll):
        //    - Capitalized "Hello" typed on KA-QWERTY drops Shift+H ('ჰ' has shift=None -> "ელლო")
        //    - Capitalized "Gamarjoba" typed on US normalizes Shift+G -> "გამარჯობა"
        //    - CamelCase "GitHub" typed on US stays immune (KA-QWERTY drops mid-word Shift+H)
        //    - "გთხოვთ" (5 consecutive consonants + Shift+T='თ') detected as KA
        let ka_qw = LayoutLayerTable::from_pairs(
            KA_HKL,
            &[
                (0x23, 'ჰ', '\0'),
                (0x12, 'ე', '\0'),
                (0x26, 'ლ', '\0'),
                (0x18, 'ო', '\0'),
                (0x22, 'გ', '\0'),
                (0x1E, 'ა', '\0'),
                (0x32, 'მ', '\0'),
                (0x13, 'რ', 'ღ'),
                (0x24, 'ჯ', 'ჟ'),
                (0x30, 'ბ', '\0'),
                (0x14, 'ტ', 'თ'),
                (0x2D, 'ხ', '\0'),
                (0x2F, 'ვ', '\0'),
                (0x17, 'ი', '\0'),
                (0x16, 'უ', '\0'),
            ],
        );
        assert_eq!(
            ka_qw.render_keys_raw(&hello_keys, false),
            ("ელლო".to_string(), None)
        );
        let v_ka_hello = evaluate_tables(&hello_keys, KA_HKL, &[&ka_qw, &us])
            .expect("Capitalized 'Hello' typed on KA-QWERTY ('ელლო') must be detected as EN");
        assert_eq!(v_ka_hello.target_hkl, US_HKL);
        assert_eq!(v_ka_hello.rendered_target, "Hello");

        let gamarjoba_keys = type_on_layout("Gamarjoba", &us_full);
        let v_gamarjoba = evaluate_tables(&gamarjoba_keys, US_HKL, &[&us, &ka_qw])
            .expect("Capitalized 'Gamarjoba' typed on US must normalize Shift+G and detect KA");
        assert_eq!(v_gamarjoba.target_hkl, KA_HKL);
        assert_eq!(v_gamarjoba.rendered_target, "გამარჯობა");

        let github_keys = type_on_layout("GitHub", &us_full);
        assert!(
            evaluate_tables(&github_keys, US_HKL, &[&us, &ka_qw]).is_none(),
            "CamelCase 'GitHub' on US must not false-positive to KA-QWERTY when mid-word Shift+H is dropped"
        );

        let gtkhovt_keys = type_on_layout("gTxovT", &us_full);
        let v_gtkhovt = evaluate_tables(&gtkhovt_keys, US_HKL, &[&us, &ka_qw])
            .expect("Georgian 'გთხოვთ' ('gTxovT' on US) with 5 consonants must be detected");
        assert_eq!(v_gtkhovt.target_hkl, KA_HKL);
        assert_eq!(v_gtkhovt.rendered_target, "გთხოვთ");

        // 4. Russian word ending on a dead key when active layout is French AZERTY ("этих" -> 'х' is 0x1A = dead '^'):
        let fr_azerty = LayoutLayerTable::from_pairs(
            FR_HKL,
            &[
                (0x05, '\'', '4'),
                (0x12, 'e', 'E'),
                (0x14, 't', 'T'),
                (0x1F, 's', 'S'),
                (0x2E, 'c', 'C'),
                (0x28, 'ù', '%'),
                (0x30, 'b', 'B'),
                (0x31, 'n', 'N'),
            ],
        )
        .with_dead_key(0x1A, false, '^');

        let etikh_keys = type_on_layout("этих", RU_LAYOUT);
        assert_eq!(fr_azerty.render_keys(&etikh_keys), None); // dead key '^' unresolved at end!
        assert_eq!(
            fr_azerty.render_keys_raw(&etikh_keys, false),
            ("ùnb".to_string(), Some('^'))
        );
        let v_etikh = evaluate_tables(&etikh_keys, FR_HKL, &[&fr_azerty, &ru])
            .expect("RU 'этих' typed on FR AZERTY (ending on dead key 0x1A '^') must be detected");
        assert_eq!(v_etikh.target_hkl, RU_HKL);
        assert_eq!(v_etikh.rendered_target, "этих");

        // 5. French contraction "c'est" ('=4 on 0x05) typed on US ("c4est"):
        let cest_keys = type_on_layout("c'est", &[
            (0x2E, 'c', 'C'),
            (0x05, '\'', '4'),
            (0x12, 'e', 'E'),
            (0x1F, 's', 'S'),
            (0x14, 't', 'T'),
        ]);
        assert_eq!(us.render_keys(&cest_keys).as_deref(), Some("c4est"));
        let v_cest = evaluate_tables(&cest_keys, US_HKL, &[&us, &fr_azerty])
            .expect("FR \"c'est\" ('c4est' on US) must be detected");
        assert_eq!(v_cest.target_hkl, FR_HKL);
        assert_eq!(v_cest.rendered_target, "c'est");

        // 6. Spanish dead-key word "está" typed on US ("est'a"):
        let es = LayoutLayerTable::from_pairs(ES_HKL, &us_full).with_dead_key(0x28, false, '´');
        let esta_keys = type_on_layout("est'a", &us_full);
        assert_eq!(es.render_keys(&esta_keys).as_deref(), Some("está"));
        let v_esta = evaluate_tables(&esta_keys, US_HKL, &[&us, &es])
            .expect("ES 'está' (\"est'a\" on US) must be detected as ES and not treated as an EN contraction");
        assert_eq!(v_esta.target_hkl, ES_HKL);
        assert_eq!(v_esta.rendered_target, "está");

        // 7. Ukrainian internal apostrophe ("п'ять") and Czech syllabic-liquid ("čtvrt"):
        let mut uk_pairs = RU_LAYOUT.to_vec();
        for e in &mut uk_pairs {
            if e.0 == 0x28 {
                *e = (0x28, 'є', 'Є');
            }
        }
        let uk = LayoutLayerTable::from_pairs(UK_HKL, &uk_pairs);
        let uk_prof = profile_for_primary_lang(LANG_UKRAINIAN).unwrap();
        assert!(
            uk_prof.score_word("п'ять") > 25,
            "Ukrainian \"п'ять\" must score strongly positive"
        );
        let працю_keys = type_on_layout("працює", &uk_pairs);
        let v_uk_dead = evaluate_tables(&працю_keys, ES_HKL, &[&es, &uk])
            .expect("UK 'працює' ending on ES dead key 0x28 ('´') must be detected");
        assert_eq!(v_uk_dead.target_hkl, UK_HKL);

        let cs = LayoutLayerTable::from_pairs(
            CS_HKL,
            &[
                (0x05, 'č', '4'),
                (0x14, 't', 'T'),
                (0x2F, 'v', 'V'),
                (0x13, 'r', 'R'),
            ],
        );
        let ctvrt_keys = type_on_layout("čtvrt", &[
            (0x05, 'č', '4'),
            (0x14, 't', 'T'),
            (0x2F, 'v', 'V'),
            (0x13, 'r', 'R'),
        ]);
        assert_eq!(us.render_keys(&ctvrt_keys).as_deref(), Some("4tvrt"));
        let v_ctvrt = evaluate_tables(&ctvrt_keys, US_HKL, &[&us, &cs])
            .expect("Czech syllabic-r word 'čtvrt' ('4tvrt' on US) must be detected");
        assert_eq!(v_ctvrt.target_hkl, CS_HKL);
        assert_eq!(v_ctvrt.rendered_target, "čtvrt");
    }

    #[test]
    fn real_45_system_kbd_dll_matrix() {
        use crate::fix::plan_word_fix;

        let us = LayoutLayerTable::from_system_kbd_dll(HKL(0x04090409 as _), "kbdus.dll");
        let ru = LayoutLayerTable::from_system_kbd_dll(HKL(0x04190419 as _), "kbdru.dll");

        // All 45 official Windows 11 KBD*.DLL layout drivers across the 20 languages:
        // (dll_name, hkl_u32, lang_label, sample_word_in_target_lang, sample_multiword_burst)
        let matrix: &[(&str, u32, &str, &str, &str)] = &[
            // 1. English (3 DLLs: tested against RU JCUKEN)
            ("KBDUS.DLL", 0x04090409, "EN-US", "question", "hello my friend"),
            ("KBDUK.DLL", 0x08090809, "EN-UK", "question", "hello my friend"),
            ("KBDUSX.DLL", 0x04090409, "EN-US-Intl", "question", "hello my friend"),
            // 2. Russian (2 DLLs)
            ("KBDRU.DLL", 0x04190419, "RU-JCUKEN", "привет", "привет как дела"),
            ("KBDRU1.DLL", 0x04190419, "RU-Typewriter", "привет", "привет как дела"),
            // 3. Ukrainian (2 DLLs)
            ("KBDUR.DLL", 0x04220422, "UK-101", "питання", "привіт як справи"),
            ("KBDUR1.DLL", 0x04220422, "UK-Enhanced", "ґрунт", "привіт як справи"),
            // 4. Belarusian (1 DLL)
            ("KBDBLR.DLL", 0x04230423, "BE-Belarusian", "прывітанне", "прывітанне сябар"),
            // 5. Kazakh (1 DLL)
            ("KBDKAZ.DLL", 0x043F043F, "KK-Kazakh", "сәлем", "сәлем достар"),
            // 6. German (2 DLLs: Swiss German KBDSG.DLL does not have 'ß', uses 'ss' / umlauts)
            ("KBDGR.DLL", 0x04070407, "DE-QWERTZ", "straße", "für das große"),
            ("KBDSG.DLL", 0x08070807, "DE-Swiss", "zurück", "für das schöne"),
            // 7. French (4 DLLs)
            ("KBDFR.DLL", 0x040C040C, "FR-Legacy", "déjà", "c'est déjà très"),
            ("KBDFRNA.DLL", 0x040C040C, "FR-Std-AZERTY", "déjà", "c'est déjà très"),
            ("KBDBE.DLL", 0x080C080C, "FR-Belgian", "déjà", "c'est déjà très"),
            ("KBDSF.DLL", 0x100C100C, "FR-Swiss", "déjà", "très bien merci"),
            // 8. Spanish (2 DLLs)
            ("KBDSP.DLL", 0x040A040A, "ES-Spain", "también", "está muy bien"),
            ("KBDLA.DLL", 0x080A080A, "ES-LatAm", "también", "está muy bien"),
            // 9. Portuguese (2 DLLs)
            ("KBDPO.DLL", 0x08160816, "PT-Portugal", "não", "não está aqui"),
            ("KBDBR.DLL", 0x04160416, "PT-Brazil-ABNT2", "coração", "não está aqui"),
            // 10. Italian (2 DLLs)
            ("KBDIT.DLL", 0x04100410, "IT-Italy", "perché", "perché è così"),
            ("KBDIT142.DLL", 0x04100410, "IT-142", "perché", "perché è così"),
            // 11. Turkish (2 DLLs)
            ("KBDTUQ.DLL", 0x041F041F, "TR-Q", "günaydın", "çok güzel bir"),
            ("KBDTUF.DLL", 0x041F041F, "TR-F", "merhaba", "çok güzel bir"),
            // 12. Polish (2 DLLs)
            ("KBDPL1.DLL", 0x04150415, "PL-Programmers", "będzie", "wszystko będzie dobrze"),
            ("KBDPL.DLL", 0x04150415, "PL-214-QWERTZ", "będzie", "wszystko będzie dobrze"),
            // 13. Dutch (1 DLL)
            ("KBDNE.DLL", 0x04130413, "NL-Dutch", "financiële", "voor België"),
            // 14. Swedish (1 DLL)
            ("KBDSW.DLL", 0x041D041D, "SV-Swedish", "också", "det är också bra"),
            // 15. Czech (2 DLLs)
            ("KBDCZ.DLL", 0x04050405, "CS-QWERTZ", "čeština", "čtvrt hodiny"),
            ("KBDCZ1.DLL", 0x04050405, "CS-QWERTY", "čeština", "čtvrt hodiny"),
            // 16. Greek (2 DLLs)
            ("KBDHE.DLL", 0x04080408, "EL-Greek", "καλημέρα", "καλημέρα σας"),
            ("KBDHEPT.DLL", 0x04080408, "EL-Polytonic", "καλημέρα", "καλημέρα σας"),
            // 17. Hebrew (3 DLLs)
            ("KBDHEB.DLL", 0x040D040D, "HE-Legacy", "שלום", "שלום לכם היום"),
            ("kbdhebl3.dll", 0x040D040D, "HE-Standard", "שלום", "שלום לכם היום"),
            ("kbdhebsi.dll", 0x040D040D, "HE-SI-2018", "שלום", "שלום לכם היום"),
            // 18. Arabic (3 DLLs)
            ("KBDA1.DLL", 0x04010401, "AR-101", "بلا", "السلام عليكم"),
            ("KBDA2.DLL", 0x04010401, "AR-102", "هذا", "هذا الكتاب"),
            ("KBDA3.DLL", 0x04010401, "AR-102-AZERTY", "مسألة", "السلام عليكم"),
            // 19. Armenian (4 DLLs)
            ("kbdarme.dll", 0x042B042B, "HY-Eastern", "բարև", "բարև ձեզ"),
            ("kbdarmw.dll", 0x042B042B, "HY-Western", "բարև", "բարև ձեզ"),
            ("kbdarmph.dll", 0x042B042B, "HY-Phonetic", "բարև", "բարև ձեզ"),
            ("kbdarmty.dll", 0x042B042B, "HY-Typewriter", "բարև", "բարև ձեզ"),
            // 20. Georgian (4 DLLs)
            ("kbdgeo.dll", 0x04370437, "KA-Legacy", "გამარჯობა", "გამარჯობა მეგობარო"),
            ("kbdgeoqw.dll", 0x04370437, "KA-QWERTY", "გთხოვთ", "გამარჯობა მეგობარო"),
            ("kbdgeoer.dll", 0x04370437, "KA-Ergonomic", "გამარჯობა", "გამარჯობა მეგობარო"),
            ("kbdgeome.dll", 0x04370437, "KA-MES", "ძალიან", "ძალიან კარგი"),
        ];

        println!(
            "\n{:<15} | {:<16} | {:<20} | {:<22} | {:<22}",
            "DLL", "Layout", "Wrong-Layout Output", "Detected Word", "Multi-Word Burst Fix"
        );
        println!("{}", "-".repeat(105));

        for &(dll, hkl_u32, label, word, burst) in matrix {
            let target_table = LayoutLayerTable::from_system_kbd_dll(HKL(hkl_u32 as _), dll);
            let wrong_table = if (hkl_u32 & 0x3FF) == LANG_ENGLISH as u32 {
                &ru
            } else {
                &us
            };

            // 1. Single-word detection test:
            let word_slots = target_table.simulate_typing(word);
            let (wrong_committed, wrong_dead) = wrong_table.render_keys_raw(&word_slots, false);
            let mut wrong_display = wrong_committed.clone();
            if let Some(d) = wrong_dead {
                wrong_display.push(d);
            }

            let eval = evaluate_tables(&word_slots, wrong_table.hkl, &[wrong_table, &target_table])
                .unwrap_or_else(|| {
                    panic!(
                        "[FAIL] {dll} ({label}): typing {word:?} on wrong layout produced {wrong_display:?} and was NOT detected!"
                    )
                });
            assert_eq!(
                eval.target_hkl, target_table.hkl,
                "[FAIL] {dll} ({label}): wrong target HKL"
            );
            assert_eq!(
                eval.rendered_target.to_lowercase(),
                word.to_lowercase(),
                "[FAIL] {dll} ({label}): rendered target mismatch"
            );

            // 2. Multi-word burst fix test (`plan_word_fix`):
            let burst_slots = target_table.simulate_typing(burst);
            let plan = plan_word_fix(&burst_slots, wrong_table.hkl, &[wrong_table, &target_table])
                .unwrap_or_else(|| {
                    panic!("[FAIL] {dll} ({label}): plan_word_fix returned None for burst {burst:?}")
                });
            assert_eq!(
                plan.replacement_text.to_lowercase(),
                burst.to_lowercase(),
                "[FAIL] {dll} ({label}): burst replacement mismatch"
            );

            // 3. False-positive immunity test (typing native word when target_table IS active must stay None):
            assert!(
                evaluate_tables(&word_slots, target_table.hkl, &[&target_table, wrong_table])
                    .is_none(),
                "[FAIL] {dll} ({label}): false positive when typing native word {word:?} on correct layout!"
            );

            println!(
                "{:<15} | {:<16} | {:<20} | {:<22} | {:<22}",
                dll, label, wrong_display, eval.rendered_target, plan.replacement_text
            );
        }
    }
}
