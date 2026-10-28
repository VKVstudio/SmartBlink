# Building SmartBlink from Source

SmartBlink compiles into a single standalone native Windows x64 executable (`smartblink.exe`) with zero external runtime dependencies.

## Quick Build (One Command)

With any standard Windows Rust toolchain installed (`x86_64-pc-windows-msvc`, `x86_64-pc-windows-gnullvm`, or `x86_64-pc-windows-gnu`):

```powershell
git clone https://github.com/VKVstudio/SmartBlink.git
cd SmartBlink
cargo build --release
```

The compiled executable is placed in `target\release\smartblink.exe` (or `target\<triple>\release\smartblink.exe` if `--target` is passed).

## Running Tests

```powershell
cargo test --release
```

## Regenerating Language Profiles (Optional)

All 20 binary language profiles (`assets/profiles/*.bin`) are pre-built and checked into the repository. To regenerate them from `tools/profilegen`:

```powershell
cargo run --manifest-path tools/profilegen/Cargo.toml --release
```

## Optional Installer (`Inno Setup 6`)

If you want to package a local per-user installer on your machine:

```powershell
iscc installer\SmartBlink.iss
```
