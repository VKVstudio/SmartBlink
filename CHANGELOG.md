# Changelog — SmartBlink

## v1.0.0 — 2026-10-24

Initial public release of **SmartBlink** for Windows 10 (1809+) and Windows 11 (x64).

### Highlights
- **Non-Intrusive Wrong-Layout Detection:**
  - Dedicated low-level hook thread (`WH_KEYBOARD_LL` + `WH_MOUSE_LL`) feeding a lock-free 256-slot SPSC ring buffer with zero allocation or blocking inside hook callbacks.
  - Per-`HKL` cached 256-entry `ToUnicodeEx(..., UNICODE_NO_CHANGED_STATE)` layer tables supporting extended scan codes (`0xE000`) and dead-key awareness.
  - 20 embedded binary language profiles (`EN, RU, UK, BE, KK, DE, FR, ES, PT, IT, TR, PL, NL, SV, CS, EL, HE, AR, HY, KA`, ~53 KB total) with 3-gate log-odds n-gram + phonotactic scoring (`< 0.1 ms` per word).
- **Peripheral Alert Channels:**
  - Net-zero keyboard LED blink (`Scroll Lock` safe default or `Caps Lock` guarded `>= 80 ms` gap mode) with instant real-keystroke abort and state restoration.
  - Pre-warmed WASAPI shared-mode 24 ms low-passed "wooden knock" click sample (`waveOut` fallback) with zero idle sleep wake-locks.
- **One-Gesture Manual Fix & Selection Conversion:**
  - Double-tap `Left Shift` (or `Double Left Ctrl` / `Pause/Break` within `<= 300 ms`) to replace the last typed word via `VK_BACK` + `KEYEVENTF_UNICODE` and switch the focused window's layout.
  - `Ctrl+Alt+Z` selection conversion with byte-for-byte multi-format `HGLOBAL` clipboard snapshot and restoration.
- **Immunity & Privacy by Design:**
  - Automatic immunity for password fields (`UI Automation` `CurrentIsPassword`), active IME composition contexts (`ImmGetContext` / `ImmGetOpenStatus`), URL/path/flag tokens, and configurable process blacklist.
  - Stubborn-user session whitelist (`Space` / `Enter` after an alert silences that word+layout pair for the session using RAM-only 64-bit hashes).
  - Volatile zeroing of all keystroke buffers and clipboard snapshots on reset and exit; zero disk logging and zero network calls.
- **Native Win32 Tray, Settings UI & Installer:**
  - Full tray context menu, native Win32 Settings window (`SmartBlinkSettingsWnd`), `HKCU\...\Run` autostart, and optional elevated `ONLOGON` scheduled task for working above Administrator windows (UIPI).
  - Per-user Inno Setup 6 installer (`SmartBlink-Setup-1.0.0.exe`).
