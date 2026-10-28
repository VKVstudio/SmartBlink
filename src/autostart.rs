//! Autostart registry (`HKCU\...\Run`) and opt-in elevated scheduled task (`ONLOGON`, `RL HIGHEST`)
//! for working above elevated admin windows (UIPI, M9, ARCHITECTURE.md §3.9, AGENTS.md #11).

use std::mem::size_of;
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, WIN32_ERROR};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    REG_VALUE_TYPE,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

pub const RUN_SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
pub const APP_RUN_VALUE_NAME: &str = "SmartBlink";
pub const ELEVATED_TASK_NAME: &str = "SmartBlink";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Returns `true` if the current SmartBlink process token is elevated (`TokenElevation`).
pub fn is_process_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

/// Returns `true` if `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\SmartBlink` is present.
pub fn is_registry_autostart_enabled() -> bool {
    query_run_value_named(APP_RUN_VALUE_NAME).is_some()
}

/// Enable or disable `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\SmartBlink`.
pub fn set_registry_autostart(enable: bool) -> bool {
    if enable {
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let cmd = format!("\"{}\"", exe.display());
        set_run_value_named(APP_RUN_VALUE_NAME, &cmd)
    } else {
        delete_run_value_named(APP_RUN_VALUE_NAME)
    }
}

/// Query a named string value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
pub fn query_run_value_named(value_name: &str) -> Option<String> {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, 0, KEY_QUERY_VALUE, &mut hkey)
            != WIN32_ERROR(0)
        {
            return None;
        }
        let name_w: Vec<u16> = value_name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut val_type = REG_VALUE_TYPE(0);
        let mut byte_len = 0u32;
        let rc = RegQueryValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            None,
            Some(&mut val_type),
            None,
            Some(&mut byte_len),
        );
        if rc != WIN32_ERROR(0) || val_type != REG_SZ || byte_len < 2 {
            let _ = RegCloseKey(hkey);
            return None;
        }
        let mut buf = vec![0u8; byte_len as usize];
        let rc2 = RegQueryValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            None,
            Some(&mut val_type),
            Some(buf.as_mut_ptr()),
            Some(&mut byte_len),
        );
        let _ = RegCloseKey(hkey);
        if rc2 != WIN32_ERROR(0) {
            return None;
        }
        let units: Vec<u16> = buf[..byte_len as usize]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let nul_pos = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        Some(String::from_utf16_lossy(&units[..nul_pos]))
    }
}

/// Set a named `REG_SZ` value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
pub fn set_run_value_named(value_name: &str, command: &str) -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_SUBKEY,
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut hkey,
            None,
        ) != WIN32_ERROR(0)
        {
            return false;
        }
        let name_w: Vec<u16> = value_name.encode_utf16().chain(std::iter::once(0)).collect();
        let data_w: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
        let data_bytes: Vec<u8> = data_w.iter().flat_map(|u| u.to_le_bytes()).collect();
        let rc = RegSetValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            0,
            REG_SZ,
            Some(&data_bytes),
        );
        let _ = RegCloseKey(hkey);
        rc == WIN32_ERROR(0)
    }
}

/// Delete a named value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
pub fn delete_run_value_named(value_name: &str) -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, 0, KEY_SET_VALUE, &mut hkey)
            != WIN32_ERROR(0)
        {
            return false;
        }
        let name_w: Vec<u16> = value_name.encode_utf16().chain(std::iter::once(0)).collect();
        let rc = RegDeleteValueW(hkey, PCWSTR(name_w.as_ptr()));
        let _ = RegCloseKey(hkey);
        // ERROR_SUCCESS (0) or ERROR_FILE_NOT_FOUND (2) both mean value is absent.
        rc == WIN32_ERROR(0) || rc == WIN32_ERROR(2)
    }
}

fn schtasks_path() -> std::path::PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    std::path::PathBuf::from(root)
        .join("System32")
        .join("schtasks.exe")
}

/// Check whether the optional `SmartBlink` ONLOGON scheduled task exists.
pub fn is_elevated_task_registered() -> bool {
    Command::new(schtasks_path())
        .args(["/Query", "/TN", ELEVATED_TASK_NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Opt-in registration of the `SmartBlink` ONLOGON highest-privilege scheduled task (`AGENTS.md` #11).
/// Prompts UAC once via `ShellExecuteW(..., "runas", "%SystemRoot%\\System32\\schtasks.exe", ...)`.
pub fn register_elevated_task_interactive(hwnd: HWND) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let bin_w: Vec<u16> = schtasks_path()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let args = format!(
        "/Create /TN \"{}\" /TR \"\\\"{}\\\"\" /SC ONLOGON /RL HIGHEST /F",
        ELEVATED_TASK_NAME,
        exe.display()
    );
    let args_w: Vec<u16> = args.encode_utf16().chain(std::iter::once(0)).collect();
    let hinst = unsafe {
        ShellExecuteW(
            hwnd,
            w!("runas"),
            PCWSTR(bin_w.as_ptr()),
            PCWSTR(args_w.as_ptr()),
            None,
            SW_HIDE,
        )
    };
    (hinst.0 as isize) > 32
}

/// Remove the `SmartBlink` scheduled task via `ShellExecuteW(..., "runas", "%SystemRoot%\\System32\\schtasks.exe", ...)`.
pub fn unregister_elevated_task_interactive(hwnd: HWND) -> bool {
    let bin_w: Vec<u16> = schtasks_path()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let args = format!("/Delete /TN \"{}\" /F", ELEVATED_TASK_NAME);
    let args_w: Vec<u16> = args.encode_utf16().chain(std::iter::once(0)).collect();
    let hinst = unsafe {
        ShellExecuteW(
            hwnd,
            w!("runas"),
            PCWSTR(bin_w.as_ptr()),
            PCWSTR(args_w.as_ptr()),
            None,
            SW_HIDE,
        )
    };
    (hinst.0 as isize) > 32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_run_value_roundtrip_and_cleanup() {
        let test_name = "SmartBlink_UnitTest_RunValue";
        let _ = delete_run_value_named(test_name);
        assert!(query_run_value_named(test_name).is_none());

        let expected = r#""C:\Program Files\SmartBlink\smartblink.exe""#;
        assert!(set_run_value_named(test_name, expected));
        let queried = query_run_value_named(test_name).expect("value must exist after set");
        assert_eq!(queried, expected);

        assert!(delete_run_value_named(test_name));
        assert!(query_run_value_named(test_name).is_none());
    }

    #[test]
    fn process_elevation_check_executes_cleanly() {
        let _elevated = is_process_elevated();
    }
}
