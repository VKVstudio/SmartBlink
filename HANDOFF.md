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
