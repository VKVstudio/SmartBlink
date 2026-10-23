# Checkpoint — SmartPunto

`Current Phase`: M1 in progress (code written, compile clean, link blocked by toolchain). Owner stopped session 2026-10-23 ~21:59 for context preservation.
`Approved Stack`: Rust (`x86_64-pc-windows-gnu` — GNU toolchain mandated by owner 2026-10-23, MSVC/Windows SDK forbidden), `windows` crate 0.58, Win32 tray + Settings window, TOML config in `%APPDATA%\SmartPunto`, WASAPI shared audio, Inno Setup 6 installer (per-user default `{autopf}`, optional elevated ONLOGON scheduled task). License MIT. Publisher `VKV Studio` / vkvstudio.com. Min OS: Windows 10 1809. Default fix hotkey: double Left Shift. Default alert mode: Both (audio + LED), LED defaults to Scroll Lock.

## Completed
- Phase 0 docs: `ARCHITECTURE.md`, `AGENTS.md`, owner decisions (MIT, `{autopf}`, Scroll-Lock LED default). Git root commit `128d0aa` (docs only) + checkpoint commit `4ff2ce3`. No push.
- M1 code written (uncommitted): `Cargo.toml` (windows 0.58 features incl. `Win32_Graphics_Gdi`, `Win32_Security`; release profile opt-level=s/lto/strip/panic=abort), `.cargo/config.toml` (target gnu + linker override), `src/main.rs`, `src/config.rs` (hand-rolled TOML subset codec + atomic save, 4 unit tests), `src/single_instance.rs` (named mutex), `src/tray.rs` (rewritten against windows 0.58 API: NOTIFYICONDATAW, TrackPopupMenu, LoadImageW), `assets/icon.ico` + `version.rc`, `tools/gen_icon.py`, `.gitignore`.

## Verified
- Compile of crate `smartpunto` succeeds on gnu target — only warnings remain (5 unused-Result/BOOL in tray.rs: ShowWindow, Shell_NotifyIconW results unwrapped).
- Toolchain present: rustup 1.99.1, default toolchain switched to `stable-x86_64-pc-windows-gnu`. rustc/cargo NOT on PATH by default — every command needs `$env:Path = "C:\Users\user\.cargo\bin;" + $env:Path` (fnm "Node selection failed" noise is harmless).
- NOT verified: final link never succeeded → no exe produced, no manual tray test, unit tests never run. Work is NOT production-complete.

## Next Immediate Task
M1 finish: fix linking, then `cargo build --release` + `cargo test --release config::tests`, manual tray smoke test (icon/tooltip/Exit, second instance silent), fix 5 warnings, commit.
First link attempt (untried): repoint `.cargo/config.toml` linker from broken MSYS2 gcc to LLVM-MinGW gcc:
`C:/Users/user/AppData/Local/Microsoft/WinGet/Packages/MartinStorsjo.LLVM-MinGW.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe/llvm-mingw-20261006-ucrt-x86_64/bin/gcc.exe`
Fallback: `bin/ld.lld.exe` in the same dir as linker (may need `-C link-self-contained=yes` tuning).

## Blockers
- MSYS2 mingw64 gcc is broken on this machine: `cc1.exe` missing (`C:\msys64\mingw64\libexec\gcc\x86_64-w64-mingw32` empty) → every link (incl. build scripts proc-macro2/quote/windows_x86_64_gnu) dies with `collect2.exe: error: ld returned 53 exit status`.
- MSVC path dead and forbidden by owner: BuildTools exists but no Windows SDK (`kernel32.lib` absent, winget SDK installs fail w/ 0x800706C3). Do not retry.
- `iscc` (Inno Setup) presence unverified this session; needed only at installer milestone, not M1.

## Junk in repo root (list only, not deleted per rules)
- `scratch_build_m1.log`, `scratch_sdk_install.log`, `scratch/` dir — build/sdk logs from this session. Do not commit.
