# Checkpoint — SmartBlink

`Current Phase`: M4 complete (25/25 unit tests green, release build clean). Ready for M5.
`Approved Stack`: Rust (`x86_64-pc-windows-gnullvm` via LLVM-MinGW UCRT; MSVC/Windows SDK forbidden), `windows` crate 0.58, Win32 tray + Settings window, hand-rolled flat TOML config in `%APPDATA%\SmartBlink\config.toml`, WASAPI shared audio, Inno Setup 6 installer (per-user default `{autopf}`, optional elevated ONLOGON scheduled task). License MIT. Publisher `VKV Studio` / vkvstudio.com. Min OS: Windows 10 1809. Default fix hotkey: double Left Shift. Default alert mode: Both (audio + LED), LED defaults to Scroll Lock.

## Completed
- Phase 0 docs + rename to **SmartBlink** (`ARCHITECTURE.md`, `AGENTS.md`). Repo folder stays `C:\projects\SmartPunto`.
- Toolchain: `stable-x86_64-pc-windows-gnullvm` (rustc 1.99.1), linker & `SMARTBLINK_WINDRES` in `C:\Users\user\.cargo\config.toml`.
- M1 complete (`cef2ab9`): single instance mutex, embedded icon/manifest/version info via `build.rs` + `windres`, config read/write, tray icon + `Exit`, confirmed on desktop by owner.
- M2 complete (`93cc3b4`): `WH_KEYBOARD_LL` + `WH_MOUSE_LL` dedicated hook thread, lock-free 256-slot SPSC ring buffer (volatile-zeroed on pop/reset), foreground `HWND`-cached process immunity, 30 Hz worker thread + 64-slot `WordBuffer` (volatile-zeroed on reset/drop), tray `Pause` / `Resume` toggle.
- M3 complete (`d0e2bc8`):
  - `src/layout.rs`: `active_layouts()` (`GetKeyboardLayoutList`), `window_layout(hwnd)` (`GetWindowThreadProcessId` + `GetKeyboardLayout`), `reconstruct_scan_code` (`0xE000` prefix when `LLKHF_EXTENDED` set), `LayoutLayerTable` & `LayoutCache` (lazy 256-entry `base` + `shift` layer cache per `HKL` using `MapVirtualKeyExW` + `ToUnicodeEx` with `UNICODE_NO_CHANGED_STATE = 0x0004`), `active_word_slice` & `last_word_run_for_fix` word segmentation, `is_immune_token` URL/path/flag filter.
  - `src/worker.rs`: `KeyStrokeSlot` records `shift` state; worker initializes `LayoutCache` for active `HKL`s.
- M4 complete:
  - `tools/profilegen`: standalone zero-dependency offline generator producing compact `SMBP` v1 binary profiles (`assets/profiles/en.bin` 5755 B, `assets/profiles/ru.bin` 5891 B) + `assets/profiles/LICENSES.md`.
  - `src/profile.rs`: embedded `en.bin` & `ru.bin` parser, binary-search bigram (`u16 -> i8`) & trigram (`u32 -> i8`) log-odds scorer + vowel/consonant/OOV phonotactic penalties, `evaluate_tables` & `evaluate_active_word` with 3-gate decision rule (`current_score <= CURRENT_MAX_PLAUSIBLE_SCORE`, `best_score >= ABSOLUTE_THRESHOLD`, `margin >= MARGIN_THRESHOLD`).
  - `src/worker.rs`: evaluates active word on keystrokes and updates `last_verdict`.

## Verified
- `cargo build --release`: 0 warnings, produces `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `cargo test --release`: `test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s` (5 config + 2 ring + 2 immunity + 4 worker + 1 hooks + 1 tray + 5 layout + 5 profile/scorer tests incl. `Ghbdtn`/`Привет`, `Cjj,otybt`/`Сообщение`, `Руддщ`/`Hello`, bilingual homograph `"here"`/`"руку"` false-positive immunity, and `<100 us/word` benchmark).

## Next Immediate Task
M5 — Alert channels: LED blink (Scroll Lock default, Caps opt-in with gap-only safety, net-zero state restore, disabled in password fields) + WASAPI shared-mode low-frequency click sample (`ARCHITECTURE.md` §3.4, §6).

## Blockers
- None. (`iscc` for Inno Setup 6 needed only at M10.)

## Junk in repo root (list only, not deleted per rules)
- `scratch_build_m1.log`, `scratch_sdk_install.log`, `scratch/` dir, `assets/icon_16.png`..`icon_48.png` — untracked, not committed.
