# Checkpoint — SmartBlink

`Current Phase`: M2 code complete, unit tests (15/15) + automated smoke test passed, owner manual check pending.
`Approved Stack`: Rust (`x86_64-pc-windows-gnullvm` via LLVM-MinGW UCRT; MSVC/Windows SDK forbidden), `windows` crate 0.58, Win32 tray + Settings window, hand-rolled flat TOML config in `%APPDATA%\SmartBlink\config.toml`, WASAPI shared audio, Inno Setup 6 installer (per-user default `{autopf}`, optional elevated ONLOGON scheduled task). License MIT. Publisher `VKV Studio` / vkvstudio.com. Min OS: Windows 10 1809. Default fix hotkey: double Left Shift. Default alert mode: Both (audio + LED), LED defaults to Scroll Lock.

## Completed
- Phase 0 docs + rename to **SmartBlink** (`ARCHITECTURE.md`, `AGENTS.md`). Repo folder stays `C:\projects\SmartPunto`.
- Toolchain switched to `stable-x86_64-pc-windows-gnullvm` (rustc 1.99.1). User-level `C:\Users\user\.cargo\config.toml` holds LLVM-MinGW `x86_64-w64-mingw32-clang.exe` linker and `SMARTBLINK_WINDRES` (`x86_64-w64-mingw32-windres.exe`); repo `.cargo/config.toml` has only `target = "x86_64-pc-windows-gnullvm"`.
- M1 complete (`cef2ab9`): single instance mutex, embedded icon/manifest/version info via `build.rs` + `windres`, config read/write, tray icon + `Exit`, confirmed on desktop by owner.
- M2 implementation complete:
  - `src/ring.rs`: lock-free 256-slot SPSC ring buffer (`InputRecord`, 16 bytes, RAM only). Producer never blocks/allocates; consumer volatile-zeroes every popped slot and `clear_and_zero()` zeroes on pause/shutdown.
  - `src/immunity.rs`: process path normalization (lowercase stem, no `.exe`), blacklist matching, `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `QueryFullProcessImageNameW` cached per foreground `HWND` (`FocusCache`).
  - `src/hooks.rs`: dedicated hook thread (`smartblink-hooks`) owning `WH_KEYBOARD_LL` and `WH_MOUSE_LL`. Minimal callbacks (<0.1 ms, zero allocations/locks) ignoring `PAUSED`, `SUPPRESS_DEPTH > 0`, and `LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED` / `LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED`. Mouse hook records only `WM_LBUTTONDOWN`/`WM_RBUTTONDOWN`/`WM_MBUTTONDOWN` click resets.
  - `src/worker.rs`: 30 Hz worker thread (`smartblink-worker`) + fixed 64-slot `WordBuffer` (volatile-zeroed on reset and `Drop`). Implements hard reset rules: mouse click, navigation/control keys, `Ctrl`/`Alt`/`Win` modifier chords, idle timeout (3500 ms), focus change, and immune process.
  - `src/tray.rs` & `src/main.rs`: added `Pause` / `Resume` menu item above separator + `Exit`, updating tray tooltip (`"SmartBlink"` vs `"SmartBlink (Paused)"`) via `NIM_MODIFY`.

## Verified
- `cargo build --release`: 0 warnings, produces `target\x86_64-pc-windows-gnullvm\release\smartblink.exe`.
- `cargo test --release`: `test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` (5 config + 2 ring + 2 immunity + 4 worker + 1 hooks + 1 tray resource).
- Automated runtime smoke test: `first alive: True`, `second exited: True`.
- Pending owner manual check on interactive desktop: tray menu shows `Pause` / `Exit`, toggling `Pause` switches menu to `Resume` and tooltip to `SmartBlink (Paused)`, no input lag while typing.

## Next Immediate Task
M3 — Layout enumeration (`GetKeyboardLayoutList`, active-layout set) + `ToUnicodeEx` (no-state-change flag `0x4`, extended scan-code `0xE0` reconstruction, per-HKL cached layer tables) + word segmentation unit tests on fixture scan-code traces (see `ARCHITECTURE.md` §3.1, §3.2, §6).

## Blockers
- None. (`iscc` for Inno Setup 6 needed only at M10.)

## Junk in repo root (list only, not deleted per rules)
- `scratch_build_m1.log`, `scratch_sdk_install.log`, `scratch/` dir, `assets/icon_16.png`..`icon_48.png` — untracked, not committed.
