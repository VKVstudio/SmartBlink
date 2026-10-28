# Checkpoint — SmartBlink

`Current Phase`: Public Beta (`v0.1.0-beta`) — Ready to push `master` + `v0.1.0-beta` tag to `https://github.com/VKVstudio/SmartBlink`.
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

- Deep 20-Language / 45-DLL Binary Research complete (`.system/layouts_research.md`):
  - Inspected `KbdLayerDescriptor` (`KBDTABLES`) across all 45 official Windows 11 `C:\Windows\System32\KBD*.DLL` layout drivers for the 20 built-in languages (`EN, RU, UK, BE, KK, DE, FR, ES, PT, IT, TR, PL, NL, SV, CS, EL, HE, AR, HY, KA`).
  - Identified 10 systemic root-cause defects in non-EN/RU layout handling (documented in `.system/layouts_research.md`):
    1. Corpus poverty in `tools/profilegen/src/corpora.rs` (18 non-EN/RU languages have only 2–4 lines / 55–85 words vs ~450 in EN/RU).
    2. Keystroke-length gate (`word_keys.len() < 3` in `worker.rs:736` and `profile.rs:510`) rejecting 2-keystroke / 3–4-char Arabic `WCH_LGTR` ligature words (`بلا`, `لأن`, `إلا`, `ألا`, `ولا`, `كلا`, `هلا`, `علا`).
    3. Bicameral script typo in `src/profile.rs:219` (`matches!(self.script, 1 | 2 | 3 | 6)` includes unicameral `6 = SCRIPT_ARABIC` instead of bicameral `7 = SCRIPT_GREEK`).
    4. Unicameral `Shift` layer traps (`KA-QWERTY` `kbdgeoqw.dll` returns `WCH_NONE` on 19 shifted letter keys; `HE` and `KA-MES` return uppercase ASCII `A..Z` on `Shift+letter`; `AR` returns punctuation/math/combining marks or `\` on `AR-102` `Shift+A`), breaking capitalized word conversion & replay.
    5. Unconditional file-path/symbol immunity (`is_immune_token` / `is_immune_word` on `\`, `/`, `@`, `_`) blocking ordinary letters on `0x2B`, `0x56`, `0x35`, `0x29`, `0x09` (`AR-102` `ذ` on `\`, `UK` `ґ` on `0x56` `\`, `PL-214` `ó/ź` on `\`, `IT` `ù` on `\`, `HY` `պ/բ` on `/` and `շ` on `\`, `KA-MES` `ძ` on `/`, `FR` `_`/`@`, `NL` `@`, `ES` `ç` on `\`, `TR-F` `x` on `\`).
    6. Top-row digit check (`worker.rs:647–660`) requiring `chars().all(|c| c.is_alphabetic())`, which rejects French/Ukrainian/Belarusian contractions (`"c'est"` -> `"c4est"`, `"d'un"` -> `"d4un"`, `"п'ять"`).
    7. Incomplete & language-blind static `combine_dead_key` (~75 hardcoded pairs vs 893 Windows 11 `KBD*.DLL` dead-key pairs; Polish `~`+`a` -> `ą` vs Portuguese `ã`; Greek `΅`+`ι/υ` -> `ΐ/ΰ` bug).
    8. `convert_str_to` (`Ctrl+Alt+Z`) inability to compose/decompose 2-key dead-key sequences (`"qu'e" <-> "qué"`, `";a" <-> "ά"`) or 2-char Arabic ligatures (`"b" <-> "لا"`).
    9. `score_word` internal apostrophe/backtick stripping (`src/profile.rs:305`) masking wrong-layout dead keys (`"qu'e"` -> `"que"`, `"est'a"` -> `"esta"`, `"d`eja"` -> `"deja"`).
    10. `SGCAPS` (`CapsLock` != `Shift`) ignored in `LayoutLayerTable::build` (`CS` `Ě..É/Ú/Ů`, `DE-CH`/`FR-CH` `Ü/Ö/Ä/È/É/À`).

- M13 complete (Deep 20-Layout & 45-KBD Engine Overhaul per `.system/layouts_research.md` §4):
  - `src/layout.rs` & `src/clipboard.rs` & `src/fix.rs`:
    - Implemented `combine_dead_key_for_lang(primary_lang, dead, base)` covering all 17 dead-key accent families from the 45 Windows 11 `KBD*.DLL` `pDeadKey` tables (including Polish `~` -> `ą/ć/ę/ł/ń/ó/ś/ź/ż`, language-aware `´+c` -> `ć` vs `ç`, Greek dialytika+tonos `΅` -> `ΐ/ΰ`, caron `ˇ`, ring `°`, cedilla `¸`, breve `˘`, dot above `˙/·`, ogonek `˛`, double acute `˝`).
    - Added `SGCAPS` (`CapsLock != Shift`) uppercase-to-lowercase fallback in `find_char_ext` (for Czech `Č/Ř/Š/Ž/Ě/Ú/Ů`, Swiss `Ü/Ö/Ä`, Italian `È/Ò/À/Ù/Ì`) and reverse dead-key decomposition (`find_dead_key_sequence`, `can_produce_char`).
    - Added `is_target_script_letter`, `lookup_for_target`, `translation_for_target`, `render_keys_for_target`, and `render_keys_raw` to `LayoutLayerTable`, normalizing `Shift` on unicameral scripts (`KA`, `HE`, `AR`) when `Shift` maps to `None` (`KA-QWERTY`), ASCII Latin (`HE`, `KA-MES`), or punctuation/combining marks (`AR`) while preserving genuine `Shift` letters (`ჭ, ღ, თ, შ, ჟ, ძ, ჩ` in `KA-QWERTY` and `أ, إ, آ, لأ, لإ, لآ` in `AR`).
    - Upgraded `convert_str_to` (`Ctrl+Alt+Z`) to compose/decompose 2-key dead-key sequences (`"qu'e" <-> "qué"`, `"kalhm;era" <-> "καλημέρα"`), 2-char Arabic ligatures (`"b" <-> "لا"`, `"G" <-> "لأ"`), and unicameral<->bicameral `Shift` normalization.
    - Updated `plan_word_fix` and `expand_erroneous_burst_run` (`src/fix.rs`) to use `render_keys_raw` and `render_keys_for_target` so `backspace_count` matches exact committed window characters even when `current_table` dropped a `Shift` key (`KA-QWERTY`) or ended on a pending dead key (`"этих"` on `FR` AZERTY, `"працює"` on `ES`).
  - `src/worker.rs` & `src/profile.rs`:
    - Relaxed keystroke-count gate from `3` to `2` while requiring `>= 3` rendered target alphabetic characters, unlocking 2-keystroke / 3–4-char Arabic `WCH_LGTR` ligature words (`بلا`, `لأن`, `إلا`, `ولا`).
    - Fixed `is_bicameral` (`matches!(self.script, 1 | 2 | 3 | 7)`) so Arabic mid-word `Shift` hamza/madda (`مسألة`, `الآن`, `لأن`) is never penalized as camelCase while Greek (`7`) gets proper bicameral checks.
    - Added vocalization stripping for `LANG_ARABIC` and `LANG_HEBREW`, replaced blind apostrophe/backtick stripping with `strip_valid_internal_apostrophe_or_hyphen` (preserving wrong-layout dead keys like `"qu'e"` and `"est'a"` as `OOV_IDX` in English), allowed internal apostrophes/hyphens in `is_pure_alphabetic_candidate` (`"c'est"`, `"peut-être"`, `"п'ять"`), allowed Czech syllabic `'r'`/`'l'` words (`"čtvrt"`, `"smrt"`) and 5–6-consonant clusters in `KA`/`CS`/`PL`/`DE` (`"გთხოვთ"`, `"čtvrtek"`), and guarded against candidate layouts that drop mid-word keys (`"GitHub"` on `US` vs `KA-QWERTY`).
  - `tools/profilegen` & `assets/profiles/*.bin`:
    - Expanded all 18 non-EN/RU training corpora in `tools/profilegen/src/corpora.rs` to ~350–450 words each, added Dutch trema/accented vowels (`áéèêëïóöü`) and Arabic `ةهءؤئ` vowel carriers in `tools/profilegen/src/main.rs`, and regenerated all 20 `assets/profiles/*.bin` files (`131,356` bytes total; 900–1380 trigrams per language).

## Verified
- `cargo build --release`: 0 warnings (`target\release\smartblink.exe`, `0.1.0-beta`, `682,496` bytes).
- `cargo test --release`: `49 passed; 0 failed` (including `real_45_system_kbd_dll_matrix` which loads all 45 real `C:\Windows\System32\KBD*.DLL` system binaries via `KbdLayerDescriptor` and verifies single-word detection, multi-word `plan_word_fix` replacement, and zero false positives across all 45 Windows 11 keyboard drivers).
- `ISCC.exe installer\SmartBlink.iss`: `target\installer\SmartBlink-Setup-0.1.0-beta.exe` compiled cleanly.
- `README.md` & `README.ru.md`: updated with honest `v0.1.0-beta` Public Beta callout (live-tested `EN <-> RU` vs 45-DLL automated test matrix needing real-world testing), both Option 1 (`cargo build --release`) and Option 2 (prebuilt `smartblink.exe` / `SmartBlink-Setup-0.1.0-beta.exe` in GitHub Releases), zero competitor mentions, and `ig-human` `detect.py` score `78.7 PASS` (`0` em dashes, `0` structural tells).

## Next Immediate Task
- Push `v0.1.0-beta` commit and tag to `https://github.com/VKVstudio/SmartBlink` upon owner confirmation.

## Blockers
- None. (`iscc` for Inno Setup 6 needed only at M10.)

## Junk in repo root (list only, not deleted per rules)
- `scratch_build_m1.log`, `scratch_sdk_install.log`, `scratch/` dir, `assets/icon_16.png`..`icon_48.png` — untracked, not committed.
