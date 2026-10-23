# NEXT TASK — M1 finish + rename to SmartBlink

Written 2026-10-23 by reviewer session. Executor: follow steps IN ORDER. Do not improvise scope.
Each step has an acceptance check. A step is done only when its check passes.

## 0. Hard rules for this task (read first)
- **Stop rule:** if the same command fails with the same error twice → STOP, do not retry a third time. Write the exact error into `HANDOFF.md` and report to owner. Never re-run an unchanged command hoping for a different result.
- **Owner "стоп" = stop immediately.** No more tool calls except saving checkpoint/handoff.
- Forbidden: MSVC (`cl.exe`, `link.exe`, `rc.exe`), Windows SDK install, `x86_64-pc-windows-msvc` target, new crates, `git push`, deleting files, renaming the repo folder `C:\projects\SmartPunto` (stays as is).
- Allowed without asking: `rustup toolchain install`/`rustup default` (step 1), editing files listed below, local git commits.
- Ask owner first: anything with `pacman`, `winget`, or any other install.
- Every shell command needs: `$env:Path = "C:\Users\user\.cargo\bin;" + $env:Path` (rustc not on PATH). The fnm "Node selection failed" line is harmless noise — ignore it.
- Commit messages in English, author `-c user.name="VKV Studio" -c user.email="dev@vkvstudio.com"`, no AI/model names anywhere.
- Do not commit: `scratch/`, `scratch_*.log`, `assets/icon_*.png`, `target/`.

## Facts already verified (do not re-investigate)
- MSYS2 gcc `C:\msys64\mingw64\bin\gcc.exe` is BROKEN (collect2 → `ld returned 53`). Never use it.
- LLVM-MinGW UCRT is installed and works. Dir (call it `$LLVM`):
  `C:\Users\user\AppData\Local\Microsoft\WinGet\Packages\MartinStorsjo.LLVM-MinGW.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\llvm-mingw-20261006-ucrt-x86_64\bin`
  Contains: `x86_64-w64-mingw32-clang.exe`, `x86_64-w64-mingw32-windres.exe`, `ld.lld.exe`.
- LLVM-MinGW has **no `libgcc.a` / `libgcc_eh.a`** → the Rust target `x86_64-pc-windows-gnu` CANNOT link with it. The matching Rust target is **`x86_64-pc-windows-gnullvm`**. This is why all previous builds failed.
- Current rustup: default `stable-x86_64-pc-windows-gnu` 1.99.1; only target installed `x86_64-pc-windows-gnu`.
- `assets/version.rc` line 1 is garbage text (`Microsoft (R) COFF resource compiler example script`) → resource compile will fail until removed.

## Step 1 — Toolchain: switch to gnullvm
1. `rustup toolchain install stable-x86_64-pc-windows-gnullvm`
   then `rustup default stable-x86_64-pc-windows-gnullvm`. Check: `rustc -vV` shows `host: x86_64-pc-windows-gnullvm`.
   - If the toolchain is not available for download → STOP, report to owner (fallback is repairing MSYS2 via pacman, needs owner OK).
2. Create USER-level file `C:\Users\user\.cargo\config.toml` (machine-specific paths live here, NOT in the repo):
   ```toml
   [target.x86_64-pc-windows-gnullvm]
   linker = "<$LLVM>/x86_64-w64-mingw32-clang.exe"

   [env]
   SMARTBLINK_WINDRES = "<$LLVM>/x86_64-w64-mingw32-windres.exe"
   ```
   (use forward slashes, full path).
3. Rewrite repo `.cargo/config.toml` to contain ONLY:
   ```toml
   # Toolchain: LLVM-MinGW (gnullvm). Linker path is machine-specific: set it in ~/.cargo/config.toml.
   [build]
   target = "x86_64-pc-windows-gnullvm"
   ```
4. `cargo clean` then `cargo build --release`.
   **Check:** exe exists at `target\x86_64-pc-windows-gnullvm\release\smartpunto.exe`. (Old warnings fine; app is still buggy — that's steps 3–4.)
5. Commit: `Build: switch to x86_64-pc-windows-gnullvm (LLVM-MinGW)`.

## Step 2 — Rename product to SmartBlink (owner decision 2026-10-23)
Name rules: product/UI = `SmartBlink`; crate/exe = `smartblink`; folder in AppData = `SmartBlink`.
Replace in these places (exact):
- `Cargo.toml`: `name = "smartpunto"` → `"smartblink"` in `[package]` AND `[[bin]]`.
- `src/main.rs` doc comment line 1.
- `src/single_instance.rs`: mutex name → `w!("Local\\SmartBlink.SingleInstance")`.
- `src/config.rs`: `APP_DIR_NAME = "SmartBlink"`; header comment `# SmartBlink config...`.
- `src/tray.rs`: class `w!("SmartBlinkTrayWnd")`, window title and tooltip `"SmartBlink"`.
- `assets/version.rc`: all 5 `SmartPunto`/`smartpunto` strings (see step 3 for full file).
- `tools/gen_icon.py`: the 1 occurrence.
- `AGENTS.md`: title line 1, line 19 path, line 35 installer name → `installer/SmartBlink.iss`.
- `ARCHITECTURE.md`: every `SmartPunto` (12 hits: lines 1, 14, 108, 110, 113, 125, 133, 135, 139, 160 and others) → `SmartBlink`. Add one line under the title: `> Renamed from SmartPunto on 2026-10-23 (trademark distance from "Punto Switcher").`
- Do NOT rewrite old entries in `HANDOFF.md` (history). Do NOT rename the repo folder.
**Check:** `Select-String -Path src\*.rs,Cargo.toml,assets\version.rc,AGENTS.md,ARCHITECTURE.md -Pattern 'SmartPunto|smartpunto'` returns only the "Renamed from" line.
`cargo build --release` → `target\x86_64-pc-windows-gnullvm\release\smartblink.exe` exists.
Commit: `Rename product to SmartBlink`.

## Step 3 — Embed icon + manifest + version info (no MSVC, no new crates)
1. Overwrite `assets/version.rc` with:
   ```rc
   // SmartBlink resources: icon, manifest, version info. Compiled by build.rs via windres.
   #include <windows.h>

   101 ICON "icon.ico"
   1 24 "app.manifest"

   VS_VERSION_INFO VERSIONINFO
    FILEVERSION 0,1,0,0
    PRODUCTVERSION 0,1,0,0
    FILEFLAGSMASK 0x3fL
    FILEFLAGS 0x0L
    FILEOS 0x40004L
    FILETYPE 0x1L
    FILESUBTYPE 0x0L
   BEGIN
       BLOCK "StringFileInfo"
       BEGIN
           BLOCK "040904B0"
           BEGIN
               VALUE "CompanyName", "VKV Studio"
               VALUE "FileDescription", "SmartBlink - wrong-layout typing alert and one-gesture fix"
               VALUE "FileVersion", "0.1.0.0"
               VALUE "InternalName", "smartblink"
               VALUE "LegalCopyright", "MIT License - Copyright (c) 2026 VKV Studio"
               VALUE "OriginalFilename", "smartblink.exe"
               VALUE "ProductName", "SmartBlink"
               VALUE "ProductVersion", "0.1.0.0"
           END
       END
       BLOCK "VarFileInfo"
       BEGIN
           VALUE "Translation", 0x409, 1200
       END
   END
   ```
   (`101` must equal `IDI_MAIN` in tray.rs. `1 24` = manifest resource id 1, type RT_MANIFEST.)
2. Create `assets/app.manifest`: standard assembly manifest with
   - `<requestedExecutionLevel level="asInvoker" uiAccess="false"/>` (invariant #11),
   - `<supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>` (Win10/11),
   - `<dpiAware>true/pm</dpiAware>` + `<dpiAwareness>PerMonitorV2</dpiAwareness>`.
3. Create `build.rs` at repo root (std only, no crates):
   - `println!("cargo:rerun-if-changed=assets/version.rc")`, same for `assets/app.manifest`, `assets/icon.ico`, and `cargo:rerun-if-env-changed=SMARTBLINK_WINDRES`.
   - windres = env `SMARTBLINK_WINDRES`, else `"x86_64-w64-mingw32-windres"` from PATH.
   - Run: `<windres> --input assets/version.rc --output-format=coff --output <OUT_DIR>/resources.o --include-dir assets` with current dir = crate root. If it fails → `panic!` with windres stderr (fail loudly, never silently ship without icon).
   - `println!("cargo:rustc-link-arg-bins=<OUT_DIR>/resources.o")`.
4. Add `build = "build.rs"` under `[package]` in Cargo.toml (optional, auto-detected — fine either way).
**Check:** build passes; in PowerShell `(Get-Item target\x86_64-pc-windows-gnullvm\release\smartblink.exe).VersionInfo.ProductName` → `SmartBlink`. Explorer shows the icon on the exe.
Commit: `Embed icon, asInvoker manifest and version info via windres`.

## Step 4 — Fix runtime bugs (tray.rs, main.rs)
### 4a. `src/main.rs`
- Order must be: `single_instance::acquire()` FIRST → if None, return immediately. Only then `config::load_or_create()`.
- Remove `CoInitializeEx`/`CoUninitialize` and the `Win32_System_Com` import (not needed in M1; UIA comes later).
- Keep `#![windows_subsystem = "windows"]`.
### 4b. `src/tray.rs` — required changes
1. **Message loop (critical):** replace the `PeekMessageW` loop with
   ```rust
   let mut msg = MSG::default();
   while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
       let _ = TranslateMessage(&msg);
       DispatchMessageW(&msg);
   }
   ```
   (`GetMessageW` returns 0 on WM_QUIT and -1 on error — `.0 > 0` exits on both. Do NOT use `.as_bool()`: -1 would be treated as true → infinite loop.)
   Delete the `msg.message == WM_APP + 2` branch.
2. **Exit flow:** in `show_menu`, when Exit is chosen call `DestroyWindow(hwnd)`. Handle `WM_DESTROY` in `wnd_proc`: remove tray icon, then `PostQuitMessage(0)`. Remove the `remove_tray_icon` call after the loop.
3. After `TrackPopupMenu`, add `PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0))` (documented MS workaround so the menu closes properly).
4. Delete `WM_TIMER`/`KillTimer(4242)` dead code.
5. `NIM_ADD` result: if `Shell_NotifyIconW(NIM_ADD, ..)` returns false → return an error from `run` (do not run an invisible app).
6. Icon: `LoadImageW(hinst, PCWSTR(IDI_MAIN as *const u16), IMAGE_ICON, GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON), LR_DEFAULTCOLOR)`. If it fails → fall back to `LoadIconW(None, IDI_APPLICATION)`; never pass a null icon.
7. **Explorer restart:** `let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));` store in a `static AtomicU32`. In `wnd_proc`, if `msg == taskbar_created` → re-add the tray icon. Keep the window a normal hidden top-level window (do NOT switch to `HWND_MESSAGE` — message-only windows do not receive that broadcast).
8. Silence remaining unused-Result warnings with `let _ =` where ignoring is intentional.
9. Remove the unused `_cfg` param or keep it but name it `cfg` with `#[allow(unused)]` — prefer removing; main passes nothing.
**Check:** `cargo build --release` with zero warnings from our crate.

## Step 5 — config.rs fixes
1. Blacklist semantics: entries are **lowercase process image names without `.exe`**. Defaults:
   `["windowsterminal", "cmd", "powershell", "pwsh", "conhost", "code", "devenv"]`. Fix the doc comment to match.
2. Replace `unquote` + `split(',')` with a tiny char-by-char parser for the string array: supports `\"` and `\\` escapes, commas inside quotes, whitespace between items. Serializer escapes `\` → `\\` and `"` → `\"`.
3. `unquote` bug: only strip quotes when the value both starts AND ends with `"`.
4. `save()`: call `f.sync_all()?` before rename (`flush` on File is a no-op). `std::fs::rename` already maps to `MoveFileExW(REPLACE_EXISTING)` — keep.
5. Tests must assert exact values, not lengths:
   - roundtrip of `vec![r#"we"ird, app"#, r"back\slash", "term"]` → identical vec;
   - `blacklist = []` → empty vec;
   - existing defaults/custom/corrupt tests stay, update to new defaults.
6. `ARCHITECTURE.md` line 14 says config via the `toml` crate — change to: "hand-rolled flat TOML subset in `src/config.rs` (no dependency)".
**Check:** `cargo test --release` → all tests pass (paste the summary line into HANDOFF).

## Step 6 — Docs sync
- `ARCHITECTURE.md` line 11: target → `x86_64-pc-windows-gnullvm` (LLVM-MinGW, UCRT), no MSVC/SDK required.
- line 17: build → `cargo` + `build.rs`/`windres` (LLVM-MinGW); remove `rc.exe`/`link.exe`. CI note: GitHub runner must install LLVM-MinGW (decide at M11, leave a TODO line).
- line 108: mutex is `Local\SmartBlink.SingleInstance` (per user session — correct for a per-user tray app); second-instance → "exits silently (M1). Activation message to first instance: deferred."
- line 123: `assets/` contains `icon.ico, app.manifest, version.rc, profiles/*.bin`.
- line 139: replace `rc.exe` with `windres via build.rs`.
- `AGENTS.md` lines 33–34: toolchain = gnullvm, user-level `~/.cargo/config.toml` holds linker + `SMARTBLINK_WINDRES`; resources compiled automatically by `build.rs`.
Commit: `Fix tray loop and exit, config parser; sync docs with gnullvm toolchain`.

## Step 7 — Verification (executor runs these; report raw results)
```powershell
$exe = "target\x86_64-pc-windows-gnullvm\release\smartblink.exe"
Remove-Item "$env:APPDATA\SmartBlink\config.toml" -ErrorAction SilentlyContinue
$p1 = Start-Process $exe -PassThru; Start-Sleep 3
"first alive: " + -not $p1.HasExited                 # expect True (old bug: exited instantly)
$p2 = Start-Process $exe -PassThru; Start-Sleep 2
"second exited: " + $p2.HasExited                    # expect True (mutex)
"config exists: " + (Test-Path "$env:APPDATA\SmartBlink\config.toml")   # expect True
Get-Content "$env:APPDATA\SmartBlink\config.toml"
Stop-Process $p1.Id                                    # cleanup only; real Exit is checked by owner
```
**Owner manual check (executor asks owner, does not claim it):** icon visible in tray with tooltip "SmartBlink"; right-click → Exit closes app and icon disappears; restart Explorer → icon comes back.

## Step 8 — Close-out
- `.system/checkpoint.md`: mark M1 done ONLY if step 7 automated checks passed AND owner confirmed manual check; otherwise "M1 code complete, manual check pending". Next Immediate Task → M2 (hook thread + ring buffer, see ARCHITECTURE.md §2/§6).
- Delete content of this file's steps? NO — replace the file content with `# NEXT TASK\nSee checkpoint.md` only after M1 is confirmed done.
- Append dated entry to `HANDOFF.md`: what changed, test summary line, check results, open items.
- Final commit: `M1 verified: checkpoint and handoff`.

## Pending owner decisions (do NOT implement now)
Reviewer proposals awaiting owner: LED visibility wizard (many laptops have no Scroll Lock LED), accidental Caps Lock detection, "repeat gesture = undo fix", click-per-key vs per-word sound option. Backlog: RGB Dynamic Lighting, per-app layout hint on focus, game mode, winget/Scoop.
