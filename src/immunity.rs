//! Foreground window & process immunity checks (ARCHITECTURE.md §3.7).
//! Caches process immunity per HWND so OpenProcess/QueryFullProcessImageNameW
//! runs only when the foreground window handle actually changes.

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
use windows::Win32::UI::Input::Ime::{ImmGetContext, ImmGetOpenStatus, ImmReleaseContext};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// Extract lowercase process image stem without `.exe` from a full path or filename.
pub fn normalize_process_name(path_or_name: &str) -> String {
    let trimmed = path_or_name.trim();
    let filename = trimmed
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(trimmed);
    let lower = filename.to_ascii_lowercase();
    lower
        .strip_suffix(".exe")
        .unwrap_or(&lower)
        .to_string()
}

/// Returns `true` if `path_or_name` matches any entry in `blacklist` (normalized).
pub fn is_process_blacklisted(path_or_name: &str, blacklist: &[String]) -> bool {
    let stem = normalize_process_name(path_or_name);
    if stem.is_empty() {
        return false;
    }
    blacklist
        .iter()
        .any(|entry| normalize_process_name(entry) == stem)
}

/// Query the executable image stem (lowercase, no `.exe`) of the process owning `hwnd`.
pub fn query_hwnd_process_name(hwnd: HWND) -> Option<String> {
    if hwnd.0.is_null() {
        return None;
    }
    unsafe {
        let mut pid: u32 = 0;
        let tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if tid == 0 || pid == 0 {
            return None;
        }
        let hproc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            hproc,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(hproc);
        if !ok || len == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        Some(normalize_process_name(&path))
    }
}

/// Returns `true` if `hwnd` has an actively open IME context (`ARCHITECTURE.md` §3.7).
pub fn is_hwnd_ime_active(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    unsafe {
        let himc = ImmGetContext(hwnd);
        if himc.0.is_null() {
            return false;
        }
        let is_open = ImmGetOpenStatus(himc).as_bool();
        let _ = ImmReleaseContext(hwnd, himc);
        is_open
    }
}

#[derive(Default)]
pub struct FocusCache {
    last_hwnd: isize,
    last_immune: bool,
    needs_recheck: bool,
    uia: Option<IUIAutomation>,
    uia_init_attempted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusStatus {
    pub hwnd_changed: bool,
    pub is_immune: bool,
}

impl FocusCache {
    pub const fn new() -> Self {
        Self {
            last_hwnd: 0,
            last_immune: false,
            needs_recheck: false,
            uia: None,
            uia_init_attempted: false,
        }
    }

    pub fn current_hwnd(&self) -> HWND {
        HWND(self.last_hwnd as *mut core::ffi::c_void)
    }

    pub fn invalidate(&mut self) {
        self.needs_recheck = true;
    }

    fn ensure_uia(&mut self) -> Option<&IUIAutomation> {
        if !self.uia_init_attempted {
            self.uia_init_attempted = true;
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                self.uia = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok();
            }
        }
        self.uia.as_ref()
    }

    /// Query UI Automation `IsPassword` on the currently focused UI element (`AGENTS.md` #7).
    pub fn is_focused_password(&mut self) -> bool {
        let Some(uia) = self.ensure_uia() else {
            return false;
        };
        unsafe {
            let Ok(elem) = uia.GetFocusedElement() else {
                return false;
            };
            elem.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(false)
        }
    }

    /// Re-check whether the currently focused child element is a password field even if top-level `HWND` did not change.
    pub fn refresh_password_immunity(&mut self) -> bool {
        if self.last_immune {
            return true;
        }
        if self.is_focused_password() {
            self.last_immune = true;
            return true;
        }
        false
    }

    /// Check the current foreground window and return whether focus changed and whether it is immune.
    pub fn update(&mut self, blacklist: &[String]) -> FocusStatus {
        let hwnd = unsafe { GetForegroundWindow() };
        self.update_for_hwnd(hwnd, blacklist)
    }

    pub fn update_for_hwnd(&mut self, hwnd: HWND, blacklist: &[String]) -> FocusStatus {
        let raw = hwnd.0 as isize;
        if raw == 0 {
            // Transient NULL foreground window during Win32 window activation/deactivation:
            // preserve last known HWND so we never falsely reset `WordBuffer` mid-word.
            return FocusStatus {
                hwnd_changed: false,
                is_immune: self.last_immune,
            };
        }
        let changed = raw != self.last_hwnd;
        if !changed && !self.needs_recheck {
            return FocusStatus {
                hwnd_changed: false,
                is_immune: self.last_immune,
            };
        }
        self.last_hwnd = raw;
        self.needs_recheck = false;
        let proc_immune = match query_hwnd_process_name(hwnd) {
            Some(proc_name) => {
                crate::config::set_last_foreground_process(&proc_name);
                is_process_blacklisted(&proc_name, blacklist)
            }
            None => false,
        };
        let immune = proc_immune || is_hwnd_ime_active(hwnd) || self.is_focused_password();
        self.last_immune = immune;
        FocusStatus {
            hwnd_changed: changed,
            is_immune: immune,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn normalize_paths_and_extensions() {
        assert_eq!(
            normalize_process_name(r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal\WindowsTerminal.exe"),
            "windowsterminal"
        );
        assert_eq!(
            normalize_process_name(r"C:\Windows\System32\cmd.EXE"),
            "cmd"
        );
        assert_eq!(normalize_process_name("Code.exe"), "code");
        assert_eq!(normalize_process_name("pwsh"), "pwsh");
    }

    #[test]
    fn default_blacklist_matches_terminals_and_ides() {
        let cfg = Config::default();
        assert!(is_process_blacklisted(
            r"C:\Users\user\AppData\Local\Programs\Microsoft VS Code\Code.exe",
            &cfg.blacklist
        ));
        assert!(is_process_blacklisted("WindowsTerminal.exe", &cfg.blacklist));
        assert!(is_process_blacklisted("powershell.exe", &cfg.blacklist));
        assert!(is_process_blacklisted("devenv.exe", &cfg.blacklist));
        assert!(!is_process_blacklisted(
            r"C:\Windows\System32\notepad.exe",
            &cfg.blacklist
        ));
        assert!(!is_process_blacklisted("telegram.exe", &cfg.blacklist));
    }
}
