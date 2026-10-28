# SmartBlink

[![Status: Public Beta](https://img.shields.io/badge/status-v0.1.0--beta%20(needs%20testing)-f59e0b?style=flat-square)](https://github.com/VKVstudio/SmartBlink/issues)
[![Release](https://img.shields.io/github/v/release/VKVstudio/SmartBlink?include_prereleases&style=flat-square&color=00b4d8)](https://github.com/VKVstudio/SmartBlink/releases/tag/v0.1.0-beta)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011%20(x64)-0078d4?style=flat-square)](https://github.com/VKVstudio/SmartBlink/releases/tag/v0.1.0-beta)
[![License: MIT](https://img.shields.io/badge/license-MIT-2ea44f?style=flat-square)](LICENSE)

**Wrong-layout typing alerts on your keyboard LED and a single-gesture word fix for Windows 10/11. Built for developers and bilingual typists who want instant layout correction without automatic text replacement breaking their code, passwords, or terminal commands.**

**[Build from Source (`cargo build --release`)](#get-started-build-from-source-or-grab-the-exe)** · **[Download `smartblink.exe` (`v0.1.0-beta`)](https://github.com/VKVstudio/SmartBlink/releases/download/v0.1.0-beta/smartblink.exe)** · **[All Releases](https://github.com/VKVstudio/SmartBlink/releases/tag/v0.1.0-beta)** · **[Русская версия (`README.ru.md`)](README.ru.md)**

![SmartBlink Settings and Tray Menu](SmartBlink_preview.png)

> **Early Beta (`v0.1.0-beta`) - Call for Real-World Testing:**
> This is an early public beta, not a finished 1.0 release. The core hook engine and `EN <-> RU` typing are tested in daily live work on Windows 11. The other 18 languages and 43 Windows keyboard drivers pass 49 automated tests against the real `C:\Windows\System32\KBD*.DLL` files, but automated tests don't replace human fingers. If you type in `UK`, `BE`, `KK`, `DE`, `FR`, `ES`, `PT`, `IT`, `NL`, `SV`, `PL`, `CS`, `EL`, `TR`, `HE`, `AR`, `HY`, or `KA` and spot a false alert, a missed word, or an app where `SendInput` behaves oddly, please [open an Issue](https://github.com/VKVstudio/SmartBlink/issues) with the word and layout pair.

## The Problem

If you type in two or three languages and look at the keyboard instead of the monitor, you've been there. You type five words, look up at the screen, and see `Ghbdtn rfr ltkf` instead of `Привет как дела` (or `Руддщ цщкдв` instead of `Hello world`). Then you're stuck holding Backspace, switching the layout, and typing the whole line again.

Traditional auto-replacing switchers try to guess the language and rewrite your text while you're typing. That's fine for plain prose, right up until you write a shell command, a SQL query, a camelCase variable, a brand name, or a password in a custom input box. One wrong guess and your text gets mangled mid-sentence, so you're fighting the tool that's supposed to help you.

## How SmartBlink Works

SmartBlink separates **detection** from **correction**. It won't ever touch your text on its own.

1. **You type as usual.** When a word doesn't match your active keyboard layout, SmartBlink blinks your keyboard LED (`Scroll Lock` by default, or guarded `Caps Lock`) and plays a soft 24 ms low-passed wooden knock through WASAPI. You'll catch the mistake right away without looking up from the keys.
2. **You decide whether to fix it.** Double-tap `Left Shift` within 300 ms, and SmartBlink replaces the last word (or the whole wrong-layout phrase you've just typed) and switches the window's keyboard layout.
3. **Changed your mind?** Double-tap `Left Shift` again within 10 seconds and it'll undo the fix and switch back. If you ignore the alert and keep typing, SmartBlink stays out of your way. Ignore the same word three times and it's whitelisted for the rest of your session.

## Approach Comparison

| What matters in daily use | **SmartBlink (`v0.1.0-beta`)** | **Auto-Replacing Switchers** | **Custom Macro Scripts** |
|---|---|---|---|
| Rewrites text on its own while you type? | **Never** (only on your gesture) | Yes (can alter code, slang, or commands) | Manual hotkey only |
| Alerts your eyes on the keyboard LED? | **Yes** (net-zero LED + 24 ms knock) | Audio or visual popup only | No |
| Languages supported out of the box | **20 languages** (`131 KB` total) | Usually 2 active languages | Manual character tables |
| Works with `AltGr`, dead keys, and RTL (`AR`/`HE`)? | **Yes** (45 Windows `KBD*.DLL` drivers) | Partial | Fragile |
| Stops automatically in password fields? | **Yes** (Windows UI Automation `IsPassword`) | Varies by app | No |
| Keeps rich clipboard data on `Ctrl+Alt+Z`? | **Yes** (all `HGLOBAL` formats byte-for-byte) | Often plain text only | Overwrites clipboard |
| Memory and disk size | **`< 3 MB` RAM / `666 KB` single `.exe`** | 20 to 40 MB RAM | Requires scripting runtime |
| Source code & network policy | **MIT open source** (RAM-only, zero network APIs) | Usually closed-source | Varies |

## 20 Languages and 45 Windows Keyboard Drivers

Most layout tools hardcode a single US QWERTY to Russian JCUKEN table and fall apart as soon as you use French AZERTY, German QWERTZ, Czech top-row caron letters, Polish `AltGr` diacritics, Spanish dead keys, Arabic `لا` ligatures, or Georgian shifted consonants.

SmartBlink reads your installed Windows keyboard layouts (`HKL`) directly via `ToUnicodeEx` across all four modifier layers (`Base`, `Shift`, `AltGr`, `Shift+AltGr`) and scores candidates against embedded bigram and trigram models (`assets/profiles/*.bin`). Every build is checked against all 45 official `C:\Windows\System32\KBD*.DLL` drivers shipped with Windows 11:

| Script / Group | Languages (`20`) | Windows 11 Layouts in Test Matrix (`45 DLLs`) | Example Wrong-Layout Input -> Fixed Output |
|---|---|---|---|
| **Cyrillic & Slavic** | `RU`, `UK`, `BE`, `KK`, `PL`, `CS` | JCUKEN, Typewriter, UK-101, UK-Enhanced (`ґ`), Belarusian (`ў`), Kazakh, Polish Programmers & 214, Czech QWERTZ & QWERTY | `ghbdtn rfr ltkf` -> `привет как дела`<br>`\heyn` -> `ґрунт`<br>`c2ktv` -> `сәлем`<br>`4e3tina` -> `čeština` |
| **Western & Northern Europe** | `EN`, `DE`, `FR`, `ES`, `PT`, `IT`, `NL`, `SV` | US, UK, US-Intl, German QWERTZ, Swiss DE/FR, French AZERTY (Legacy, Standard, Belgian), Spanish & LatAm, Portuguese & ABNT2, Italian & 142, Dutch, Swedish | `stra-e` -> `straße`<br>`c4est d2j0` -> `c'est déjà`<br>`tambi'en` -> `también`<br>`cora;'ao` -> `coração`<br>`ocks[` -> `också` |
| **Mediterranean & Turkish** | `EL`, `TR` | Greek (Standard, 220, 319, Polytonic), Turkish Q & Turkish F | `kalhm;era` -> `καλημέρα`<br>`g]naydin` -> `günaydın` |
| **Right-to-Left (Abjads)** | `AR`, `HE` | Arabic 101, 102 (`ذ` on `\`), 102-AZERTY, Hebrew Legacy, Standard, and SI-2018 | `fb` -> `بلا` (2-char `لا` ligature)<br>`lsHgm` -> `مسألة`<br>`akuo` -> `שלום` |
| **Caucasian** | `HY`, `KA` | Armenian Eastern, Western, Phonetic, Typewriter; Georgian QWERTY, Ergonomic, Legacy, MES | `bar&` -> `բարև`<br>`gTxovT` -> `გთხოვთ` |

## Default Shortcuts

| Action | Default Shortcut | Alternatives in Settings |
|---|---|---|
| **Fix last word or wrong-layout burst** | Double-tap `Left Shift` (`<= 300 ms`) | Double-tap `Left Ctrl` or single `Pause/Break` |
| **Undo the last fix** (within 10 seconds) | Same gesture again (`2x Left Shift`) | Follows your chosen fix key |
| **Convert highlighted text** | `Ctrl + Left Alt + Z` | Preserves all clipboard formats byte-for-byte; ignores `Right Alt` (`AltGr`) so Polish `AltGr+Z` (`ż`) works normally |

## Get Started: Build from Source or Grab the `.exe`

### Option 1: Build from Source in One Command (Recommended)

Because a keyboard utility sees every key you press, you shouldn't have to trust a precompiled binary. SmartBlink has only one Rust dependency (`windows` crate by Microsoft) and builds from source in a few seconds:

```powershell
git clone https://github.com/VKVstudio/SmartBlink.git
cd SmartBlink
cargo build --release
```

That produces a standalone `target\release\smartblink.exe` (`~666 KB`). To run the 49 automated tests (including the read-only check across all 45 Windows 11 `C:\Windows\System32\KBD*.DLL` layout drivers on your machine):

```powershell
cargo test --release
```

### Option 2: Prebuilt `.exe` (If You Just Want to Run It)

If you don't have the Rust toolchain installed and just want a ready-made binary to test right away, grab it directly from **[GitHub Releases (`v0.1.0-beta`)](https://github.com/VKVstudio/SmartBlink/releases/tag/v0.1.0-beta)**:

- **[`smartblink.exe`](https://github.com/VKVstudio/SmartBlink/releases/download/v0.1.0-beta/smartblink.exe)** (`675 KB`) - single portable executable. Put it anywhere and run it; no installer or runtime needed.
- **[`SmartBlink-Setup-0.1.0-beta.exe`](https://github.com/VKVstudio/SmartBlink/releases/download/v0.1.0-beta/SmartBlink-Setup-0.1.0-beta.exe)** (`2.25 MB`) - optional per-user installer with Start Menu shortcut and clean uninstaller.

Once `smartblink.exe` is running:

1. **Left-click** the tray icon (or launch `smartblink.exe` a second time) to open the **Settings** window. Click **Trigger Test Alert Now** in the top banner to hear the 24 ms knock and watch your keyboard LED blink.
2. **Right-click** the tray icon for quick one-click toggles (Pause, Alert Mode, LED Target, Fix Key, Run at Startup, and optional Admin Task for fixing text inside elevated windows).

## Why It's Safe to Run at Work

- **Keystrokes never leave RAM.** SmartBlink stores only the last 256 physical scan-code events in a lock-free ring buffer and a 64-slot active word buffer. Both buffers get overwritten with zeros (`std::ptr::write_volatile`) whenever you click the mouse, press an arrow key, switch windows, pause for 3.5 seconds, or close the app.
- **No network code linked.** The binary links only standard Win32 OS libraries (`user32`, `gdi32`, `ole32`, `mmdevapi`). There's no HTTP client, no telemetry SDK, no crash uploader, and no auto-updater.
- **Automatic immunity where you don't want it.** SmartBlink stays silent in password fields (detected via Windows UI Automation `CurrentIsPassword`), active IME composition windows, URLs, file paths, CLI flags (`--release`), and blacklisted developer tools (`WindowsTerminal.exe`, `cmd.exe`, `powershell.exe`, `pwsh.exe`, `Code.exe`, `devenv.exe`, `idea64.exe`, `nvim.exe`, and others you can edit right in the Settings window).
- **Minimal hook thread.** The `WH_KEYBOARD_LL` and `WH_MOUSE_LL` callbacks only push a 16-byte struct to the ring buffer and return right away. There's zero allocation, zero audio playback, and zero string processing on the input hook thread, so it won't add a microsecond of lag to your typing.

## Frequently Asked Questions

**Will SmartBlink ever change what I type without me pressing the fix key?**
No, it won't. That's the first rule in [`AGENTS.md`](AGENTS.md) and [`ARCHITECTURE.md`](ARCHITECTURE.md). SmartBlink only alerts you with a light blink and a soft click. Your text and active layout don't change unless you double-tap `Left Shift` (or your configured fix key) or press `Ctrl+Alt+Z` on selected text.

**Why does it blink `Scroll Lock` by default instead of `Caps Lock`?**
If an app toggles `Caps Lock` while your fingers are moving at 400 CPM, Windows can invert the case of the next letter you type (`hELLO`). `Scroll Lock` sits on the same physical LED cluster on full-size keyboards and doesn't affect letter case. If you're on a compact laptop that only has a `Caps Lock` LED, switch LED Target to **Caps Lock (Guarded)** in Settings: it waits for an 80 ms pause between keystrokes, aborts the blink the instant you touch a key, and restores your original `Caps Lock` state.

**What if I want to fix text inside an Administrator Command Prompt or Registry Editor?**
By Windows User Interface Privilege Isolation (UIPI), a normal user-level app can't inject keystrokes into an elevated Administrator window. If you'd like SmartBlink to work in elevated windows too, enable **Run as Admin Task** in the tray menu or Settings. That registers an optional Windows Scheduled Task (`ONLOGON`) so SmartBlink starts with elevated rights and won't nag you with a UAC dialog at every login.

## Documentation & Source

- [Architecture & Technical Specification (`ARCHITECTURE.md`)](ARCHITECTURE.md)
- [20-Language & 45-DLL Keyboard Research (`.system/layouts_research.md`)](.system/layouts_research.md)
- [Configuration File Reference (`docs/CONFIG.md`)](docs/CONFIG.md)
- [Privacy & Threat Model (`docs/PRIVACY.md`)](docs/PRIVACY.md)
- [Building from Source (`docs/BUILDING.md`)](docs/BUILDING.md)
- [Contributing (`CONTRIBUTING.md`)](CONTRIBUTING.md)
- [Language Profile Licenses (`assets/profiles/LICENSES.md`)](assets/profiles/LICENSES.md)

## License

MIT License. Copyright (c) 2026 [VKV Studio](https://vkvstudio.com). See [`LICENSE`](LICENSE) for full text.
