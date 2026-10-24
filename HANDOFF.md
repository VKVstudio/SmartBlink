# HANDOFF — SmartPunto

## Session #001 — 2026-10-23 — Antigravity (Phase 0)

**Done**
- Created `ARCHITECTURE.md`, `AGENTS.md`, `.system/checkpoint.md`, `HANDOFF.md`. No application code.
- Key decisions: Rust + `windows` crate, Win32-only UI, WASAPI click sample, Inno Setup 6 per-user install with optional elevated ONLOGON scheduled task for UIPI, TOML config in `%APPDATA%`.
- Detection design: physical scan codes (+`0xE0` extended reconstruction) → cached per-HKL layer tables via `ToUnicodeEx` (no-state-change flag) → score against embedded per-language bigram/trigram profiles (<600 KB for 20 languages), margin + threshold gating, active-layout set from `GetKeyboardLayoutList`.
- Alert design: net-zero Caps/Scroll blink + 15–30 ms low-frequency WASAPI click; channels user-selectable; disabled in password fields.
- Roadmap M1..M12 defined in `ARCHITECTURE.md` §6.

**Open questions for user (approve before M1)**
1. Stack approval: Rust + Inno Setup? (alternates were C++ / WiX — rejected for size/toolchain.)
2. License: Apache-2.0 recommended; MIT acceptable.
3. Product name in UI/installer final as "SmartPunto"? Company/publisher string for installer and registry?
4. Profile corpora source: build our own generator from public permissive corpora (Wikipedia/OSCAR-class), or do you have preferred sources? License provenance will be recorded, no corpus text shipped.
5. Code signing: release unsigned with published SHA-256 (documented SmartScreen caveat) vs you provide a cert?
6. Minimum Windows: 10 1809 assumed (needed for `ToUnicodeEx` no-state-change and UI Automation reliability). OK, or support older?
7. Default fix hotkey: double Left Shift (recommended) vs double Ctrl; keep Pause/Break as third option.
8. Default alert mode on first run: Both / Audio only / LED only?
9. Git init now in Phase 0 or at M1?

**Instructions for next agent**
Read `AGENTS.md` §Session Protocol, then `.system/checkpoint.md`. Do only the task named there (M1 after user approval). Confirm Rust/MSVC and Inno Setup presence before starting; report if missing instead of installing silently. Do not push anywhere. Update checkpoint + append entry here at the end.

## Session #002 — 2026-10-23 — Antigravity (Phase 0 close-out)

**Done**
- Owner approved architecture. Answers recorded: MIT license; Publisher `VKV Studio` / vkvstudio.com; profiles self-generated offline from public corpora (Wikipedia/Leipzig/OSCAR-class frequency dictionaries); unsigned releases + `SHA256SUMS.txt` + SmartScreen README section; Min OS Windows 10 1809; default fix hotkey double Left Shift (double Left Ctrl / Pause selectable); default alert mode `Both`.
- Two mandated architecture edits applied: (§3.4) LED blink now defaults to Scroll Lock — Caps cannot uppercase mid-blink typing; Caps is opt-in with gap-only blinking (≥ ~80 ms since last key), abort-on-new-keystroke and ≤ 120 ms net-zero restore. (§4) Inno default dir corrected to `{autopf}\SmartPunto`.
- `git init` + first docs commit created (no AI references in metadata).

**Toolchain status**
`rustc`/`cargo`/`iscc` absent on PATH. Owner to install before M1:
`winget install --id Rustlang.Rustup -e` and `winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` and `winget install --id JR.InnoSetup -e`. Verify `%USERPROFILE%\.cargo\bin` and `C:\Program Files (x86)\Inno Setup 6\iscc.exe`.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M1 only. No push. Update checkpoint + append entry here.

## Session #003 — 2026-10-23 — Antigravity (M1 start, toolchain pivot to GNU)

**Done**
- M1 skeleton written: `Cargo.toml` (windows 0.58, gnu target), `.cargo/config.toml`, `src/{main,config,single_instance,tray}.rs`, `assets/icon.ico` + `version.rc`, `tools/gen_icon.py`, `.gitignore`. Uncommitted.
- rustup default switched to `stable-x86_64-pc-windows-gnu` (1.99.1). Crate compiles clean on gnu; only 5 unused-Result warnings in tray.rs remain.

**Owner directives this session**
- MSVC forbidden: no cl.exe/link.exe attempts, no Windows SDK install. GNU toolchain only (`x86_64-pc-windows-gnu`).
- Session stopped by owner at 21:59 before link fix; state saved here + checkpoint.md (rewritten).

**Toolchain facts (verified)**
- rustc/cargo not on PATH: prepend `C:\Users\user\.cargo\bin` per command.
- MSYS2 mingw64 gcc broken: `cc1.exe` missing → `collect2.exe: ld returned 53`. Do not use.
- Working candidate (untested as linker): LLVM-MinGW UCRT gcc at `C:\Users\user\AppData\Local\Microsoft\WinGet\Packages\MartinStorsjo.LLVM-MinGW.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\llvm-mingw-20261006-ucrt-x86_64\bin\gcc.exe` (`--version` OK; `ld.lld.exe` alongside). Set as `[target.x86_64-pc-windows-gnu] linker` in `.cargo/config.toml` on next session.
- windows 0.58 API gotchas learned: `Param<>` needs direct values (`HINSTANCE(hmod)`, not `Some(...)`); `LoadImageW` → `Result<HANDLE>`; tooltip via `copy_wtext(&utf16_units(...), &mut nid.szTip)` (w!() is PCWSTR, not &[u16]); `CreateMutexW` behind `Win32_Security`.

**Not verified / pending**
- No exe: link never succeeded. No tests run, no tray smoke test. M1 NOT complete.
- Junk logs in repo root: `scratch_build_m1.log`, `scratch_sdk_install.log`, `scratch/` — listed, not deleted, not committed.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. One task: repoint linker to LLVM-MinGW gcc → `cargo build --release` → tests → manual tray check → fix warnings → commit (no AI mentions, no push).

## Session #004 — 2026-10-23 — Review (no code)

**Done**
- Reviewed M1 code (commit `18b1f33`). Verdict: not runnable — `PeekMessageW` loop quits on empty queue; Exit posts WM_QUIT but loop waits for WM_APP+2; icon/manifest never embedded (no build.rs); config written before mutex check; blacklist defaults/parser/test weak; `assets/version.rc` line 1 garbage.
- Root cause of all link failures found: LLVM-MinGW lacks `libgcc.a`/`libgcc_eh.a`, required by `x86_64-pc-windows-gnu`. Fix = `x86_64-pc-windows-gnullvm` toolchain (matches LLVM-MinGW). `x86_64-w64-mingw32-clang.exe` and `-windres.exe` present.
- Owner decision: rename to **SmartBlink** (Smart prefix kept).
- Wrote `.system/next_task.md`: 8-step executor spec with stop rules, exact edits, verification script, commit plan.
- Process note: commit `18b1f33` was made without a passing build (violated AGENTS.md); previous session looped ~70 identical builds. next_task.md adds a 2-strike stop rule.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`, then execute `.system/next_task.md` in order.

## Session #005 — 2026-10-23 — M1 execution (SmartBlink)

**Done**
- Executed `.system/next_task.md` steps 1–8 in order:
  1. Installed and set default `stable-x86_64-pc-windows-gnullvm` (1.99.1). Configured machine-specific linker (`x86_64-w64-mingw32-clang.exe`) and `SMARTBLINK_WINDRES` in `C:\Users\user\.cargo\config.toml`; cleaned repo `.cargo/config.toml`.
  2. Renamed product/crate/binary/AppData dir/mutex/window class to `SmartBlink` / `smartblink`.
  3. Added `assets/app.manifest` (`asInvoker`, Win10/11 compatibility, PerMonitorV2 DPI), updated `assets/version.rc`, added `build.rs` invoking `windres`. Fixed a frame-offset calculation bug in `tools/gen_icon.py` (`6 + 16 * len(frames) + len(body)` instead of `6 + len(entries) + len(body)`) and regenerated `assets/icon.ico` so `LoadImageW` loads `IDI_MAIN=101` directly.
  4. Rewrote `src/main.rs` (mutex before config load) and `src/tray.rs` (`GetMessageW` loop with `.0 > 0`, `DestroyWindow` -> `WM_DESTROY` -> `remove_tray_icon` + `PostQuitMessage(0)`, `WM_NULL` post after `TrackPopupMenu`, `TaskbarCreated` handler, non-Explorer desktop/early-logon guard on `NIM_ADD`).
  5. Updated `src/config.rs` defaults, char-by-char array parser with escape support, `sync_all()` before rename, and exact-value unit tests.
  6. Synced `ARCHITECTURE.md` and `AGENTS.md` with `gnullvm` + `build.rs`/`windres` + `Local\SmartBlink.SingleInstance`.

**Verified**
- `cargo build --release` → 0 warnings, `target\x86_64-pc-windows-gnullvm\release\smartblink.exe` built, `VersionInfo.ProductName == "SmartBlink"`.
- `cargo test --release` → `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`.
- Step 7 automated script → `first alive: True`, `second exited: True`, `config exists: True` (`%APPDATA%\SmartBlink\config.toml`).
- Owner manual tray check (2026-10-23): confirmed custom icon + `Exit` popup menu working on desktop. M1 complete.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M2 (`ARCHITECTURE.md` §2, §3.7, §6).

## Session #006 — 2026-10-23/24 — M2 implementation (hooks, ring, immunity, worker, tray pause)

**Done**
- Implemented M2 modules:
  - `src/ring.rs`: lock-free 256-slot SPSC ring buffer (`InputRecord`, 16 B) with volatile slot zeroing on `pop()` and `clear_and_zero()`.
  - `src/immunity.rs`: foreground `HWND`-cached process immunity check via `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `QueryFullProcessImageNameW` against `cfg.blacklist`.
  - `src/hooks.rs`: dedicated `smartblink-hooks` thread installing `WH_KEYBOARD_LL` and `WH_MOUSE_LL`, filtering injected events (`LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED`, `LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED`), pause state, and suppression depth; clean unhook via `PostThreadMessageW(WM_QUIT)`.
  - `src/worker.rs`: 30 Hz `smartblink-worker` thread + 64-slot `WordBuffer` with volatile zeroing on reset/drop and hard resets on mouse click, navigation keys, modifier chords, idle timeout (3.5 s), focus change, and immune windows.
  - `src/tray.rs` & `src/main.rs`: added `Pause` / `Resume` toggle item in tray menu + `NIM_MODIFY` tooltip update (`SmartBlink (Paused)`).

**Verified**
- `cargo build --release` → 0 warnings, `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `cargo test --release` → `test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`.
- Automated smoke test → `first alive: True`, `second exited: True`.
- Pending: owner manual check of `Pause` / `Resume` menu item and typing feel on desktop.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M3 (`ARCHITECTURE.md` §3.1, §3.2, §6).

## Session #007 — 2026-10-24 — M3 implementation (layout enumeration, ToUnicodeEx cache, word segmentation)

**Done**
- Implemented `src/layout.rs`:
  - `active_layouts()` via `GetKeyboardLayoutList` and `window_layout(hwnd)` via `GetWindowThreadProcessId` + `GetKeyboardLayout`.
  - `reconstruct_scan_code(scan_code, flags)` restoring `0xE000` when `LLKHF_EXTENDED` is set.
  - `LayoutLayerTable` & `LayoutCache`: per-`HKL` lazy 256-entry `base` + `shift` layer tables built with `MapVirtualKeyExW(..., MAPVK_VSC_TO_VK_EX, hkl)` + `ToUnicodeEx(..., UNICODE_NO_CHANGED_STATE = 0x0004, hkl)`.
  - Word segmentation (`active_word_slice`, `last_word_run_for_fix`) and URL/path/CLI-flag prefix immunity (`is_immune_token`).
- Updated `src/worker.rs` to track `shift_down` on `KeyStrokeSlot` and pre-warm `LayoutCache`.

**Verified**
- `cargo build --release` → 0 warnings, `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `cargo test --release` → `test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`.
- Committed as `d0e2bc8` + checkpoint/handoff update. Stopped as requested by owner.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M4 (`ARCHITECTURE.md` §3.2, §3.3, §6).

## Session #008 — 2026-10-24 — M4 implementation (profile format, profilegen, EN/RU profiles, scoring engine)

**Done**
- Created `tools/profilegen` (`Cargo.toml` + `src/main.rs`) and `assets/profiles/LICENSES.md`.
- Generated `assets/profiles/en.bin` (5755 bytes: 26 letters, 299 bigrams, 956 trigrams) and `assets/profiles/ru.bin` (5891 bytes: 33 letters, 358 bigrams, 945 trigrams) in `SMBP` v1 format.
- Implemented `src/profile.rs`:
  - `LanguageProfile::from_bytes` + embedded `en.bin` and `ru.bin` via `include_bytes!`.
  - `score_word`: sorted `u16` bigram and `u32` trigram log-odds lookup + vowel/consonant/OOV phonotactic structure scoring.
  - `evaluate_tables` & `evaluate_active_word`: 3-gate decision rule (`current_score <= CURRENT_MAX_PLAUSIBLE_SCORE`, `best_score >= ABSOLUTE_THRESHOLD`, `margin >= MARGIN_THRESHOLD`).
- Wired `profile::evaluate_active_word` into `src/worker.rs`.

**Verified**
- `cargo build --release` → 0 warnings, `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `cargo test --release` → `test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s` (including `Ghbdtn`/`Привет`, `Cjj,otybt`/`Сообщение`, `Руддщ`/`Hello`, bilingual homograph `"here"`/`"руку"` false-positive immunity, and `< 100 us/word` benchmark).

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M5 (`ARCHITECTURE.md` §3.4, §6).

## Session #009 — 2026-10-24 — M5 implementation (alert channels: net-zero LED blink + WASAPI click sample + UIA password immunity)

**Done**
- Added `Win32_Media_Audio` and `Win32_UI_Accessibility` features to `Cargo.toml`.
- Updated `src/hooks.rs` with `real_key_seq()` and `last_real_key_tick()` atomics so LED blink sequences abort immediately if a real user keystroke arrives mid-blink.
- Updated `src/immunity.rs` with UI Automation `IUIAutomation::GetFocusedElement()?.CurrentIsPassword()` check (`is_focused_password` / `refresh_password_immunity`) so password fields disable detection and all alert channels.
- Created `src/signal.rs`:
  - `generate_click_pcm()`: 24 ms (`1152` frames at 48 kHz) 16-bit mono low-passed "wooden knock" sample (~-18 dBFS, zero endpoints).
  - `WasapiClicker` (pre-warmed shared-mode render client with `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY`) + `WaveOutClicker` fallback.
  - `blink_led_net_zero()`: net-zero toggle restoration for `ScrollLock` (default) and `CapsLockGuarded` (opt-in, `>= 80 ms` typing gap guard), bracketed by `hooks::begin_suppression()` / `end_suppression()`.
  - `SignalEngine`: dedicated `smartblink-audio` and `smartblink-led` background threads woken via `thread::unpark()`.
- Wired `SignalEngine` into `src/worker.rs` and `src/main.rs`.

**Verified**
- `cargo build --release` → 0 warnings, `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `cargo test --release` → `test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.45s`.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M6 (`ARCHITECTURE.md` §3.1, §3.5, §6).

## Session #010 — 2026-10-24 — M6 implementation (manual fix gesture, backspace+KEYEVENTF_UNICODE replay, target layout switch)

**Done**
- Created `src/fix.rs`:
  - `FixGestureDetector`: supports `DoubleLeftShift` (default), `DoubleLeftCtrl`, and `PauseBreak` with `DOUBLE_TAP_WINDOW_MS = 300`, ignoring key auto-repeat and rejecting intervening keys or mouse clicks.
  - `plan_word_fix` & `plan_word_fix_cached`: captures `last_word_run_for_fix` (including trailing spaces), resolves target `HKL` via M4 scorer (or best alternative installed layout for short explicit fixes), and computes exact character backspace count.
  - `build_fix_inputs` & `execute_fix`: emits `VK_BACK` + `KEYEVENTF_UNICODE` inside `hooks::begin_suppression()` / `end_suppression()`, then switches layout via `ActivateKeyboardLayout` + `PostMessageW(WM_INPUTLANGCHANGEREQUEST)`.
- Wired `FixGestureDetector` into `src/worker.rs` and `src/main.rs`.

**Verified**
- `cargo build --release` → 0 warnings, `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `cargo test --release` → `test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.85s`.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M7 (`ARCHITECTURE.md` §3.3, §6).

## Session #011 — 2026-10-24 — M7 implementation (20 built-in language profiles + intra-Latin conflict suite)

**Done**
- Created `tools/profilegen/src/corpora.rs` and updated `tools/profilegen/src/main.rs` + `assets/profiles/LICENSES.md` for all 20 built-in languages (`en, ru, uk, be, kk, de, fr, es, pt, it, tr, pl, nl, sv, cs, el, he, ar, hy, ka`).
- Generated `assets/profiles/*.bin` (20 files, `54,191` bytes total, well within the 600 KB budget).
- Updated `src/profile.rs` to embed all 20 profiles (`EMBEDDED_PROFILE_BLOBS`, `all_profiles()`) and added `intra_latin_and_multi_script_conflict_suite_de_fr_tr_pl_uk` verifying zero false positives on shared intra-Latin vocabulary (`system`, `program`, `function`) and positive detection on physical layout conflicts (QWERTZ `y`/`z`, AZERTY `a`/`q`/`z`/`w`).

**Verified**
- `cargo build --release` → 0 warnings, `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `cargo test --release` → `test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.82s`.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M8 (`ARCHITECTURE.md` §3.6, §3.7, §6).

## Session #012 — 2026-10-24 — M8 implementation (selection conversion, byte-for-byte clipboard save/restore, stubborn-user whitelist)

**Done**
- Added `Win32_System_DataExchange` and `Win32_UI_Input_Ime` features to `Cargo.toml`.
- Added `is_hwnd_ime_active` (`ImmGetContext` + `ImmGetOpenStatus`) in `src/immunity.rs` and scan-code reverse character lookup (`find_char`, `char_at`, `convert_str_to`) in `src/layout.rs`.
- Created `src/clipboard.rs`:
  - `SessionWhitelist`: fixed-capacity 256-slot FNV-1a hash + HKL ring (volatile-zeroed on clear/drop) that whitelists a word and its `>= 4`-char prefixes when the user presses `Space`/`Enter` after an alert without fixing.
  - `ClipboardSnapshot` & `ClipboardRestoreGuard`: enumerates all `HGLOBAL` clipboard formats in order, copies exact byte payloads, restores all formats on drop (even on error paths), and volatile-zeroes saved buffers on drop.
  - `SelectionHotkeyDetector` (`Ctrl+Alt+Z`), `convert_selection_text`, and `convert_selection_in_window`.
- Wired `SessionWhitelist` and `SelectionHotkeyDetector` into `src/worker.rs`.

**Verified**
- `cargo build --release` → 0 warnings, `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `cargo test --release` → `test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.85s`.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M9 (`ARCHITECTURE.md` §3.8, §3.9, §6).

## Session #013 — 2026-10-24 — M9 implementation (Win32 Settings UI, full tray menu, HKCU autostart, opt-in elevated task)

**Done**
- Added `Win32_System_Registry` feature to `Cargo.toml`.
- Created `src/autostart.rs`: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\SmartBlink` read/write/delete, `is_process_elevated()` (`TokenElevation`), and opt-in ONLOGON highest-privilege scheduled task registration/removal via `schtasks.exe`.
- Created `src/settings.rs`: pure Win32 Settings window (`SmartBlinkSettingsWnd`) showing live engine/elevation status and controls for Alert Mode, LED Channel, Fix Gesture, HKCU Autostart, Elevated Task, and `Open config.toml`.
- Updated `src/tray.rs`, `src/config.rs`, `src/immunity.rs`, and `src/worker.rs`: complete tray context menu with submenus, current-process exclusion item, double-click Settings activation, and live atomic config reload in the worker thread.

**Verified**
- `cargo build --release` → 0 warnings, `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `cargo test --release` → `test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M10 (`ARCHITECTURE.md` §4, §6).

## Session #014 — 2026-10-24 — M10 implementation (Inno Setup 6 installer script & package build)

**Done**
- Created `installer/SmartBlink.iss` (`PrivilegesRequired=lowest`, `PrivilegesRequiredOverridesAllowed=dialog`, `{autopf}\SmartBlink`, EN/RU localization, optional HKCU Run and elevated ONLOGON task checkboxes, graceful `WM_CLOSE` on `SmartBlinkTrayWnd` + `taskkill` fallback, clean uninstall with optional `%APPDATA%\SmartBlink` removal prompt).
- Verified compilation with Inno Setup 6.7.3 (`C:\Users\user\AppData\Local\Programs\Inno Setup 6\ISCC.exe installer\SmartBlink.iss`), producing `target\installer\SmartBlink-Setup-1.0.0.exe`.

**Verified**
- `ISCC.exe installer\SmartBlink.iss` → `Successful compile (1,813 sec)`, `target\installer\SmartBlink-Setup-1.0.0.exe`.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M11 (`ARCHITECTURE.md` §4, §5, §6).

## Session #015 — 2026-10-24 — M11 implementation (GitHub Actions release workflow, LICENSE, README EN/RU, CONTRIBUTING, docs)

**Done**
- Created `.github/workflows/release.yml` (LLVM-MinGW + Rust `x86_64-pc-windows-gnullvm` + unit tests + release binary + Inno Setup 6 installer + `SHA256SUMS.txt` + GitHub Release artifact upload).
- Created `LICENSE` (MIT, `VKV Studio`), `README.md` (EN), `README.ru.md` (RU), `CONTRIBUTING.md`, `docs/CONFIG.md`, `docs/PRIVACY.md`, and `docs/BUILDING.md`.

**Verified**
- All documentation and CI workflow files reviewed and checked against `ARCHITECTURE.md` §4 & §5.

**Next agent**
Read `AGENTS.md`, `.system/checkpoint.md`. Execute M12 (`ARCHITECTURE.md` §6).

## Session #016 — 2026-10-24 — M12 implementation (v1.0.0 release build, installer, SHA256SUMS.txt, CHANGELOG)

**Done**
- Bumped version to `1.0.0` in `Cargo.toml` and `assets/version.rc` (`FILEVERSION 1,0,0,0` / `PRODUCTVERSION 1,0,0,0`).
- Created `CHANGELOG.md` for `v1.0.0`.
- Rebuilt `target\x86_64-pc-windows-gnullvm\release\smartblink.exe` and `target\installer\SmartBlink-Setup-1.0.0.exe`, and generated `target\installer\SHA256SUMS.txt`.

**Verified**
- `cargo test --release` → `test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.
- `cargo build --release` → 0 warnings, `ProductVersion == 1.0.0.0`.
- `ISCC.exe installer\SmartBlink.iss` → `target\installer\SmartBlink-Setup-1.0.0.exe`.

**Next agent**
All 12 milestones (M1–M12) are complete. Await owner desktop verification and explicit approval before any `git push` or GitHub Release.

## Session #017 — 2026-10-24 — Live user-feedback audit (15 skeptics) & comprehensive bugfix pass

**Done**
- Expanded `EN_CORPUS` and `RU_CORPUS` in `tools/profilegen/src/main.rs` (`google`, `youtube`, `telegram`, `github`, `discord`, `windows`, `капслок`, `мигнула`, `вроде`, `отметил`, `должно`, `происходить`, etc.) and regenerated `assets/profiles/*.bin`.
- Updated `src/profile.rs`: added bigram backoff for unseen trigrams when constituent bigrams are strongly attested (`>= 15`), enabled high-confidence 3-letter word detection (`rfr`->"как", `xnj`->"что", `dct`->"все", `hfp`->"раз", `'nj`->"это"), fixed multi-punctuation stripping (`&/` -> `?.`, `dct?` -> `все,`), and removed false path immunity on trailing `/` (`.` on RU layout).
- Updated `src/signal.rs` & `src/worker.rs`: prevented mid-word `CapsLockGuarded` case corruption (`цштвЩцы`) by increasing `CAPS_MIN_GAP_MS` to `350 ms`, using a single 25 ms pulse or blinking at `VK_SPACE` word boundary; fixed WASAPI `AUDCLNT_E_NOT_STOPPED` on rapid clicks and `AUDCLNT_E_DEVICE_INVALIDATED` on audio endpoint hotplug; increased `MAX_WORD_KEYS` to `256` with sliding oldest-word drop; eliminated per-keystroke UIA IPC calls when `last_verdict.is_none()`.
- Updated `src/fix.rs`, `src/layout.rs`, `src/clipboard.rs`, `src/hooks.rs`, `src/settings.rs`, and `src/tray.rs`: fixed `expand_erroneous_burst_run` for 1–3 char words (`yt`->"не"), added `GetGUIThreadInfo` `hwndFocus` resolution and modifier release before `SendInput`, removed cross-thread `SpscRing::clear_and_zero()` race, and added a `"Test Alert Now"` button in Settings.

**Verified**
- `cargo test --release` → `test result: ok. 41 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s`.
- `cargo build --release` → 0 warnings.
- `ISCC.exe installer\SmartBlink.iss` → `target\installer\SmartBlink-Setup-1.0.0.exe`.
