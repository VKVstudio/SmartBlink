//! Offline language profile generator (`tools/profilegen`).
//! Generates `assets/profiles/en.bin` and `assets/profiles/ru.bin` in SMBP v1 format.
//!
//! Binary format (`SMBP` v1, little-endian):
//! - [0..4]   magic: `b"SMBP"`
//! - [4]      version: `u8` = 1
//! - [5]      script: `u8` (1 = Latin, 2 = Cyrillic, ...)
//! - [6..8]   primary_lang: `u16` (`LANG_ENGLISH = 0x09`, `LANG_RUSSIAN = 0x19`)
//! - [8..10]  alpha_count: `u16`
//! - [10..18] vowel_mask: `u64` (bit `i` set if 1-based alphabet index `i` is a vowel)
//! - [18..20] bigram_count: `u16`
//! - [20..22] trigram_count: `u16`
//! - [22]     default_bigram_score: `i8`
//! - [23]     default_trigram_score: `i8`
//! - [24]     oov_char_penalty: `i8`
//! - [25]     reserved: `u8` = 0
//! - alphabet: `[u16; alpha_count]` sorted ascending BMP lowercase code units
//! - bigrams:  `[(u16, i8); bigram_count]` sorted by `u16` key ascending (`(i0 << 8) | i1`)
//! - trigrams: `[(u32, i8); trigram_count]` sorted by `u32` key ascending (`(i0 << 16) | (i1 << 8) | i2`)

mod corpora;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const MAGIC: &[u8; 4] = b"SMBP";
const VERSION: u8 = 1;

pub const SCRIPT_LATIN: u8 = 1;
pub const SCRIPT_CYRILLIC: u8 = 2;
pub const SCRIPT_ARMENIAN: u8 = 3;
pub const SCRIPT_GEORGIAN: u8 = 4;
pub const SCRIPT_HEBREW: u8 = 5;
pub const SCRIPT_ARABIC: u8 = 6;
pub const SCRIPT_GREEK: u8 = 7;

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

struct LangSpec<'a> {
    file_name: &'a str,
    script: u8,
    primary_lang: u16,
    alphabet: &'a str,
    vowels: &'a str,
    corpus: &'a [(&'a str, u32)],
}

fn build_profile_bytes(spec: &LangSpec<'_>) -> Vec<u8> {
    let mut alpha_chars: Vec<u16> = spec.alphabet.chars().map(|c| c as u16).collect();
    alpha_chars.sort_unstable();
    alpha_chars.dedup();
    assert!(alpha_chars.len() <= 62, "alphabet fits in 64-bit vowel_mask");

    let char_to_idx = |c: char| -> Option<u8> {
        let code = c as u16;
        alpha_chars
            .binary_search(&code)
            .ok()
            .map(|pos| (pos + 1) as u8)
    };

    let mut vowel_mask: u64 = 0;
    for vc in spec.vowels.chars() {
        if let Some(idx) = char_to_idx(vc) {
            vowel_mask |= 1u64 << idx;
        }
    }

    let mut bigram_counts: BTreeMap<u16, u64> = BTreeMap::new();
    let mut trigram_counts: BTreeMap<u32, u64> = BTreeMap::new();
    let mut total_bigrams: u64 = 0;
    let mut total_trigrams: u64 = 0;

    for &(phrase, weight) in spec.corpus {
        for raw_word in phrase.split_whitespace() {
            let mut indices: Vec<u8> = Vec::with_capacity(raw_word.len() + 2);
            indices.push(0); // start boundary '^'
            for ch in raw_word.chars() {
                for lc in ch.to_lowercase() {
                    if let Some(idx) = char_to_idx(lc) {
                        indices.push(idx);
                    }
                }
            }
            if indices.len() <= 2 {
                continue;
            }
            indices.push(0); // end boundary '$'

            let w = weight as u64;
            for win in indices.windows(2) {
                let key = ((win[0] as u16) << 8) | (win[1] as u16);
                *bigram_counts.entry(key).or_insert(0) += w;
                total_bigrams += w;
            }
            for win in indices.windows(3) {
                let key = ((win[0] as u32) << 16) | ((win[1] as u32) << 8) | (win[2] as u32);
                *trigram_counts.entry(key).or_insert(0) += w;
                total_trigrams += w;
            }
        }
    }

    let alpha_n = alpha_chars.len() as f64;
    let uniform_bigram = 1.0 / ((alpha_n + 1.0) * (alpha_n + 1.0));
    let ref_trigram = 0.00018_f64;

    let mut bigrams: Vec<(u16, i8)> = Vec::with_capacity(bigram_counts.len());
    for (key, count) in bigram_counts {
        let p = (count as f64) / (total_bigrams as f64);
        let raw = (14.0 * (p / uniform_bigram).ln()).round() as i32;
        let score = raw.clamp(-45, 58) as i8;
        bigrams.push((key, score));
    }

    let mut trigrams: Vec<(u32, i8)> = Vec::with_capacity(trigram_counts.len());
    for (key, count) in trigram_counts {
        let p = (count as f64) / (total_trigrams as f64);
        let raw = (12.0 * (p / ref_trigram).ln()).round() as i32;
        let score = raw.clamp(-35, 62) as i8;
        trigrams.push((key, score));
    }

    let default_bigram_score: i8 = -55;
    let default_trigram_score: i8 = -45;
    let oov_char_penalty: i8 = -90;

    let mut out = Vec::with_capacity(32 + alpha_chars.len() * 2 + bigrams.len() * 3 + trigrams.len() * 5);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(spec.script);
    out.extend_from_slice(&spec.primary_lang.to_le_bytes());
    out.extend_from_slice(&(alpha_chars.len() as u16).to_le_bytes());
    out.extend_from_slice(&vowel_mask.to_le_bytes());
    out.extend_from_slice(&(bigrams.len() as u16).to_le_bytes());
    out.extend_from_slice(&(trigrams.len() as u16).to_le_bytes());
    out.push(default_bigram_score as u8);
    out.push(default_trigram_score as u8);
    out.push(oov_char_penalty as u8);
    out.push(0u8); // reserved

    for code in &alpha_chars {
        out.extend_from_slice(&code.to_le_bytes());
    }
    for &(key, score) in &bigrams {
        out.extend_from_slice(&key.to_le_bytes());
        out.push(score as u8);
    }
    for &(key, score) in &trigrams {
        out.extend_from_slice(&key.to_le_bytes());
        out.push(score as u8);
    }

    out
}

const EN_CORPUS: &[(&str, u32)] = &[
    // High-frequency core English vocabulary & conversational/technical phrases
    ("the and that have for not with you this but his from they say her she will one all would there their what out about who get which when make can like time just him know take people into year your good some could them see other than then now look only come its over think also back after use two how our work first well way even new want because any these give day most", 50),
    ("hello world welcome please thank thanks sorry morning evening night today tomorrow yesterday friend message system program function window keyboard layout button click setting config file folder path process thread memory buffer string number value result option error warning check build release test debug version update install", 42),
    ("should never between through without another something nothing everything someone everyone anyone always never often sometimes usually already again before around under where while", 25),
    ("important different following available possible necessary national political social economic", 20),
    ("question answer problem solution example change create delete remove select insert support service control", 30),
    ("project company business market product customer client server network database request response", 32),
    ("working writing reading typing looking running starting stopping waiting calling sending receiving", 35),
    ("application development interface architecture performance security privacy license documentation", 28),
    ("understand remember within without together however therefore although whether actually probably", 28),
    ("computer software hardware internet browser website page link image video audio sound", 28),
    ("general specific simple complex public private local global default custom current active", 28),
    ("history science nature language english russian german french spanish italian", 22),
    ("water house place point right small large great young early state country", 24),
    ("return struct impl enum match const static async await crate module trait", 24),
    ("family mother father brother sister child children school student teacher", 22),
    ("money price order payment account profile report status table list group", 24),
    ("happy great nice clear clean fast quick slow easy hard strong light dark", 22),
    ("letter word sentence paragraph text document article book story paper", 26),
    ("need feel keep seem meet free tree week deep sleep green speed street", 26),
    ("here there where were ever never over under after water other father", 32),
];

const RU_CORPUS: &[(&str, u32)] = &[
    // High-frequency core Russian vocabulary & conversational/technical phrases
    ("и в не на что я с он а как то все она так его но да ты к у же вы за бы по только ее мне было вот от меня еще нет о из ему теперь когда даже ну вдруг ли если уже или ни быть был него до вас нибудь опять уж вам ведь там потом себя ничего ей может они тут где есть надо ней для мы тебя их чем была сам чтоб без будто чего раз тоже себе под будет ж тогда кто этот того потому этого какой совсем ним здесь этого один почти мой тем чтобы нее сейчас были куда зачем всех никогда можно при наконец два об другой хоть после над больше тот через эти нас про всего них какая много разве три эту моя впрочем хорошо свою этой перед иногда лучше чуть том нельзя такой им более всегда конечно всю между", 50),
    ("привет здравствуйте спасибо пожалуйста сообщение система программа функция окно клавиатура раскладка кнопка настройка файл папка процесс поток память буфер строка число значение результат ошибка проверка сборка версия установка", 45),
    ("добрый день утро вечер ночь сегодня завтра вчера друг работа время человек жизнь вопрос ответ дело место рука глаз лицо сторона голова дом слово вид сила конец ребенок город земля вода отец женщина машина страна мир случай", 40),
    ("работать писать читать печатать смотреть делать говорить знать думать хотеть мочь идти стоять видеть казаться остаться начаться понимать помнить ждать спрашивать отвечать получать отправлять", 36),
    ("который наш свой ваш такой другой каждый весь сам самый новый старый большой маленький хороший плохой первый последний главный общий полный простой сложный русский английский", 35),
    ("проект компания клиент сервер сеть база запрос ответ таблица список группа профиль аккаунт оплата заказ цена деньги договор документ письмо текст книга статья страница сайт браузер интернет", 32),
    ("разработка приложение интерфейс архитектура безопасность приватность лицензия документация поддержка управление контроль сервис решение проблема пример изменение создание удаление выбор", 30),
    ("почему потому поэтому однако конечно наверное действительно просто очень совсем сразу снова вместе вокруг около против после перед через между", 32),
    ("хлеб гриб блок строка также жизнь хорошо люблю тебя меня себя дням годам людям делам словам руку руки руке рукой руках голову", 30),
];

fn main() {
    let specs = [
        LangSpec {
            file_name: "en.bin",
            script: SCRIPT_LATIN,
            primary_lang: LANG_ENGLISH,
            alphabet: "abcdefghijklmnopqrstuvwxyz",
            vowels: "aeiouy",
            corpus: EN_CORPUS,
        },
        LangSpec {
            file_name: "ru.bin",
            script: SCRIPT_CYRILLIC,
            primary_lang: LANG_RUSSIAN,
            alphabet: "абвгдеёжзийклмнопрстуфхцчшщъыьэюя",
            vowels: "аеёиоуыэюя",
            corpus: RU_CORPUS,
        },
        LangSpec {
            file_name: "uk.bin",
            script: SCRIPT_CYRILLIC,
            primary_lang: LANG_UKRAINIAN,
            alphabet: "абвгґдеєжзиіїйклмнопрстуфхцчшщьюя",
            vowels: "аеєиіїоуюя",
            corpus: corpora::UK_CORPUS,
        },
        LangSpec {
            file_name: "be.bin",
            script: SCRIPT_CYRILLIC,
            primary_lang: LANG_BELARUSIAN,
            alphabet: "абвгдеёжзійклмнопрстуўфхцчшыьэюя",
            vowels: "аеёіоуыэюя",
            corpus: corpora::BE_CORPUS,
        },
        LangSpec {
            file_name: "kk.bin",
            script: SCRIPT_CYRILLIC,
            primary_lang: LANG_KAZAKH,
            alphabet: "аәбвгғдеёжзийкқлмнңоөпрстуұүфхһцчшщъыіьэюя",
            vowels: "аәеёиоөуұүыіэюя",
            corpus: corpora::KK_CORPUS,
        },
        LangSpec {
            file_name: "de.bin",
            script: SCRIPT_LATIN,
            primary_lang: LANG_GERMAN,
            alphabet: "abcdefghijklmnopqrstuvwxyzäöüß",
            vowels: "aeiouyäöü",
            corpus: corpora::DE_CORPUS,
        },
        LangSpec {
            file_name: "fr.bin",
            script: SCRIPT_LATIN,
            primary_lang: LANG_FRENCH,
            alphabet: "abcdefghijklmnopqrstuvwxyzàâçéèêëîïôùûüœ",
            vowels: "aeiouyàâéèêëîïôùûüœ",
            corpus: corpora::FR_CORPUS,
        },
        LangSpec {
            file_name: "es.bin",
            script: SCRIPT_LATIN,
            primary_lang: LANG_SPANISH,
            alphabet: "abcdefghijklmnopqrstuvwxyzáéíñóúü",
            vowels: "aeiouyáéíóúü",
            corpus: corpora::ES_CORPUS,
        },
        LangSpec {
            file_name: "pt.bin",
            script: SCRIPT_LATIN,
            primary_lang: LANG_PORTUGUESE,
            alphabet: "abcdefghijklmnopqrstuvwxyzàáâãçéêíóôõú",
            vowels: "aeiouyàáâãéêíóôõú",
            corpus: corpora::PT_CORPUS,
        },
        LangSpec {
            file_name: "it.bin",
            script: SCRIPT_LATIN,
            primary_lang: LANG_ITALIAN,
            alphabet: "abcdefghijklmnopqrstuvwxyzàèéìòù",
            vowels: "aeiouyàèéìòù",
            corpus: corpora::IT_CORPUS,
        },
        LangSpec {
            file_name: "tr.bin",
            script: SCRIPT_LATIN,
            primary_lang: LANG_TURKISH,
            alphabet: "abcçdefgğhıijklmnoöprsştuüvyz",
            vowels: "aeıioöuü",
            corpus: corpora::TR_CORPUS,
        },
        LangSpec {
            file_name: "pl.bin",
            script: SCRIPT_LATIN,
            primary_lang: LANG_POLISH,
            alphabet: "aąbcćdeęfghijklłmnńoóprsśtuwyzźż",
            vowels: "aąeęioóuy",
            corpus: corpora::PL_CORPUS,
        },
        LangSpec {
            file_name: "nl.bin",
            script: SCRIPT_LATIN,
            primary_lang: LANG_DUTCH,
            alphabet: "abcdefghijklmnopqrstuvwxyz",
            vowels: "aeiouy",
            corpus: corpora::NL_CORPUS,
        },
        LangSpec {
            file_name: "sv.bin",
            script: SCRIPT_LATIN,
            primary_lang: LANG_SWEDISH,
            alphabet: "abcdefghijklmnopqrstuvwxyzåäö",
            vowels: "aeiouyåäö",
            corpus: corpora::SV_CORPUS,
        },
        LangSpec {
            file_name: "cs.bin",
            script: SCRIPT_LATIN,
            primary_lang: LANG_CZECH,
            alphabet: "aábcčdďeéěfghiíjklmnňoópqrřsštťuúůvwxyýzž",
            vowels: "aáeéěiíoóuúůyý",
            corpus: corpora::CS_CORPUS,
        },
        LangSpec {
            file_name: "el.bin",
            script: SCRIPT_GREEK,
            primary_lang: LANG_GREEK,
            alphabet: "αάβγδεέζηήθιίϊΐκλμνξοόπρσςτυύϋΰφχψωώ",
            vowels: "αάεέηήιίϊΐοόυύϋΰωώ",
            corpus: corpora::EL_CORPUS,
        },
        LangSpec {
            file_name: "he.bin",
            script: SCRIPT_HEBREW,
            primary_lang: LANG_HEBREW,
            alphabet: "אבגדהוזחטיךכלםמןנסעףפץצקרשת",
            vowels: "אהويאע",
            corpus: corpora::HE_CORPUS,
        },
        LangSpec {
            file_name: "ar.bin",
            script: SCRIPT_ARABIC,
            primary_lang: LANG_ARABIC,
            alphabet: "ءآأؤإئابةتثجحخدذرزسشصضطظعغفقكلمنهوىي",
            vowels: "اأإآوىي",
            corpus: corpora::AR_CORPUS,
        },
        LangSpec {
            file_name: "hy.bin",
            script: SCRIPT_ARMENIAN,
            primary_lang: LANG_ARMENIAN,
            alphabet: "աբգդեզէըթժիլխծկհձղճմյնշոչպջռսվտրցւփքօֆև",
            vowels: "աեէըիոօև",
            corpus: corpora::HY_CORPUS,
        },
        LangSpec {
            file_name: "ka.bin",
            script: SCRIPT_GEORGIAN,
            primary_lang: LANG_GEORGIAN,
            alphabet: "აბგდევზთიკლმნოპჟრსტუფქღყშჩცძწჭხჯჰ",
            vowels: "აეიოუ",
            corpus: corpora::KA_CORPUS,
        },
    ];

    let out_dir = Path::new("assets/profiles");
    fs::create_dir_all(out_dir).expect("create assets/profiles directory");

    let mut total_bytes = 0usize;
    for spec in &specs {
        let bytes = build_profile_bytes(spec);
        let path = out_dir.join(spec.file_name);
        fs::write(&path, &bytes).expect("write profile bin");
        let bg = u16::from_le_bytes([bytes[18], bytes[19]]);
        let tg = u16::from_le_bytes([bytes[20], bytes[21]]);
        total_bytes += bytes.len();
        println!(
            "Generated {}: {} bytes ({} bigrams, {} trigrams)",
            path.display(),
            bytes.len(),
            bg,
            tg
        );
    }
    println!("Total size across {} profiles: {} bytes", specs.len(), total_bytes);
}
