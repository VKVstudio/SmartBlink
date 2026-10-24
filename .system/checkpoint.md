# Checkpoint — SmartBlink

`Current Phase`: M7 complete (32/32 unit tests green, release build clean). Ready for M8.
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
- M7 complete:
  - `tools/profilegen/src/corpora.rs` & `tools/profilegen/src/main.rs`: generated all 20 built-in language profiles under `assets/profiles/*.bin` (`en, ru, uk, be, kk, de, fr, es, pt, it, tr, pl, nl, sv, cs, el, he, ar, hy, ka`), total size `54,191` bytes (~53 KB).
  - `src/profile.rs`: embedded all 20 profile blobs (`all_profiles()`) and added `intra_latin_and_multi_script_conflict_suite_de_fr_tr_pl_uk` verifying zero false positives on shared Latin vocabulary and positive detection on physical layout conflicts (QWERTZ `y`/`z`, AZERTY `a`/`q`/`z`/`w`).

## Verified
- `cargo build --release`: 0 warnings, produces `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `cargo test --release`: `test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.82s`.

## Next Immediate Task
M8 — Selection conversion (`Ctrl+Alt+Z`) + byte-for-byte clipboard save/restore + stubborn-user session whitelist (`ARCHITECTURE.md` §3.6, §3.7, §6).

## Blockers
- None. (`iscc` for Inno Setup 6 needed only at M10.)

## Junk in repo root (list only, not deleted per rules)
- `scratch_build_m1.log`, `scratch_sdk_install.log`, `scratch/` dir, `assets/icon_16.png`..`icon_48.png` — untracked, not committed.
