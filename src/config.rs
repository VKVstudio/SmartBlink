//! Config model + TOML codec (hand-rolled: one-key-per-line flat TOML subset)
//! and atomic load/save. No typed-text fields ever (privacy invariant).

use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub const APP_DIR_NAME: &str = "SmartBlink";
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Alert channel selection. Mirrors ARCHITECTURE.md §3.4/§3.5 config keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyMode {
    Silent,
    Led,
    Sound,
    Both,
}

impl NotifyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            NotifyMode::Silent => "silent",
            NotifyMode::Led => "led",
            NotifyMode::Sound => "sound",
            NotifyMode::Both => "both",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "silent" => Some(NotifyMode::Silent),
            "led" => Some(NotifyMode::Led),
            "sound" => Some(NotifyMode::Sound),
            "both" => Some(NotifyMode::Both),
            _ => None,
        }
    }
}

/// Fix gesture trigger. M1 stores the value; detection of it arrives in M3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixTrigger {
    DoubleLeftShift,
    DoubleLeftCtrl,
    PauseBreak,
}

impl FixTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            FixTrigger::DoubleLeftShift => "double_left_shift",
            FixTrigger::DoubleLeftCtrl => "double_left_ctrl",
            FixTrigger::PauseBreak => "pause_break",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "double_left_shift" => Some(FixTrigger::DoubleLeftShift),
            "double_left_ctrl" => Some(FixTrigger::DoubleLeftCtrl),
            "pause_break" => Some(FixTrigger::PauseBreak),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub notify_mode: NotifyMode,
    pub led_mode: LedMode,
    pub fix_trigger: FixTrigger,
    pub autostart: bool,
    /// Blacklisted process image names (lowercase, without `.exe`). Passive in these.
    pub blacklist: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedMode {
    /// Scroll Lock only — never corrupts typing register (default, §3.4).
    ScrollLock,
    /// Caps Lock allowed only during key gaps >= ~80ms, register protected.
    CapsLockGuarded,
}

impl LedMode {
    pub fn as_str(self) -> &'static str {
        match self {
            LedMode::ScrollLock => "scroll_lock",
            LedMode::CapsLockGuarded => "caps_guarded",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "scroll_lock" => Some(LedMode::ScrollLock),
            "caps_guarded" => Some(LedMode::CapsLockGuarded),
            _ => None,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            notify_mode: NotifyMode::Both,
            led_mode: LedMode::ScrollLock,
            fix_trigger: FixTrigger::DoubleLeftShift,
            autostart: false,
            blacklist: vec![
                "windowsterminal".into(),
                "cmd".into(),
                "powershell".into(),
                "pwsh".into(),
                "conhost".into(),
                "code".into(),
                "devenv".into(),
            ],
        }
    }
}

pub fn app_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs_fallback());
    base.join(APP_DIR_NAME)
}

fn dirs_fallback() -> PathBuf {
    // Last-resort: %USERPROFILE%\AppData\Roaming without APPDATA env.
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    PathBuf::from(home).join("AppData").join("Roaming")
}

pub fn config_path() -> PathBuf {
    app_dir().join(CONFIG_FILE_NAME)
}

/// Serialize to the flat TOML subset we own (stable key order, LF endings).
pub fn to_toml(cfg: &Config) -> String {
    let mut out = String::new();
    out.push_str("# SmartBlink config. No typed text is ever stored here.\n");
    out.push_str(&format!("notify_mode = \"{}\"\n", cfg.notify_mode.as_str()));
    out.push_str(&format!("led_mode = \"{}\"\n", cfg.led_mode.as_str()));
    out.push_str(&format!("fix_trigger = \"{}\"\n", cfg.fix_trigger.as_str()));
    out.push_str(&format!("autostart = {}\n", cfg.autostart));
    out.push_str("blacklist = [");
    for (i, b) in cfg.blacklist.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let escaped = b.replace('\\', "\\\\").replace('"', "\\\"");
        out.push_str(&format!("\"{escaped}\""));
    }
    out.push_str("]\n");
    out
}

/// Parse our own TOML subset. Unknown keys are ignored; malformed lines skipped.
pub fn from_toml(s: &str) -> Config {
    let mut cfg = Config::default();
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();
        match key {
            "notify_mode" => {
                if let Some(v) = NotifyMode::parse(unquote(val)) {
                    cfg.notify_mode = v;
                }
            }
            "led_mode" => {
                if let Some(v) = LedMode::parse(unquote(val)) {
                    cfg.led_mode = v;
                }
            }
            "fix_trigger" => {
                if let Some(v) = FixTrigger::parse(unquote(val)) {
                    cfg.fix_trigger = v;
                }
            }
            "autostart" => {
                cfg.autostart = val == "true";
            }
            "blacklist" => {
                cfg.blacklist = parse_string_array(val);
            }
            _ => {}
        }
    }
    cfg
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn parse_string_array(s: &str) -> Vec<String> {
    let s = s.trim();
    let Some(inner) = s.strip_prefix('[').and_then(|r| r.strip_suffix(']')) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() || c == ',' {
            continue;
        }
        if c == '"' {
            let mut item = String::new();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    if let Some(esc) = chars.next() {
                        item.push(esc);
                    }
                } else if ch == '"' {
                    break;
                } else {
                    item.push(ch);
                }
            }
            out.push(item);
        } else {
            let mut item = String::from(c);
            while let Some(&ch) = chars.peek() {
                if ch == ',' {
                    break;
                }
                item.push(ch);
                chars.next();
            }
            let trimmed = item.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
    }
    out
}

/// Load config; create with defaults on first run. Corrupt file -> defaults (never crash).
pub fn load_or_create() -> Config {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(text) => from_toml(&text),
        Err(_) => {
            let cfg = Config::default();
            // Best-effort first-write; failure is non-fatal (in-memory defaults).
            let _ = save(&cfg);
            cfg
        }
    }
}

/// Atomic rewrite: temp file in same dir, sync_all, rename over target.
pub fn save(cfg: &Config) -> std::io::Result<()> {
    let dir = app_dir();
    fs::create_dir_all(&dir)?;
    let path = config_path();
    let tmp = dir.join(format!("{CONFIG_FILE_NAME}.tmp"));
    let mut f = fs::File::create(&tmp)?;
    f.write_all(to_toml(cfg).as_bytes())?;
    f.sync_all()?;
    drop(f);
    fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_defaults() {
        let cfg = Config::default();
        let text = to_toml(&cfg);
        let back = from_toml(&text);
        assert_eq!(back.notify_mode, cfg.notify_mode);
        assert_eq!(back.led_mode, cfg.led_mode);
        assert_eq!(back.fix_trigger, cfg.fix_trigger);
        assert_eq!(back.autostart, cfg.autostart);
        assert_eq!(
            back.blacklist,
            vec![
                "windowsterminal",
                "cmd",
                "powershell",
                "pwsh",
                "conhost",
                "code",
                "devenv"
            ]
        );
    }

    #[test]
    fn roundtrip_custom() {
        let cfg = Config {
            notify_mode: NotifyMode::Silent,
            led_mode: LedMode::CapsLockGuarded,
            fix_trigger: FixTrigger::PauseBreak,
            autostart: true,
            blacklist: vec!["code".into(), "chrome".into()],
        };
        let back = from_toml(&to_toml(&cfg));
        assert_eq!(back.notify_mode, NotifyMode::Silent);
        assert_eq!(back.led_mode, LedMode::CapsLockGuarded);
        assert_eq!(back.fix_trigger, FixTrigger::PauseBreak);
        assert!(back.autostart);
        assert_eq!(back.blacklist, vec!["code", "chrome"]);
    }

    #[test]
    fn corrupt_text_yields_defaults() {
        let back = from_toml("notify_mode = \"nonsense\"\n???garbage\nautostart = maybe");
        assert_eq!(back.notify_mode, NotifyMode::Both);
        assert!(!back.autostart);
        assert_eq!(back.blacklist, Config::default().blacklist);
    }

    #[test]
    fn blacklist_escapes_and_commas_roundtrip() {
        let expected: Vec<String> = vec![
            r#"we"ird, app"#.into(),
            r"back\slash".into(),
            "term".into(),
        ];
        let cfg = Config {
            blacklist: expected.clone(),
            ..Default::default()
        };
        let back = from_toml(&to_toml(&cfg));
        assert_eq!(back.blacklist, expected);
    }

    #[test]
    fn empty_blacklist_roundtrip() {
        let back = from_toml("blacklist = []\n");
        assert!(back.blacklist.is_empty());
    }
}
