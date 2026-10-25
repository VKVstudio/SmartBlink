# Checkpoint — SmartBlink

`Current Phase`: v1.0.0 Production-Ready — Multi-Persona (8 Roles) + 20-Layout Skeptic Audit, Security/Layout/UX Hardening, & GitHub Release Preparation complete (46/46 unit tests green, release binary & Inno Setup 6 installer rebuilt, `SHA256SUMS.txt` updated).
`Approved Stack`: Rust (`x86_64-pc-windows-gnullvm` via LLVM-MinGW UCRT; MSVC/Windows SDK forbidden), `windows` crate 0.58, Win32 tray + Settings window, hand-rolled flat TOML config in `%APPDATA%\SmartBlink\config.toml`, WASAPI shared audio, Inno Setup 6 installer (per-user default `{autopf}`, optional elevated ONLOGON scheduled task). License MIT. Publisher `VKV Studio` / vkvstudio.com. Min OS: Windows 10 1809. Default fix hotkey: double Left Shift. Default alert mode: Both (audio + LED), LED defaults to Scroll Lock.

## Completed
- Phase 0 docs + rename to **SmartBlink** (`ARCHITECTURE.md`, `AGENTS.md`). Canonical repo folder: `C:\projects\SmartBlink`.
- Toolchain: `stable-x86_64-pc-windows-gnullvm` (rustc 1.99.1), linker & `SMARTBLINK_WINDRES` in `C:\Users\user\.cargo\config.toml`.
- M1 complete (`cef2ab9`): single instance mutex, embedded icon/manifest/version info via `build.rs` + `windres`, config read/write, tray icon + `Exit`, confirmed on desktop by owner.
- M2 complete (`93cc3b4`): `WH_KEYBOARD_LL` + `WH_MOUSE_LL` dedicated hook thread, lock-free 256-slot SPSC ring buffer (volatile-zeroed on pop/reset), foreground `HWND`-cached process immunity, 30 Hz worker thread + 64-slot `WordBuffer` (volatile-zeroed on reset/drop), tray `Pause` / `Resume` toggle.
- M3 complete (`d0e2bc8`):
  - `src/layout.rs`: `active_layouts()`, `window_layout(hwnd)`, `reconstruct_scan_code`, `LayoutLayerTable` & `LayoutCache`, `active_word_slice` & `last_word_run_for_fix`, `is_immune_token`.
- M4 complete (`6c39363`):
  - `tools/profilegen`, `assets/profiles/en.bin` & `ru.bin`, `src/profile.rs` scoring engine with 3-gate decision rule.
- M5 complete (`f54cbfd`):
  - `src/signal.rs`: WASAPI shared-mode click sample (`waveOut` fallback) + net-zero LED blink (`ScrollLock` default, `CapsLockGuarded` opt-in) + UI Automation `CurrentIsPassword` immunity.
- M6 complete (`bf592e5`):
  - `src/fix.rs`: `FixGestureDetector` (`DoubleLeftShift` default, `DoubleLeftCtrl`, `PauseBreak` within `<= 300 ms`), `plan_word_fix` & `plan_word_fix_cached`, `build_fix_inputs` (`VK_BACK` + `KEYEVENTF_UNICODE`), `execute_fix` inside `hooks::begin_suppression()` / `end_suppression()`, and target window layout switch (`ActivateKeyboardLayout` + `PostMessageW(WM_INPUTLANGCHANGEREQUEST)`).
- M7 complete (`4ab0c6c`):
  - `tools/profilegen/src/corpora.rs` & `tools/profilegen/src/main.rs`: generated all 20 built-in language profiles under `assets/profiles/*.bin` (`54,191` bytes total).
  - `src/profile.rs`: embedded all 20 profile blobs (`all_profiles()`) + `intra_latin_and_multi_script_conflict_suite_de_fr_tr_pl_uk`.
- M8 complete (`c3e3461`):
  - `src/clipboard.rs`: `SessionWhitelist`, `ClipboardSnapshot` & `ClipboardRestoreGuard`, `SelectionHotkeyDetector` (`Ctrl+Alt+Z`), `convert_selection_text` & `convert_selection_in_window`.
  - `src/immunity.rs` & `src/layout.rs`: added `is_hwnd_ime_active` and reverse character lookup (`find_char`, `char_at`, `convert_str_to`).
- M9 complete (`b0b4bb3`):
  - `src/autostart.rs`: `HKCU\...\Run\SmartBlink` registration + opt-in elevated scheduled task (`ONLOGON`, `RL HIGHEST`) and `TokenElevation` check.
  - `src/settings.rs`: pure Win32 Settings window (`SmartBlinkSettingsWnd`).
  - `src/tray.rs` & `src/config.rs` & `src/worker.rs`: full tray popup menu + live atomic config reload.
- M10 complete (`4de994e`):
  - `installer/SmartBlink.iss`: Inno Setup 6 installer script compiled via `ISCC.exe` -> `target\installer\SmartBlink-Setup-1.0.0.exe`.
- M11 complete (`4a568f4`):
  - `.github/workflows/release.yml`, `LICENSE`, `README.md`, `README.ru.md`, `CONTRIBUTING.md`, `docs/CONFIG.md`, `docs/PRIVACY.md`, `docs/BUILDING.md`.
- M12 complete:
  - Bumped version to `1.0.0` in `Cargo.toml` and `assets/version.rc`, added `CHANGELOG.md`, rebuilt `smartblink.exe` (`1.0.0.0`) and `target\installer\SmartBlink-Setup-1.0.0.exe`, and generated `target\installer\SHA256SUMS.txt`.
- Multi-Persona (8 Roles) & 20-Layout Skeptic Audit + GitHub Release Prep complete:
  - Audited codebase with 10 `flash` subagents covering 8 roles (Security Auditor, Input-Utility Systems Programmer, Power User, Novice User, Legal/Privacy Auditor, CEO, Marketer, SEO/GEO Optimizer) and all 20 built-in layouts (`EN, RU, UK, BE, KK, DE, FR, ES, PT, IT, TR, PL, NL, SV, CS, EL, HE, AR, HY, KA`).
  - `tools/profilegen` & `assets/profiles/*.bin` (`61,766` bytes total): fixed Hebrew vowel mask (`ו`/`י` instead of Arabic `و`/`ي`), added Turkish case folding (`İ` -> `i`, `I` -> `ı`, strip `U+0307`), cleaned foreign-script tokens from `DE`/`FR`/`TR`/`EL`/`SV`/`AR`/`BE` corpora, and regenerated all 20 binary profiles.
  - `src/worker.rs`, `src/layout.rs`, `src/profile.rs`, `src/fix.rs`: added 4-layer `ToUnicodeEx` caching (`base`, `shift`, `altgr`, `shift_altgr`), `AltGr` (`VK_LCONTROL`+`VK_RMENU`) word buffer preservation, 2-key dead-key composition (`combine_dead_key`), Arabic 2-char `لا` ligature support (`ScanTranslation::Chars`), candidate-aware top-row digit/slash immunity (`FR` `"d2j0"`->`"déjà"`, `CS` `"4e3tina"`->`"čeština"`, `KK` `"c2ktv"`->`"сәлем"`, `HY` `"ta8i"`->`"տարի"`, `HE` `"/וקדאןםמ"`->`"question"`, `IT` `"pi\\"`->`"più"`), Georgian unicameral Shift-letter handling (`"dRes"`->`"დღეს"`), active idle timeout, and 10-second reversible fix (undo on second fix gesture).
  - Security & UX hardening (`src/hooks.rs`, `src/clipboard.rs`, `src/config.rs`, `src/autostart.rs`, `src/settings.rs`, `src/tray.rs`, `src/main.rs`): wired `SUPPRESS_DEPTH` into `low_level_keyboard_proc`, distinguished `Left Alt` from `AltGr` (`VK_RMENU`) in `SelectionHotkeyDetector` so `AltGr+Z` (Polish `ż`) never triggers selection conversion, added `16 MB`/`32 MB` clipboard snapshot caps and volatile RAM zeroing, capped `config.toml` at `64 KB` / `256` blacklist entries with PID-qualified temp file, used explicit `%SystemRoot%\System32\schtasks.exe` & `notepad.exe` paths, fixed `Sel: Ctrl+Alt+Z` hero badge clipping in `src/settings.rs`, and added second-instance `WM_APP_OPEN_SETTINGS` activation.
  - Embedded `SmartBlink_preview.png`, comparison table, and FAQ in `README.md` & `README.ru.md`, updated `Cargo.toml`, `CHANGELOG.md`, `installer/SmartBlink.iss`, `.github/workflows/release.yml`, and `.gitignore`.

## Verified
- `cargo build --release`: 0 warnings, `target\x86_64-pc-windows-gnullvm\release\smartblink.exe` (`612,352` bytes, `SHA256 = 5866e2d5ce306855b5cc3a2ff4931b8d5c430dff1544a9f6921e564e378430f6`).
- `cargo test --release`: `test result: ok. 46 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.40s`.
- `ISCC.exe installer\SmartBlink.iss`: `Successful compile` -> `target\installer\SmartBlink-Setup-1.0.0.exe` (`2,327,059` bytes, `SHA256 = acfbb1a7d3230ad6d9e298951b2a9cd3be77b834004a65280e9573050d05ca76`).
- `target\installer\SHA256SUMS.txt` updated for both `smartblink.exe` and `SmartBlink-Setup-1.0.0.exe`.

## Next Immediate Task
- Await owner's explicit approval/command if and when to push to a GitHub remote or publish a GitHub release (`AGENTS.md` Session Protocol #7).

## Blockers
- None. (`iscc` for Inno Setup 6 needed only at M10.)

## Junk in repo root (list only, not deleted per rules)
- `scratch_build_m1.log`, `scratch_sdk_install.log`, `scratch/` dir, `assets/icon_16.png`..`icon_48.png` — untracked, not committed.
