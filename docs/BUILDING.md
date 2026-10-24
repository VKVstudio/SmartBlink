# Building SmartBlink from Source

SmartBlink builds into a single standalone native Windows x64 executable (`smartblink.exe`, ~260 KB) with zero external runtime dependencies.

## Prerequisites

1. **Windows 10 (1809+) or Windows 11 (x64)**
2. **LLVM-MinGW (UCRT x86_64)**
   - Download `llvm-mingw-*-ucrt-x86_64.zip` from [mstorsjo/llvm-mingw](https://github.com/mstorsjo/llvm-mingw/releases) and add its `bin` folder (`x86_64-w64-mingw32-clang.exe`, `x86_64-w64-mingw32-windres.exe`) to `PATH`.
3. **Rust Toolchain (`stable-x86_64-pc-windows-gnullvm`)**
   ```powershell
   rustup toolchain install stable-x86_64-pc-windows-gnullvm
   rustup default stable-x86_64-pc-windows-gnullvm
   ```
4. **Inno Setup 6** *(optional, for building the setup installer)*
   - Install [Inno Setup 6](https://jrsoftware.org/isinfo.php) (`ISCC.exe`).

## Building & Testing

```powershell
# Run all unit, conflict, audio/LED, and clipboard round-trip tests
cargo test --release

# Build optimized release executable
cargo build --release
```

The compiled binary is written to:

```text
target\x86_64-pc-windows-gnullvm\release\smartblink.exe
```

## Regenerating Language Profiles (Optional)

All 20 binary language profiles (`assets/profiles/*.bin`) are pre-built and checked into the repository. To regenerate them from the offline generator tool:

```powershell
cargo run --manifest-path tools/profilegen/Cargo.toml --release
```

## Building the Installer

```powershell
iscc installer\SmartBlink.iss
```

The installer is output to `target\installer\SmartBlink-Setup-1.0.0.exe`.
