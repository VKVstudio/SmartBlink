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
