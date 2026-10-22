# Checkpoint — SmartPunto

`Current Phase`: Phase 0 complete — architecture approved by owner 2026-10-23. Ready for M1 (toolchain install pending).
`Approved Stack`: Rust (`x86_64-pc-windows-msvc`, static CRT, `windows` crate), Win32 tray + Settings window, TOML config in `%APPDATA%\SmartPunto`, WASAPI shared audio, Inno Setup 6 installer (per-user default `{autopf}`, optional elevated ONLOGON scheduled task). License MIT. Publisher `VKV Studio` / vkvstudio.com. Min OS: Windows 10 1809. Default fix hotkey: double Left Shift. Default alert mode: Both (audio + LED), LED defaults to Scroll Lock.

## Completed
- `ARCHITECTURE.md`: stack rationale, threading/hook model, `ToUnicodeEx` mapping with extended-scan reconstruction and no-state-change flag, profile format + scoring, alert channels, fix pipeline, selection/clipboard, immunity, tray/mutex/config, autostart + UIPI plan, installer pipeline, security model, roadmap M1..M12.
- Owner-approved decisions applied: MIT license, `{autopf}` Inno constant fixed, LED blink redesigned (Scroll Lock default + register-safety guard, Caps opt-in with gap-only blinking).
- `AGENTS.md`: invariants + session protocol (one step per session, no AI in commits, no push without approval).
- `HANDOFF.md`: Session #001 + #002 entries. Git repo initialized, docs committed.

## Verified
- Docs exist and are readable at `C:\projects\SmartPunto`. WinAPI constraints (`ToUnicodeEx` state mutation, `0xE0` scan reconstruction, `MAPVK_VK_TO_VSC_EX`, Inno per-user HKCU uninstall) checked against Microsoft/jrsoftware docs. Architecture reviewed and approved by owner.
- Not verified: nothing compiled — no code yet.

## Next Immediate Task
M1: Cargo skeleton + manifest/icon + global mutex single instance + tray icon with Exit + config create/read/write. Prerequisite: install toolchain (see Blockers).

## Blockers
- Toolchain checked 2026-10-23: `rustc`/`cargo` and `iscc` are NOT on PATH. Owner installs via winget before M1 (commands in HANDOFF #002); re-check `%USERPROFILE%\.cargo\bin` and `C:\Program Files (x86)\Inno Setup 6` at M1 start.
