//! Tray icon, full lazy context menu, and message loop (M9, ARCHITECTURE.md §3.8, §3.9).
//! Uses Shell_NotifyIconW with a hidden top-level window so TaskbarCreated broadcasts arrive.

use crate::autostart;
use crate::config::{self, FixTrigger, LedMode, NotifyMode};
use crate::hooks;
use crate::settings;
use std::sync::atomic::{AtomicU32, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, FindWindowW, GetCursorPos, GetMessageW, GetSystemMetrics, LoadIconW,
    LoadImageW, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SetForegroundWindow, ShowWindow, TrackPopupMenu, TranslateMessage,
    HICON, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, MB_ICONINFORMATION, MB_OK, MF_CHECKED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, SM_CXSMICON, SM_CYSMICON,
    SW_HIDE, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP,
    WM_NULL, WM_RBUTTONUP, WNDCLASSW,
};

pub const WM_TRAY: u32 = WM_APP + 1;
pub const WM_TRAY_EXIT: usize = (WM_APP + 2) as usize;
pub const WM_TRAY_PAUSE_TOGGLE: usize = (WM_APP + 3) as usize;
pub const WM_TRAY_NOTIFY_BOTH: usize = (WM_APP + 10) as usize;
pub const WM_TRAY_NOTIFY_SOUND: usize = (WM_APP + 11) as usize;
pub const WM_TRAY_NOTIFY_LED: usize = (WM_APP + 12) as usize;
pub const WM_TRAY_NOTIFY_SILENT: usize = (WM_APP + 13) as usize;
pub const WM_TRAY_LED_SCROLL: usize = (WM_APP + 20) as usize;
pub const WM_TRAY_LED_CAPS: usize = (WM_APP + 21) as usize;
pub const WM_TRAY_FIX_LSHIFT: usize = (WM_APP + 30) as usize;
pub const WM_TRAY_FIX_LCTRL: usize = (WM_APP + 31) as usize;
pub const WM_TRAY_FIX_PAUSE: usize = (WM_APP + 32) as usize;
pub const WM_TRAY_AUTOSTART_TOGGLE: usize = (WM_APP + 40) as usize;
pub const WM_TRAY_ELEVATED_TASK_TOGGLE: usize = (WM_APP + 41) as usize;
pub const WM_TRAY_EXCLUDE_CURRENT: usize = (WM_APP + 42) as usize;
pub const WM_TRAY_SETTINGS: usize = (WM_APP + 50) as usize;
pub const WM_TRAY_ABOUT: usize = (WM_APP + 51) as usize;

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

unsafe fn update_tray_tooltip(hwnd: HWND, paused: bool) {
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
            if mouse == WM_LBUTTONDBLCLK {
                settings::show_settings_window();
            } else if mouse == WM_LBUTTONUP || mouse == WM_RBUTTONUP {
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
    let Ok(hmenu) = CreatePopupMenu() else {
        return;
    };
    let cfg = config::get_runtime_config();

    let pause_label = if hooks::is_paused() {
        w!("Resume Detection")
    } else {
        w!("Pause Detection")
    };
    let _ = AppendMenuW(hmenu, MF_STRING, WM_TRAY_PAUSE_TOGGLE, pause_label);
    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);

    // Alert Mode submenu
    if let Ok(alert_menu) = CreatePopupMenu() {
        let check = |cond: bool| if cond { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(
            alert_menu,
            MF_STRING | check(cfg.notify_mode == NotifyMode::Both),
            WM_TRAY_NOTIFY_BOTH,
            w!("Both (Audio Click + LED Blink)"),
        );
        let _ = AppendMenuW(
            alert_menu,
            MF_STRING | check(cfg.notify_mode == NotifyMode::Sound),
            WM_TRAY_NOTIFY_SOUND,
            w!("Audio Click Only"),
        );
        let _ = AppendMenuW(
            alert_menu,
            MF_STRING | check(cfg.notify_mode == NotifyMode::Led),
            WM_TRAY_NOTIFY_LED,
            w!("LED Blink Only"),
        );
        let _ = AppendMenuW(
            alert_menu,
            MF_STRING | check(cfg.notify_mode == NotifyMode::Silent),
            WM_TRAY_NOTIFY_SILENT,
            w!("Silent"),
        );
        let _ = AppendMenuW(
            hmenu,
            MF_POPUP,
            alert_menu.0 as usize,
            w!("Alert Mode"),
        );
    }

    // LED Channel submenu
    if let Ok(led_menu) = CreatePopupMenu() {
        let check = |cond: bool| if cond { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(
            led_menu,
            MF_STRING | check(cfg.led_mode == LedMode::ScrollLock),
            WM_TRAY_LED_SCROLL,
            w!("Scroll Lock (Safe Default)"),
        );
        let _ = AppendMenuW(
            led_menu,
            MF_STRING | check(cfg.led_mode == LedMode::CapsLockGuarded),
            WM_TRAY_LED_CAPS,
            w!("Caps Lock (Guarded)"),
        );
        let _ = AppendMenuW(
            hmenu,
            MF_POPUP,
            led_menu.0 as usize,
            w!("LED Channel"),
        );
    }

    // Fix Gesture submenu
    if let Ok(fix_menu) = CreatePopupMenu() {
        let check = |cond: bool| if cond { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(
            fix_menu,
            MF_STRING | check(cfg.fix_trigger == FixTrigger::DoubleLeftShift),
            WM_TRAY_FIX_LSHIFT,
            w!("Double Left Shift"),
        );
        let _ = AppendMenuW(
            fix_menu,
            MF_STRING | check(cfg.fix_trigger == FixTrigger::DoubleLeftCtrl),
            WM_TRAY_FIX_LCTRL,
            w!("Double Left Ctrl"),
        );
        let _ = AppendMenuW(
            fix_menu,
            MF_STRING | check(cfg.fix_trigger == FixTrigger::PauseBreak),
            WM_TRAY_FIX_PAUSE,
            w!("Pause / Break"),
        );
        let _ = AppendMenuW(
            hmenu,
            MF_POPUP,
            fix_menu.0 as usize,
            w!("Fix Hotkey"),
        );
    }

    // Exclusions: add last foreground process
    let last_proc = config::get_last_foreground_process();
    if let Some(ref proc_name) = last_proc {
        let already = cfg.blacklist.iter().any(|b| b == proc_name);
        let label = if already {
            format!("Excluded: {proc_name}.exe\0")
        } else {
            format!("Exclude Process: {proc_name}.exe\0")
        };
        let label_w: Vec<u16> = label.encode_utf16().collect();
        let flags = if already {
            MF_STRING | MF_CHECKED | MF_GRAYED
        } else {
            MF_STRING
        };
        let _ = AppendMenuW(
            hmenu,
            flags,
            WM_TRAY_EXCLUDE_CURRENT,
            PCWSTR(label_w.as_ptr()),
        );
    }

    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);

    // Autostart & Elevated Task
    let run_checked = if autostart::is_registry_autostart_enabled() {
        MF_CHECKED
    } else {
        MF_UNCHECKED
    };
    let _ = AppendMenuW(
        hmenu,
        MF_STRING | run_checked,
        WM_TRAY_AUTOSTART_TOGGLE,
        w!("Run at Windows Startup"),
    );

    let task_checked = if autostart::is_elevated_task_registered() {
        MF_CHECKED
    } else {
        MF_UNCHECKED
    };
    let _ = AppendMenuW(
        hmenu,
        MF_STRING | task_checked,
        WM_TRAY_ELEVATED_TASK_TOGGLE,
        w!("Work Above Admin Windows (Elevated Task)"),
    );

    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);
    let _ = AppendMenuW(hmenu, MF_STRING, WM_TRAY_SETTINGS, w!("Settings..."));
    let _ = AppendMenuW(hmenu, MF_STRING, WM_TRAY_ABOUT, w!("About SmartBlink"));
    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);
    let _ = AppendMenuW(hmenu, MF_STRING, WM_TRAY_EXIT, w!("Exit"));

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

    handle_menu_command(hwnd, cmd.0 as usize);
    settings::refresh_if_open();
}

unsafe fn handle_menu_command(hwnd: HWND, cmd: usize) {
    match cmd {
        WM_TRAY_PAUSE_TOGGLE => {
            let paused = hooks::toggle_paused();
            update_tray_tooltip(hwnd, paused);
        }
        WM_TRAY_NOTIFY_BOTH => {
            config::update_runtime_config(|c| c.notify_mode = NotifyMode::Both);
        }
        WM_TRAY_NOTIFY_SOUND => {
            config::update_runtime_config(|c| c.notify_mode = NotifyMode::Sound);
        }
        WM_TRAY_NOTIFY_LED => {
            config::update_runtime_config(|c| c.notify_mode = NotifyMode::Led);
        }
        WM_TRAY_NOTIFY_SILENT => {
            config::update_runtime_config(|c| c.notify_mode = NotifyMode::Silent);
        }
        WM_TRAY_LED_SCROLL => {
            config::update_runtime_config(|c| c.led_mode = LedMode::ScrollLock);
        }
        WM_TRAY_LED_CAPS => {
            config::update_runtime_config(|c| c.led_mode = LedMode::CapsLockGuarded);
        }
        WM_TRAY_FIX_LSHIFT => {
            config::update_runtime_config(|c| c.fix_trigger = FixTrigger::DoubleLeftShift);
        }
        WM_TRAY_FIX_LCTRL => {
            config::update_runtime_config(|c| c.fix_trigger = FixTrigger::DoubleLeftCtrl);
        }
        WM_TRAY_FIX_PAUSE => {
            config::update_runtime_config(|c| c.fix_trigger = FixTrigger::PauseBreak);
        }
        WM_TRAY_EXCLUDE_CURRENT => {
            if let Some(proc_name) = config::get_last_foreground_process() {
                config::update_runtime_config(|c| {
                    if !c.blacklist.iter().any(|b| b == &proc_name) {
                        c.blacklist.push(proc_name);
                    }
                });
            }
        }
        WM_TRAY_AUTOSTART_TOGGLE => {
            let next = !autostart::is_registry_autostart_enabled();
            if autostart::set_registry_autostart(next) {
                config::update_runtime_config(|c| c.autostart = next);
            }
        }
        WM_TRAY_ELEVATED_TASK_TOGGLE => {
            if autostart::is_elevated_task_registered() {
                let _ = autostart::unregister_elevated_task_interactive(hwnd);
            } else {
                let _ = autostart::register_elevated_task_interactive(hwnd);
            }
        }
        WM_TRAY_SETTINGS => {
            settings::show_settings_window();
        }
        WM_TRAY_ABOUT => {
            let _ = MessageBoxW(
                hwnd,
                w!("SmartBlink — Wrong-layout typing detector & one-gesture fix.\n\n• 20 built-in language profiles\n• Net-zero LED blink + low-latency WASAPI click\n• Manual fix: Double Left Shift / Ctrl+Alt+Z selection\n• 100% RAM-only, zero telemetry\n\nPublisher: VKV Studio (vkvstudio.com)\nLicense: MIT"),
                w!("About SmartBlink"),
                MB_OK | MB_ICONINFORMATION,
            );
        }
        WM_TRAY_EXIT => {
            let _ = DestroyWindow(hwnd);
        }
        _ => {}
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
