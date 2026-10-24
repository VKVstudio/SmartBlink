# Checkpoint — SmartBlink

`Current Phase`: M12 + live audit bugfix pass + Canonical Dark Bento UI & Brand Assets complete (`v1.0.0`, 43/43 unit tests green, release binary & Inno Setup 6 installer rebuilt, `SHA256SUMS.txt` updated).
`Approved Stack`: Rust (`x86_64-pc-windows-gnullvm` via LLVM-MinGW UCRT; MSVC/Windows SDK forbidden), `windows` crate 0.58, Win32 tray + Settings window, hand-rolled flat TOML config in `%APPDATA%\SmartBlink\config.toml`, WASAPI shared audio, Inno Setup 6 installer (per-user default `{autopf}`, optional elevated ONLOGON scheduled task). License MIT. Publisher `VKV Studio` / vkvstudio.com. Min OS: Windows 10 1809. Default fix hotkey: double Left Shift. Default alert mode: Both (audio + LED), LED defaults to Scroll Lock.

## Completed
- Phase 0 docs + rename to **SmartBlink** (`ARCHITECTURE.md`, `AGENTS.md`). Repo folder stays `C:\projects\SmartPunto`.
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

## Verified
- `cargo build --release`: 0 warnings, `target\x86_64-pc-windows-gnullvm\release\smartblink.exe` (`ProductVersion == 1.0.0.0`).
- `cargo test --release`: `test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.59s`.
- `ISCC.exe installer\SmartBlink.iss`: `Successful compile` -> `target\installer\SmartBlink-Setup-1.0.0.exe`.
- `target\installer\SHA256SUMS.txt` generated for both `smartblink.exe` and `SmartBlink-Setup-1.0.0.exe`.

## Next Immediate Task
**PRIORITY TASK (for next session):**
1. **Multi-Persona & Per-Layout/Country Skeptic Audit (using `flash` subagents):**
   - Launch a full panel of `flash` subagents covering:
     - **Role/Persona Skeptics (8+):**
       1. **Top-Tier Security & Threat-Model Auditor:** audit `WH_KEYBOARD_LL` / `WH_MOUSE_LL`, RAM zeroing (`ptr::write_volatile`), UIPI / privilege boundaries, scheduled task (`schtasks`), registry autostart, clipboard snapshot/restore, config parser (`%APPDATA%\SmartBlink\config.toml`), and supply-chain/binary surface for any memory leak, keystroke leak, race condition, injection, or crash vector.
       2. **Competitor / Input-Utility Systems Programmer (Punto Switcher / Caramba / KeySwitcher veteran):** audit scan-code mapping, `ToUnicodeEx` dead-key/AltGr states, multi-word burst boundaries, homograph bridging, backspace/cursor tracking, and terminal/IDE/game edge cases.
       3. **Everyday Power User:** audit typing feel, false positives/negatives, hotkey responsiveness, audio/LED alert ergonomics, and clipboard selection conversion (`Ctrl+Alt+Z`).
       4. **Non-Technical / "Grandmother" Novice User:** audit foolproof onboarding, accidental key mashing, clear recovery when confused, and zero-surprise behavior.
       5. **Legal & Privacy Compliance Auditor (GDPR / CCPA / MIT / Windows Store & SmartScreen):** verify zero telemetry, license headers, third-party notices, privacy guarantees, and installer disclosures.
       6. **Product Director / CEO:** audit product positioning, reliability invariants, installer polish, and release readiness.
       7. **Marketer:** audit value proposition, README/landing messaging, screenshots/diagrams, and user conversion flow.
       8. **SEO / GEO Optimizer:** audit GitHub repository metadata, `README.md` / `README.ru.md`, structured keywords, AI-search (ChatGPT/Perplexity/Gemini/Google) discoverability, and release notes.
     - **Per-Layout & Country/Culture Skeptics (all 20 built-in layouts/countries on `flash`):**
       - Audit `EN`, `RU`, `UK`, `BE`, `KK`, `DE` (QWERTZ + dead keys/Umlauts), `FR` (AZERTY + number row Shift + dead keys), `ES`, `PT`, `IT`, `TR` (dotted/dotless `i`/`ı`), `PL` (`AltGr` / right-Alt diacritics!), `NL`, `SV`, `CS` (háček/čárka dead keys), `EL` (Greek `;` question mark + tonos), `HE` (RTL + final letter forms), `AR` (RTL + connected forms), `HY` (Armenian punctuation), `KA` (Georgian Shift layers).
       - Check country-specific keyboard habits (`AltGr` = `Ctrl+Alt` on European keyboards vs `WordBuffer` chord reset!), national punctuation keys, and whether per-country/layout UX or hotkey/button options are needed.
2. **Fix & Verify:**
   - Implement fixes for all discovered vulnerabilities, layout/AltGr/dead-key/RTL bugs, UX gaps, and docs/SEO/GEO improvements.
   - Verify with `cargo test --release`, `cargo build --release`, and `ISCC.exe installer\SmartBlink.iss`.
3. **Prepare for GitHub Upload:**
   - Embed and showcase `SmartBlink_preview.png` (located at `C:\projects\SmartPunto\SmartBlink_preview.png`, showing the Dark Bento Settings window + Tray Flyout) in `README.md` and `README.ru.md` (and fix the minor text clip on `Selection: Ctrl+Alt+Z` in the top-left hero badge of `src/settings.rs`).
   - Ensure working tree, commits, `README.md`, `README.ru.md`, `CHANGELOG.md`, `LICENSE`, CI workflows, and `SHA256SUMS.txt` are 100% clean and ready for GitHub push/release (do NOT push without explicit owner command).

## Blockers
- None. (`iscc` for Inno Setup 6 needed only at M10.)

## Junk in repo root (list only, not deleted per rules)
- `scratch_build_m1.log`, `scratch_sdk_install.log`, `scratch/` dir, `assets/icon_16.png`..`icon_48.png` — untracked, not committed.
