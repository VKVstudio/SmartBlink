# SmartBlink

**Wrong-layout typing detection and one-gesture manual fix for Windows 10/11.**

*[Русская документация (README.ru.md)](README.ru.md)*

Most touch/semi-touch typists look at the **keyboard**, not the monitor, and notice they typed in the wrong keyboard layout (`Ghbdtn` instead of `Привет`, or `Руддщ` instead of `Hello`) only after several words. Auto-switchers (like Punto Switcher) silently corrupt code, passwords, abbreviations, and bilingual terms.

**SmartBlink** solves this cleanly:
1. **Alerts you where your eyes are:** blinks the keyboard LED (`Scroll Lock` by default, or guarded `Caps Lock`) and plays a subtle 24 ms low-passed "wooden knock" click via WASAPI.
2. **Never mutates your text automatically:** only when **you** double-tap `Left Shift` (or press `Ctrl+Alt+Z` on a selection) does SmartBlink replace the word and switch the window's keyboard layout.

---

## Key Features

- **Zero Auto-Corruption:** Never alters text or switches layouts without your explicit fix gesture.
- **20 Built-In Language Profiles (~53 KB total):**
  - `EN`, `RU`, `UK`, `BE`, `KK`, `DE`, `FR`, `ES`, `PT`, `IT`, `TR`, `PL`, `NL`, `SV`, `CS`, `EL`, `HE`, `AR`, `HY`, `KA`.
  - Uses physical scan codes + `ToUnicodeEx(..., UNICODE_NO_CHANGED_STATE)` cached per installed Windows `HKL` — no `N×(N-1)` hardcoded character tables.
- **Peripheral LED + Audio Alerts:**
  - **Net-Zero LED Blink:** restores the exact original `Scroll Lock` / `Caps Lock` state after blinking; aborts immediately if a real keystroke arrives mid-blink.
  - **Pre-Warmed WASAPI Click:** 24 ms 48 kHz low-passed acoustic knock with zero disk I/O and no idle sleep wake-lock.
- **One-Gesture Manual Fix & Selection Conversion:**
  - **Last Word Fix:** Double-tap `Left Shift` (configurable to `Double Left Ctrl` or `Pause/Break` within `<= 300 ms`) to convert the last typed word and switch the active window's layout.
  - **Selection Conversion (`Ctrl+Alt+Z`):** Converts any highlighted text across layouts while preserving **all** existing clipboard formats byte-for-byte in original order.
- **Immunity First & Privacy by Design:**
  - Automatically pauses in password fields (`UI Automation` `CurrentIsPassword`), active IME composition windows, and blacklisted terminals/IDEs (`WindowsTerminal`, `cmd`, `powershell`, `pwsh`, `code`, `devenv`, etc.).
  - Ignore an alert and press `Space` or `Enter` to add that word to the RAM-only session whitelist.
  - **100% RAM-Only:** Keystrokes live only in a 256-slot SPSC ring buffer and 64-slot word buffer that are volatile-zeroed on reset and exit. Zero disk logs, zero network calls.
- **Tiny Native Footprint:** Single standalone Rust + Win32 executable (`smartblink.exe`, ~260 KB), `< 3 MB` RAM, `0.0%` idle CPU.

---

## Installation & Usage

1. Run `SmartBlink-Setup-1.0.0.exe` (installs per-user to `%LOCALAPPDATA%\Programs\SmartBlink` by default, no UAC required) or launch standalone `smartblink.exe`.
2. **Tray Icon:**
   - **Double-click** the tray icon (or choose **Settings...**) to open the Settings window.
   - **Right-click** the tray icon to switch Alert Mode (`Both`, `Audio Only`, `LED Only`, `Silent`), LED Channel (`Scroll Lock`, `Caps Lock Guarded`), Fix Hotkey, exclude the current application, or enable Windows startup.
3. **Default Shortcuts:**
   - **Fix last word:** Double-tap `Left Shift` (`<= 300 ms`)
   - **Convert selected text:** `Ctrl+Alt+Z`

### Windows SmartScreen & Checksums

Unsigned community builds may display a Windows SmartScreen prompt on first launch (*"More info" → "Run anyway"*). Always verify the binary against `SHA256SUMS.txt` published with each release:

```powershell
Get-FileHash .\smartblink.exe -Algorithm SHA256
```

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
