# Privacy & Security Policy

SmartBlink is engineered with **Privacy by Design**:

1. **100% RAM-Only Keystroke Processing**
   - Keystrokes are captured as physical scan codes in a fixed 256-slot lock-free SPSC ring buffer and a 256-slot active word buffer.
   - Every slot is overwritten with zeros via `std::ptr::write_volatile` as soon as it is consumed, reset, or dropped.
   - The "stubborn user" session whitelist stores only 64-bit hashes in a fixed 256-entry RAM array (also volatile-zeroed on exit), never plaintext words.
   - **Zero disk logging:** SmartBlink never writes typed characters, words, or clipboard contents to disk.

2. **Zero Network Access**
   - SmartBlink links zero networking libraries (`WinHTTP`, `WinInet`, `Winsock` are not used).
   - There is no telemetry, analytics, crash reporting, or automatic background update checking.

3. **Automatic Password & Terminal Immunity**
   - Before alerting or executing a manual fix, SmartBlink queries Windows UI Automation (`IUIAutomationElement::CurrentIsPassword`) on the focused control.
   - When a password field is focused, detection is paused, audio/LED signals are suppressed, and fix hotkeys are ignored.
   - Terminals, IDEs, and user-excluded processes in `blacklist` are bypassed via a per-`HWND` focus cache.

4. **Clipboard Preservation**
   - During explicit `Ctrl+Alt+Z` selection conversion, all `HGLOBAL` clipboard formats (bounded to 16 MB per format / 32 MB total) are snapshotted in memory, restored byte-for-byte in original order immediately afterward (including on error paths), and volatile-zeroed on drop.

5. **Least Privilege (`asInvoker`)**
   - SmartBlink ships with an `asInvoker` application manifest and runs as a standard user by default.
   - Working above elevated Administrator windows (UIPI) is offered strictly as an opt-in Scheduled Task (`ONLOGON`, `RunLevel Highest`) created only on explicit user action.
