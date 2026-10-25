//! Tray icon, Bento flyout trigger, and message loop (M9/M12, ARCHITECTURE.md §3.8, §3.9).
//! Uses Shell_NotifyIconW with a hidden top-level window (`SmartBlinkTrayWnd`) so
//! `TaskbarCreated` broadcasts and installer `WM_CLOSE` messages arrive reliably.

use crate::hooks;
use crate::settings;
use std::sync::atomic::{AtomicU32, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, GetMessageW,
    GetSystemMetrics, LoadIconW, LoadImageW, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, ShowWindow, TranslateMessage, HICON, IDI_APPLICATION, IMAGE_ICON,
    LR_DEFAULTCOLOR, MSG, SM_CXSMICON, SM_CYSMICON, SW_HIDE, WM_APP, WM_CLOSE, WM_DESTROY,
    WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW,
};

pub const WM_TRAY: u32 = WM_APP + 1;
pub const WM_APP_OPEN_SETTINGS: u32 = WM_APP + 42;

/// Notify an already-running SmartBlink instance to open its Settings window.
pub fn notify_existing_instance_open_settings() {
    unsafe {
        if let Ok(hwnd) = FindWindowW(w!("SmartBlinkTrayWnd"), None) {
            if !hwnd.0.is_null() {
                let _ = PostMessageW(hwnd, WM_APP_OPEN_SETTINGS, WPARAM(0), LPARAM(0));
            }
        }
    }
}

const ICON_UID: u32 = 1;
const IDI_MAIN: usize = 101; // MAKEINTRESOURCE(101)

static TASKBAR_CREATED_MSG: AtomicU32 = AtomicU32::new(0);

/// Message loop. Returns Ok(()) on clean quit (Exit selected).
pub fn run() -> windows::core::Result<()> {
    unsafe {
        let hmod = GetModuleHandleW(None)?;
        let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
        TASKBAR_CREATED_MSG.store(taskbar_created, Ordering::Relaxed);

        let cls = w!("SmartBlinkTrayWnd");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: HINSTANCE(hmod.0 as _),
            lpszClassName: cls,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(windows::core::Error::from_win32());
        }

        let hwnd = CreateWindowExW(
            Default::default(),
            cls,
            w!("SmartBlink"),
            Default::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            HINSTANCE(hmod.0 as _),
            None,
        )?;
        settings::set_tray_owner_hwnd(hwnd);
        let _ = ShowWindow(hwnd, SW_HIDE);

        if let Err(err) = add_tray_icon(hwnd, hmod) {
            let _ = DestroyWindow(hwnd);
            return Err(err);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Ok(())
    }
}

unsafe fn load_tray_icon(hmod: HMODULE) -> HICON {
    let hinst = HINSTANCE(hmod.0 as _);
    let cx = GetSystemMetrics(SM_CXSMICON);
    let cy = GetSystemMetrics(SM_CYSMICON);
    if let Ok(h) = LoadImageW(
        hinst,
        PCWSTR(IDI_MAIN as *const u16),
        IMAGE_ICON,
        cx,
        cy,
        LR_DEFAULTCOLOR,
    ) {
        if !h.is_invalid() {
            return HICON(h.0);
        }
    }
    LoadIconW(None, IDI_APPLICATION).unwrap_or(HICON(std::ptr::null_mut()))
}

fn current_tooltip_text(paused: bool) -> &'static str {
    if paused {
        "SmartBlink (Paused)"
    } else {
        "SmartBlink"
    }
}

unsafe fn add_tray_icon(hwnd: HWND, hmod: HMODULE) -> windows::core::Result<()> {
    let hicon = load_tray_icon(hmod);

    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_UID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: hicon,
        ..Default::default()
    };
    let tip = current_tooltip_text(hooks::is_paused());
    copy_wtext(&utf16_units(tip), &mut nid.szTip);
    if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
        if let Ok(tray_hwnd) = FindWindowW(w!("Shell_TrayWnd"), None) {
            if !tray_hwnd.0.is_null() {
                return Err(windows::core::Error::from_win32());
            }
        }
    }
    Ok(())
}

pub unsafe fn sync_tray_tooltip(hwnd: HWND, paused: bool) {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_UID,
        uFlags: NIF_TIP,
        ..Default::default()
    };
    let tip = current_tooltip_text(paused);
    copy_wtext(&utf16_units(tip), &mut nid.szTip);
    let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
}

unsafe fn remove_tray_icon(hwnd: HWND) {
    let nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_UID,
        ..Default::default()
    };
    let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
}

fn copy_wtext<const N: usize>(lit: &[u16], dst: &mut [u16; N]) {
    let n = lit.len().min(N - 1);
    dst[..n].copy_from_slice(&lit[..n]);
    dst[n] = 0;
}

fn utf16_units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        m if m == WM_TRAY => {
            let mouse = (lparam.0 & 0xFFFF) as u32;
            if mouse == WM_LBUTTONUP || mouse == WM_LBUTTONDBLCLK {
                settings::show_settings_window();
            } else if mouse == WM_RBUTTONUP {
                settings::show_tray_flyout(hwnd);
            }
            LRESULT(0)
        }
        m if m == WM_APP_OPEN_SETTINGS => {
            settings::show_settings_window();
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            remove_tray_icon(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        m => {
            let tc = TASKBAR_CREATED_MSG.load(Ordering::Relaxed);
            if tc != 0 && m == tc {
                if let Ok(hmod) = GetModuleHandleW(None) {
                    let _ = add_tray_icon(hwnd, hmod);
                }
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_resource_loads() {
        unsafe {
            let hmod = GetModuleHandleW(None).expect("GetModuleHandleW");
            let hinst = HINSTANCE(hmod.0 as _);
            let cx = GetSystemMetrics(SM_CXSMICON);
            let cy = GetSystemMetrics(SM_CYSMICON);
            let h = LoadImageW(
                hinst,
                PCWSTR(IDI_MAIN as *const u16),
                IMAGE_ICON,
                cx,
                cy,
                LR_DEFAULTCOLOR,
            )
            .expect("LoadImageW for IDI_MAIN (101) must succeed");
            assert!(!h.is_invalid());
        }
    }
}
