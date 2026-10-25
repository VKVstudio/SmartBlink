# SmartBlink

**Privacy-first wrong-layout typing detector and one-gesture manual word fixer for Windows 10/11 — the safe Punto Switcher alternative that never auto-corrupts your text.**

*[Русская документация (README.ru.md)](README.ru.md)*

![SmartBlink — Canonical Dark Bento Settings UI & Tray Flyout](SmartBlink_preview.png)

Most touch and semi-touch typists look at the **keyboard**, not the monitor, and notice they typed in the wrong keyboard layout (`Ghbdtn` instead of `Привет`, or `Руддщ` instead of `Hello`) only after several words. Traditional auto-switchers (like Punto Switcher or Caramba Switcher) silently mutate code, passwords, terminal commands, abbreviations, and bilingual terms.

**SmartBlink** solves this cleanly:
1. **Alerts you where your eyes are:** blinks the keyboard LED (`Scroll Lock` by default, or guarded `Caps Lock`) and plays a subtle 24 ms low-passed "wooden knock" click via WASAPI.
2. **Never mutates your text automatically:** only when **you** double-tap `Left Shift` (or press `Ctrl+Alt+Z` on a selection) does SmartBlink replace the word or burst and switch the window's keyboard layout. Tap the gesture a second time immediately to undo.

---

## Why SmartBlink vs. Punto Switcher / AutoHotkey?

| Feature | **SmartBlink** | **Punto Switcher** | **Caramba Switcher** | **AHK Scripts** |
|---|---|---|---|---|
| **Auto-replaces text while typing?** | **Never** (100% manual gesture) | Yes (corrupts code & terms) | Yes (forced auto-switch) | Depends on script |
| **Keyboard LED + WASAPI knock alert** | **Yes** (Net-zero LED + 24 ms click) | Sound only | No | No |
| **Built-in language profiles** | **20 languages (~61 KB total)** | RU / EN focused | Limited | Manual char tables |
| **`AltGr`, Dead Keys & RTL (`HE`/`AR`)** | **Full support** (`ToUnicodeEx` cache) | Partial | Partial | Fragile |
| **Password field immunity (`UIA`)** | **Automatic (`CurrentIsPassword`)** | Manual exclusions | Heuristic | None |
| **Clipboard preservation (`Ctrl+Alt+Z`)** | **All `HGLOBAL` formats byte-for-byte** | Often clobbers rich formats | Text-only | Clobbers clipboard |
| **RAM / Binary footprint** | **`< 3 MB` RAM / ~285 KB `.exe`** | ~25–40 MB RAM + background services | ~20 MB RAM | Interpreter runtime |
| **Telemetry / Disk logging / Network** | **Zero** (100% RAM-only, MIT Open Source) | Proprietary (Yandex) | Proprietary | Varies |

---

## Key Features

- **Zero Auto-Corruption:** Never alters text or switches layouts without your explicit fix gesture.
- **20 Built-In Language Profiles (~61 KB total):**
  - `EN`, `RU`, `UK`, `BE`, `KK`, `DE`, `FR`, `ES`, `PT`, `IT`, `TR`, `PL`, `NL`, `SV`, `CS`, `EL`, `HE`, `AR`, `HY`, `KA`.
  - Uses physical scan codes + `ToUnicodeEx(..., UNICODE_NO_CHANGED_STATE)` cached across `base`, `shift`, `altgr`, and `shift_altgr` layers per installed Windows `HKL` — no `N×(N-1)` hardcoded character tables.
  - Full support for European `AltGr` diacritics (e.g. Polish `ą, ć, ę, ł, ń, ó, ś, ź, ż`), dead-key accents (`´`, `` ` ``, `^`, `~`, `¨`, `ˇ`), Arabic 2-char `لا` ligatures, top-row letter layouts (`FR` AZERTY, `CS` QWERTZ, `KK`, `HY`), and Georgian unicameral shifted letters.
- **Peripheral LED + Audio Alerts:**
  - **Net-Zero LED Blink:** restores the exact original `Scroll Lock` / `Caps Lock` state after blinking; aborts immediately if a real keystroke arrives mid-blink.
  - **Pre-Warmed WASAPI Click:** 24 ms 48 kHz low-passed acoustic knock with zero disk I/O and no idle sleep wake-lock.
- **One-Gesture Manual Fix, Multi-Word Burst Expansion & Selection Conversion:**
  - **Last Word / Burst Fix:** Double-tap `Left Shift` (configurable to `Double Left Ctrl` or `Pause/Break` within `<= 300 ms`) to convert the last typed word (or consecutive wrong-layout phrase) and switch the active window's layout. Double-tap again within 10 seconds to **undo**.
  - **Selection Conversion (`Ctrl+LeftAlt+Z`):** Converts any highlighted text across layouts while preserving **all** existing clipboard formats byte-for-byte in original order (never conflicts with `AltGr+Z`).
- **Canonical Dark Bento UI & Tray Flyout:**
  - Per-monitor DPI-aware 12-column Dark Bento Settings dashboard (`SmartBlinkSettingsWnd`) with 1-click segmented cards, mechanical keycap selectors, interactive process exclusion chips, and a **Live Hardware Alert Test** button.
- **Immunity First & Privacy by Design:**
  - Automatically pauses in password fields (`UI Automation` `CurrentIsPassword`), active IME composition windows, and blacklisted terminals/IDEs (`WindowsTerminal`, `cmd`, `powershell`, `pwsh`, `code`, `devenv`, etc.).
  - Ignore an alert 3 times and press `Space` or `Enter` to add that word to the RAM-only session whitelist.
  - **100% RAM-Only:** Keystrokes live only in a 256-slot SPSC ring buffer and 256-slot word buffer that are volatile-zeroed on reset and exit. Zero disk logs, zero network calls.
- **Tiny Native Footprint:** Single standalone Rust + Win32 executable (`smartblink.exe`, ~285 KB), `< 3 MB` RAM, `0.0%` idle CPU.

---

## Installation & Usage

1. Run `SmartBlink-Setup-1.0.0.exe` (installs per-user to `%LOCALAPPDATA%\Programs\SmartBlink` by default, no UAC required) or launch standalone `smartblink.exe`.
2. **Tray Icon & Bento UI:**
   - **Left-click** (or double-click) the tray icon — or run `smartblink.exe` a second time — to open the **Dark Bento Settings** window.
   - **Right-click** the tray icon to open the **Bento Tray Flyout** for 1-click control over Alert Mode (`Both`, `Audio Only`, `LED Only`, `Silent`), LED Target (`Scroll Lock`, `Caps Lock`), Fix Hotkey (`2x LShift`, `2x LCtrl`, `Pause`), Startup, and Admin Task.
3. **Default Shortcuts:**
   - **Fix last word / phrase (or Undo last fix):** Double-tap `Left Shift` (`<= 300 ms`)
   - **Convert selected text:** `Ctrl+Alt+Z` (Left Alt)

### Windows SmartScreen & Checksums

Unsigned community builds may display a Windows SmartScreen prompt on first launch (*"More info" → "Run anyway"*). Always verify the binary against `SHA256SUMS.txt` published with each release:

```powershell
Get-FileHash .\smartblink.exe -Algorithm SHA256
```

---

## Frequently Asked Questions (FAQ)

**Q: Does SmartBlink ever replace words automatically while I type?**
No. By architectural invariant #1, SmartBlink never deletes, replaces, or alters your text and never switches your keyboard layout unless you explicitly press your fix gesture (`Double Left Shift`, `Double Left Ctrl`, or `Pause/Break`) or `Ctrl+Alt+Z`.

**Q: Does SmartBlink log keystrokes to disk or send data over the network?**
Never. Keystrokes exist only as physical scan codes in a 256-slot RAM ring buffer and a 256-slot active word buffer, both overwritten via `std::ptr::write_volatile` on reset and exit. SmartBlink links zero networking APIs.

**Q: Why use `Scroll Lock` by default instead of `Caps Lock`?**
Toggling `Caps Lock` while a user is mid-word can invert the case of simultaneously typed letters. `Scroll Lock` is completely decoupled from letter casing on Windows. For laptops without a physical Scroll Lock LED, SmartBlink offers **Caps Lock (Guarded)** mode, which pulses only after an `>= 80 ms` typing pause and aborts immediately if a key is pressed.

---

## Documentation

- [Architecture & Design (`ARCHITECTURE.md`)](ARCHITECTURE.md)
- [Configuration Reference (`docs/CONFIG.md`)](docs/CONFIG.md)
- [Privacy & Security (`docs/PRIVACY.md`)](docs/PRIVACY.md)
- [Building from Source (`docs/BUILDING.md`)](docs/BUILDING.md)
- [Contributing (`CONTRIBUTING.md`)](CONTRIBUTING.md)
- [Language Profile Licenses (`assets/profiles/LICENSES.md`)](assets/profiles/LICENSES.md)

## License

MIT License — Copyright (c) 2026 [VKV Studio](https://vkvstudio.com). See [`LICENSE`](LICENSE).
