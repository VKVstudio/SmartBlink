# Changelog — SmartBlink

## v1.0.0 — 2026-10-25

Initial public release of **SmartBlink** for Windows 10 (1809+) and Windows 11 (x64).

### Highlights
- **Non-Intrusive Wrong-Layout Detection:**
  - Dedicated low-level hook thread (`WH_KEYBOARD_LL` + `WH_MOUSE_LL`) feeding a lock-free 256-slot SPSC ring buffer with zero allocation or blocking inside hook callbacks and explicit synthetic-injection suppression (`SUPPRESS_DEPTH`).
  - Per-`HKL` cached 256-entry `ToUnicodeEx(..., UNICODE_NO_CHANGED_STATE)` 4-layer tables (`base`, `shift`, `altgr`, `shift_altgr`) supporting extended scan codes (`0xE000`), 2-key dead-key composition (`´`, `` ` ``, `^`, `~`, `¨`, `ˇ`), and 2-char ligatures (Arabic `لا`).
  - 20 embedded binary language profiles (`EN, RU, UK, BE, KK, DE, FR, ES, PT, IT, TR, PL, NL, SV, CS, EL, HE, AR, HY, KA`, ~61 KB total) with 3-gate log-odds n-gram + phonotactic scoring (`< 0.1 ms` per word), Turkish-aware case folding (`İ`/`I`), and candidate-aware top-row letter handling (`FR`, `CS`, `KK`, `HY`).
- **Peripheral Alert Channels:**
  - Net-zero keyboard LED blink (`Scroll Lock` safe default or `Caps Lock` guarded `>= 80 ms` gap mode) with instant real-keystroke abort and state restoration.
  - Pre-warmed WASAPI shared-mode 24 ms low-passed "wooden knock" click sample (`waveOut` fallback) with zero idle sleep wake-locks.
- **One-Gesture Manual Fix, Burst Expansion & Selection Conversion:**
  - Double-tap `Left Shift` (or `Double Left Ctrl` / `Pause/Break` within `<= 300 ms`) to replace the last typed word or wrong-layout burst via `VK_BACK` + `KEYEVENTF_UNICODE` and switch the focused window's layout; tap a second time within 10 s to undo.
  - `Ctrl+LeftAlt+Z` selection conversion with byte-for-byte multi-format `HGLOBAL` clipboard snapshot and restoration (bounded to 16 MB/format, 32 MB total; never conflicts with `AltGr+Z`).
- **Immunity & Privacy by Design:**
  - Automatic immunity for password fields (`UI Automation` `CurrentIsPassword`), active IME composition contexts (`ImmGetContext` / `ImmGetOpenStatus`), URL/path/flag tokens, and configurable process blacklist.
  - Stubborn-user session whitelist (`Space` / `Enter` after 3 ignored alerts silences that word+layout pair for the session using RAM-only 64-bit hashes).
  - Volatile zeroing of all keystroke buffers and clipboard snapshots on reset and exit; zero disk logging and zero network calls.
- **Canonical Dark Bento UI, Tray Flyout & Installer:**
  - Per-monitor DPI-aware 12-column Dark Bento Settings window (`SmartBlinkSettingsWnd`), About window (`SmartBlinkAboutWnd`), and Tray Flyout (`SmartBlinkTrayPopupWnd`) with vector-rasterized logo, Live Hardware Alert Test button, 1-click segmented cards, and second-instance activation.
  - Per-user Inno Setup 6 installer (`SmartBlink-Setup-1.0.0.exe`).
