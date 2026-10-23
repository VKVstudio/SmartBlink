# Checkpoint — SmartBlink

`Current Phase`: M1 code complete, automated verification passed, owner manual tray check pending.
`Approved Stack`: Rust (`x86_64-pc-windows-gnullvm` via LLVM-MinGW UCRT; MSVC/Windows SDK forbidden), `windows` crate 0.58, Win32 tray + Settings window, hand-rolled flat TOML config in `%APPDATA%\SmartBlink\config.toml`, WASAPI shared audio, Inno Setup 6 installer (per-user default `{autopf}`, optional elevated ONLOGON scheduled task). License MIT. Publisher `VKV Studio` / vkvstudio.com. Min OS: Windows 10 1809. Default fix hotkey: double Left Shift. Default alert mode: Both (audio + LED), LED defaults to Scroll Lock.

## Completed
- Phase 0 docs + rename to **SmartBlink** (`ARCHITECTURE.md`, `AGENTS.md`). Repo folder stays `C:\projects\SmartPunto`.
- Toolchain switched to `stable-x86_64-pc-windows-gnullvm` (rustc 1.99.1). User-level `C:\Users\user\.cargo\config.toml` holds LLVM-MinGW `x86_64-w64-mingw32-clang.exe` linker and `SMARTBLINK_WINDRES` (`x86_64-w64-mingw32-windres.exe`); repo `.cargo/config.toml` has only `target = "x86_64-pc-windows-gnullvm"`.
- M1 implementation complete:
  - `build.rs` + `assets/version.rc` + `assets/app.manifest` (`asInvoker`, PerMonitorV2 DPI) + `assets/icon.ico` (fixed `.ico` directory offset bug in `tools/gen_icon.py` and regenerated).
  - `src/main.rs`: acquires `Local\SmartBlink.SingleInstance` mutex BEFORE `config::load_or_create()`, then runs `tray::run()`.
  - `src/tray.rs`: blocking `GetMessageW` loop (`.0 > 0`), `DestroyWindow` -> `WM_DESTROY` -> `remove_tray_icon` + `PostQuitMessage(0)`, `PostMessageW(WM_NULL)` after `TrackPopupMenu`, `TaskbarCreated` re-registration via `RegisterWindowMessageW`, system small-icon metrics (`SM_CXSMICON`/`SM_CYSMICON`), graceful startup when `Shell_TrayWnd` is not yet present.
  - `src/config.rs`: lowercase process image names without `.exe` (`windowsterminal`, `cmd`, `powershell`, `pwsh`, `conhost`, `code`, `devenv`), char-by-char string array parser with `\"` and `\\` escape handling, `File::sync_all()` before atomic rename.

## Verified
- `cargo build --release`: 0 warnings, produces `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `(Get-Item target\x86_64-pc-windows-gnullvm\release\smartblink.exe).VersionInfo.ProductName` == `SmartBlink`.
- `cargo test --release`: `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` (5 config tests + 1 embedded icon resource test `IDI_MAIN=101`).
- Automated runtime verification script (Step 7):
  - `first alive: True`
  - `second exited: True`
  - `config exists: True` at `%APPDATA%\SmartBlink\config.toml` with expected default contents.
- Pending owner manual check on interactive desktop: tray icon + tooltip `"SmartBlink"`, right-click -> `Exit` closes process and removes icon, Explorer restart restores icon.

## Next Immediate Task
After owner confirms M1 manual tray check:
M2 — Keyboard/mouse hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`), lock-free SPSC ring buffer (RAM only, zeroed on reset), foreground window/process immunity check, tray Pause/Resume toggle (see `ARCHITECTURE.md` §2, §3.7, §6).

## Blockers
- None for M1/M2. (`iscc` for Inno Setup 6 needed only at M10.)

## Junk in repo root (list only, not deleted per rules)
- `scratch_build_m1.log`, `scratch_sdk_install.log`, `scratch/` dir, `assets/icon_16.png`..`icon_48.png` — untracked, not committed.
