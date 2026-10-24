# Configuration Reference (`config.toml`)

SmartBlink stores its configuration in:

```text
%APPDATA%\SmartBlink\config.toml
```

The file is created automatically with safe defaults on first launch and updated atomically (write to `config.toml.tmp`, `FlushFileBuffers` / `sync_all`, rename over `config.toml`) whenever settings are changed via the tray menu or the Settings window.

> **Privacy Guarantee:** `config.toml` never stores typed words, keystrokes, or session whitelist entries.

## Default `config.toml`

```toml
# SmartBlink config. No typed text is ever stored here.
notify_mode = "both"
led_mode = "scroll_lock"
fix_trigger = "double_left_shift"
autostart = false
blacklist = ["windowsterminal", "cmd", "powershell", "pwsh", "conhost", "code", "devenv"]
```

## Keys

| Key | Allowed Values | Default | Description |
|---|---|---|---|
| `notify_mode` | `"both"`, `"sound"`, `"led"`, `"silent"` | `"both"` | Selects which alert channels fire when wrong-layout typing is detected. |
| `led_mode` | `"scroll_lock"`, `"caps_guarded"` | `"scroll_lock"` | Selects which keyboard LED blinks. `"scroll_lock"` never affects letter casing; `"caps_guarded"` blinks Caps Lock only during typing pauses (`>= 80 ms`) and aborts/restores immediately if a real keystroke arrives. |
| `fix_trigger` | `"double_left_shift"`, `"double_left_ctrl"`, `"pause_break"` | `"double_left_shift"` | Manual gesture (`<= 300 ms` double-tap with no intervening keys) that converts the last typed word and switches the active window's keyboard layout. |
| `autostart` | `true`, `false` | `false` | Mirrors the `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\SmartBlink` startup registration. |
| `blacklist` | Array of lowercase process stems (without `.exe`) | `["windowsterminal", ...]` | Processes where SmartBlink stays completely passive (no detection, no alerts, no hotkeys). |
