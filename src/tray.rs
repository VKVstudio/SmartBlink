//! Tray icon + message loop (M1 scope: icon, tooltip, Exit menu).
//! Uses Shell_NotifyIconW with a hidden message-only window.

use crate::config::Config;
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetCursorPos, IMAGE_ICON, KillTimer, LoadImageW, LR_DEFAULTCOLOR, MF_STRING, MSG, PeekMessageW,
    PM_REMOVE, PostQuitMessage, RegisterClassW, SetForegroundWindow, ShowWindow, SW_HIDE,
    TrackPopupMenu, TPM_RETURNCMD, TPM_RIGHTBUTTON, TranslateMessage, WNDCLASSW, WM_APP,
    WM_LBUTTONUP, WM_RBUTTONUP, WM_TIMER,
};

pub const WM_TRAY: u32 = WM_APP + 1;
/// Menu command id for the Exit item (WM_APP range is free for app use).
pub const WM_TRAY_EXIT: usize = (WM_APP + 2) as usize;
const ICON_UID: u32 = 1;
const IDI_MAIN: usize = 101; // MAKEINTRESOURCE(101): low word set, high word zero.

/// Message loop. Returns Ok(()) on clean quit (Exit selected).
pub fn run(_cfg: &Config) -> windows::core::Result<()> {
    unsafe {
        let hmod = GetModuleHandleW(None)?;
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
        ShowWindow(hwnd, SW_HIDE).ok();

        add_tray_icon(hwnd, hmod);

        // Message loop until PostQuitMessage.
        let mut msg = MSG::default();
        loop {
            if !PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                break;
            }
            if msg.message == WM_APP + 2 {
                // Exit command posted back from the tray menu handler.
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        remove_tray_icon(hwnd);
        Ok(())
    }
}

unsafe fn add_tray_icon(hwnd: HWND, hmod: windows::Win32::Foundation::HMODULE) {
    let hicon = LoadImageW(
        HINSTANCE(hmod.0 as _),


        windows::core::PCWSTR(IDI_MAIN as *const u16), // MAKEINTRESOURCE equivalent
        IMAGE_ICON,
        32,
        32,
        LR_DEFAULTCOLOR,
    )
    .map(|h| windows::Win32::UI::WindowsAndMessaging::HICON(h.0))
    .unwrap_or(windows::Win32::UI::WindowsAndMessaging::HICON(std::ptr::null_mut()));

    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_UID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: hicon,
        ..Default::default()
    };
    copy_wtext(&utf16_units("SmartBlink"), &mut nid.szTip);
    Shell_NotifyIconW(NIM_ADD, &nid);
}

unsafe fn remove_tray_icon(hwnd: HWND) {
    let nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_UID,
        ..Default::default()
    };
    Shell_NotifyIconW(NIM_DELETE, &nid);
}

/// Copy a wide string (UTF-16 units, no terminator) into a fixed [u16; N] buffer.
fn copy_wtext<const N: usize>(lit: &[u16], dst: &mut [u16; N]) {
    let n = lit.len().min(N - 1);
    dst[..n].copy_from_slice(&lit[..n]);
    dst[n] = 0;
}

/// UTF-16 units of a Rust str, without terminator.
fn utf16_units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        m if m == WM_TRAY => {
            // LOWORD(lparam) = mouse message id.
            let mouse = (lparam.0 & 0xFFFF) as u32;
            if mouse == WM_LBUTTONUP || mouse == WM_RBUTTONUP {
                show_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            KillTimer(hwnd, 4242).ok();
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_menu(hwnd: HWND) {
    let hmenu = match CreatePopupMenu() {
        Ok(h) => h,
        Err(_) => return,
    };
    if AppendMenuW(hmenu, MF_STRING, WM_TRAY_EXIT, w!("Exit")).is_err() {
        DestroyMenu(hmenu).ok();
        return;
    }
    // TrackPopupMenu with TPM_RETURNCMD blocks until a command is chosen.
    SetForegroundWindow(hwnd).ok();
    let mut pt = POINT::default();
    GetCursorPos(&mut pt).ok();
    let cmd = TrackPopupMenu(
        hmenu,
        TPM_RIGHTBUTTON | TPM_RETURNCMD,
        pt.x,
        pt.y,
        0,
        hwnd,
        None,
    );
    DestroyMenu(hmenu).ok();
    if cmd.0 == WM_TRAY_EXIT as i32 {
        PostQuitMessage(0);
    }
}
