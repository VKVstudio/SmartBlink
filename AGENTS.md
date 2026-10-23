# AGENTS.md — SmartBlink

Windows tray utility for wrong-layout typing detection and one-gesture fix. Users look at the keyboard, not the screen: alerts are LED blink + muted click; correction happens only on explicit user gesture.

Read `ARCHITECTURE.md` before any code decision. This file is process rules only, no architecture.

## Architectural invariants (never violate)
1. **No automatic text mutation.** Never delete, replace, or alter user text without the fix gesture or the selection hotkey. No layout switching without a user-triggered fix.
2. **Keystrokes never leave RAM.** No disk/network logging of typed content. Ring buffer only, zeroed on reset.
3. **Hook thread stays minimal.** `LowLevelKeyboardProc`/`LowLevelMouseProc` bodies: append record + timestamp, return. No detection, no audio, no allocation, no blocking calls. Input lag is a product defect.
4. **Synthetic input must be marked and suppressed.** All injected events go through the single fix executor with suppression accounting; hook ignores `LLKHF_INJECTED`/`LLKHF_LOWER_IL_INJECTED`.
5. **No N*(N-1) manual char tables.** Mapping is `scan_code (+0xE0 for extended) + key state + HKL → ToUnicodeEx` with cached layer tables and per-language statistical profiles only.
6. **Hypothesis must clear margin + absolute threshold**, else stay silent. False positives are worse than silence.
7. **Immunity first.** Password fields (UI Automation `IsPassword`), IME-active windows, blacklisted processes, modifier chords, URL/path prefixes → passive. Signal channels disabled in password fields.
8. **LED blink is net-zero**: original Caps/Scroll state restored exactly.
9. **Clipboard preserved byte-for-byte** on selection conversion, all formats, restore even on error paths.
10. **No dependencies with big runtimes.** No Electron, no .NET runtime, no Python, no AHK. Rust + `windows` crate only; new crates require a note in `ARCHITECTURE.md`.
11. **Manifest stays `asInvoker`.** Elevation only via an opt-in scheduled task created on explicit user action.
12. Config lives in `%APPDATA%\SmartBlink\config.toml`, atomic rewrite, no secrets, no typed text.

## Session Protocol
1. On start: read `AGENTS.md`, `.system/checkpoint.md`, last entries of `HANDOFF.md`. Read `ARCHITECTURE.md` sections relevant to the task.
2. Do exactly **one** milestone step from `checkpoint.md` "Next Immediate Task". No extra features, no refactors outside scope.
3. Drafts, mocks, stubs and tests are never reported as production-complete. State what is verified vs pending.
4. Verify before claiming: build passes (`cargo build --release`), relevant unit tests pass, and the claimed behavior was actually exercised. If a check could not run, say so.
5. Git: local commit per finished verified step, message in English, **no AI mentions** — no `Co-authored-by`, no "Generated with", no agent/model names anywhere in commit metadata or files.
6. Never delete files, assets, or git history without explicit user approval for that specific action. Never copy the repo root to temp dirs.
7. No push, no deploy, no GitHub release, no cloud resources without explicit per-action user approval.
8. At session end (or when context fills): update `.system/checkpoint.md` (done / verified / pending / next task / blockers) and append a dated entry to `HANDOFF.md`. Keep both terse: facts, decisions, paths. No filler, no restating unchanged summaries.
9. Context budget is 64k. One file = one responsibility. Docs must be dense; do not duplicate architecture text into handoff beyond what changed.

## Build commands
- Rust toolchain: MSVC target, `cargo build --release` (see `.system/checkpoint.md` for the confirmed local toolchain path if rustc is not on PATH).
- Resources: `rc.exe assets/version.rc` before linking when version info changes.
- Installer: `iscc installer/SmartBlink.iss` (Inno Setup 6).

## Definition of done per milestone
Compiles in release, unit tests green for the touched module, manual behavior verified on the host, checkpoint + handoff updated, commit created.
