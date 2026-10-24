# Contributing to SmartBlink

Thank you for your interest in contributing to **SmartBlink**!

## Core Architectural Invariants

Before proposing changes, read [`ARCHITECTURE.md`](ARCHITECTURE.md). All contributions must preserve these non-negotiable invariants:

1. **No automatic text mutation.** SmartBlink never deletes, replaces, or alters typed text or switches keyboard layouts unless the user explicitly triggers the manual fix gesture (`Double Left Shift`, `Double Left Ctrl`, or `Pause/Break`) or the selection conversion hotkey (`Ctrl+Alt+Z`).
2. **Keystrokes never leave RAM.** No disk or network logging of typed characters. Both the 256-slot SPSC ring buffer and the 64-slot word buffer volatile-zero their memory on reset and drop.
3. **Minimal hook callback.** `LowLevelKeyboardProc` and `LowLevelMouseProc` only append a 16-byte `InputRecord` to the lock-free ring buffer and return immediately. No allocations, locks, I/O, or scoring inside the hook thread.
4. **No `N*(N-1)` character mapping tables.** Layout conversion uses physical scan codes (`+0xE000` extended prefix) + `ToUnicodeEx(..., UNICODE_NO_CHANGED_STATE)` cached per `HKL` and per-language binary n-gram profiles (`assets/profiles/*.bin`).
5. **Net-zero LED blink & byte-for-byte clipboard preservation.** Original `Scroll Lock` / `Caps Lock` toggle state is restored after every blink, and all `HGLOBAL` clipboard formats are restored in original order after selection conversion.
6. **Zero heavy runtime dependencies.** Pure Rust + `windows` crate (`x86_64-pc-windows-gnullvm`).

## Development Workflow

1. Follow the build instructions in [`docs/BUILDING.md`](docs/BUILDING.md).
2. Run the full test suite before submitting a pull request:
   ```powershell
   cargo test --release
   cargo build --release
   ```
3. Ensure zero compiler warnings and include deterministic unit tests for any new detection, conversion, or immunity behavior.

## License

By contributing to SmartBlink, you agree that your contributions will be licensed under the [MIT License](LICENSE).
