//! Tray icon + message loop (M1 scope: icon, tooltip, Exit menu).
//! Uses Shell_NotifyIconW with a hidden top-level window so TaskbarCreated broadcasts arrive.

use std::sync::atomic::{AtomicU32, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics, HICON, IDI_APPLICATION,
    IMAGE_ICON, LoadIconW, LoadImageW, LR_DEFAULTCOLOR, MF_STRING, MSG, PostMessageW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, ShowWindow,
    SM_CXSMICON, SM_CYSMICON, SW_HIDE, TrackPopupMenu, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TranslateMessage, WNDCLASSW, WM_APP, WM_DESTROY, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
};

pub const WM_TRAY: u32 = WM_APP + 1;
/// Menu command id for the Exit item (WM_APP range is free for app use).
pub const WM_TRAY_EXIT: usize = (WM_APP + 2) as usize;
const ICON_UID: u32 = 1;
const IDI_MAIN: usize = 101; // MAKEINTRESOURCE(101): low word set, high word zero.

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
    copy_wtext(&utf16_units("SmartBlink"), &mut nid.szTip);
    if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
        return Err(windows::core::Error::from_win32());
    }
    Ok(())
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

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        m if m == WM_TRAY => {
            // LOWORD(lparam) = mouse message id.
            let mouse = (lparam.0 & 0xFFFF) as u32;
            if mouse == WM_LBUTTONUP || mouse == WM_RBUTTONUP {
                show_menu(hwnd);
            }
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

unsafe fn show_menu(hwnd: HWND) {
    let hmenu = match CreatePopupMenu() {
        Ok(h) => h,
        Err(_) => return,
    };
    if AppendMenuW(hmenu, MF_STRING, WM_TRAY_EXIT, w!("Exit")).is_err() {
        let _ = DestroyMenu(hmenu);
        return;
    }
    // TrackPopupMenu with TPM_RETURNCMD blocks until a command is chosen.
    let _ = SetForegroundWindow(hwnd);
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let cmd = TrackPopupMenu(
        hmenu,
        TPM_RIGHTBUTTON | TPM_RETURNCMD,
        pt.x,
        pt.y,
        0,
        hwnd,
        None,
    );
    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(hmenu);
    if cmd.0 == WM_TRAY_EXIT as i32 {
        let _ = DestroyWindow(hwnd);
    }
}
