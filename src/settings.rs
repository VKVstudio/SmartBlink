//! Pure Win32 Settings window (`SmartBlinkSettingsWnd`, M9, ARCHITECTURE.md §3.8, §3.9).
//! Opened via the Tray menu ("Settings...") or by double-clicking the tray icon.

use crate::autostart;
use crate::config::{self, Config, FixTrigger, LedMode, NotifyMode};
use crate::hooks;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, HBRUSH, DEFAULT_GUI_FONT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetDlgItem, GetSystemMetrics, IsWindow,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowTextW, ShowWindow, HMENU,
    SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNORMAL, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND,
    WM_CREATE, WM_DESTROY, WM_SETFONT, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_OVERLAPPED, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE,
};

static SETTINGS_HWND: AtomicIsize = AtomicIsize::new(0);
static CLASS_REGISTERED: std::sync::Once = std::sync::Once::new();

const IDC_STATUS_LABEL: i32 = 1001;
const IDC_BTN_NOTIFY: i32 = 1002;
const IDC_BTN_LED: i32 = 1003;
const IDC_BTN_FIX: i32 = 1004;
const IDC_BTN_AUTOSTART: i32 = 1005;
const IDC_BTN_ELEVATED_TASK: i32 = 1006;
const IDC_BTN_OPEN_CONFIG: i32 = 1007;
const IDC_BTN_CLOSE: i32 = 1008;

/// Open or bring to foreground the Win32 Settings window.
pub fn show_settings_window() {
    unsafe {
        let existing = HWND(SETTINGS_HWND.load(Ordering::Acquire) as *mut _);
        if !existing.0.is_null() && IsWindow(existing).as_bool() {
            refresh_controls(existing);
            let _ = ShowWindow(existing, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(existing);
            return;
        }

        let Ok(hmod) = GetModuleHandleW(None) else {
            return;
        };
        let hinst = HINSTANCE(hmod.0 as _);
        let cls = w!("SmartBlinkSettingsWnd");

        CLASS_REGISTERED.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(settings_wnd_proc),
                hInstance: hinst,
                hbrBackground: HBRUSH((5 + 1) as *mut _), // COLOR_WINDOW + 1
                lpszClassName: cls,
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });

        let wnd_w = 620;
        let wnd_h = 430;
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let pos_x = ((screen_w - wnd_w) / 2).max(40);
        let pos_y = ((screen_h - wnd_h) / 2).max(40);

        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE;
        if let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            cls,
            w!("SmartBlink — Settings"),
            style,
            pos_x,
            pos_y,
            wnd_w,
            wnd_h,
            None,
            None,
            hinst,
            None,
        ) {
            SETTINGS_HWND.store(hwnd.0 as isize, Ordering::Release);
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

pub fn next_notify_mode(mode: NotifyMode) -> NotifyMode {
    match mode {
        NotifyMode::Both => NotifyMode::Sound,
        NotifyMode::Sound => NotifyMode::Led,
        NotifyMode::Led => NotifyMode::Silent,
        NotifyMode::Silent => NotifyMode::Both,
    }
}

pub fn next_led_mode(mode: LedMode) -> LedMode {
    match mode {
        LedMode::ScrollLock => LedMode::CapsLockGuarded,
        LedMode::CapsLockGuarded => LedMode::ScrollLock,
    }
}

pub fn next_fix_trigger(trigger: FixTrigger) -> FixTrigger {
    match trigger {
        FixTrigger::DoubleLeftShift => FixTrigger::DoubleLeftCtrl,
        FixTrigger::DoubleLeftCtrl => FixTrigger::PauseBreak,
        FixTrigger::PauseBreak => FixTrigger::DoubleLeftShift,
    }
}

pub fn format_notify_button(mode: NotifyMode) -> String {
    let label = match mode {
        NotifyMode::Both => "Both (Audio Click + LED Blink)",
        NotifyMode::Sound => "Audio Click Only",
        NotifyMode::Led => "LED Blink Only",
        NotifyMode::Silent => "Silent (Tray Only)",
    };
    format!("Alert Mode:  {label}")
}

pub fn format_led_button(mode: LedMode) -> String {
    let label = match mode {
        LedMode::ScrollLock => "Scroll Lock (Safe Default)",
        LedMode::CapsLockGuarded => "Caps Lock (Guarded >= 80 ms gap)",
    };
    format!("LED Channel:  {label}")
}

pub fn format_fix_button(trigger: FixTrigger) -> String {
    let label = match trigger {
        FixTrigger::DoubleLeftShift => "Double Left Shift (<= 300 ms)",
        FixTrigger::DoubleLeftCtrl => "Double Left Ctrl (<= 300 ms)",
        FixTrigger::PauseBreak => "Pause / Break Key",
    };
    format!("Fix Gesture:  {label}")
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            create_controls(hwnd);
            refresh_controls(hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            match id {
                IDC_BTN_NOTIFY => {
                    config::update_runtime_config(|cfg| {
                        cfg.notify_mode = next_notify_mode(cfg.notify_mode);
                    });
                    refresh_controls(hwnd);
                }
                IDC_BTN_LED => {
                    config::update_runtime_config(|cfg| {
                        cfg.led_mode = next_led_mode(cfg.led_mode);
                    });
                    refresh_controls(hwnd);
                }
                IDC_BTN_FIX => {
                    config::update_runtime_config(|cfg| {
                        cfg.fix_trigger = next_fix_trigger(cfg.fix_trigger);
                    });
                    refresh_controls(hwnd);
                }
                IDC_BTN_AUTOSTART => {
                    let currently_on = autostart::is_registry_autostart_enabled();
                    let next_on = !currently_on;
                    if autostart::set_registry_autostart(next_on) {
                        config::update_runtime_config(|cfg| {
                            cfg.autostart = next_on;
                        });
                    }
                    refresh_controls(hwnd);
                }
                IDC_BTN_ELEVATED_TASK => {
                    if autostart::is_elevated_task_registered() {
                        let _ = autostart::unregister_elevated_task_interactive(hwnd);
                    } else {
                        let _ = autostart::register_elevated_task_interactive(hwnd);
                    }
                    refresh_controls(hwnd);
                }
                IDC_BTN_OPEN_CONFIG => {
                    let path = config::config_path();
                    let path_w: Vec<u16> = path
                        .to_string_lossy()
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();
                    let _ = ShellExecuteW(
                        hwnd,
                        w!("open"),
                        PCWSTR(path_w.as_ptr()),
                        None,
                        None,
                        SW_SHOWNORMAL,
                    );
                }
                IDC_BTN_CLOSE => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            SETTINGS_HWND.store(0, Ordering::Release);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn create_controls(hwnd: HWND) {
    let Ok(hmod) = GetModuleHandleW(None) else {
        return;
    };
    let hinst = HINSTANCE(hmod.0 as _);
    let font = GetStockObject(DEFAULT_GUI_FONT);

    let add_child = |cls: PCWSTR, text: PCWSTR, style: WINDOW_STYLE, y: i32, h: i32, id: i32| {
        if let Ok(child) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            cls,
            text,
            WS_CHILD | WS_VISIBLE | style,
            24,
            y,
            556,
            h,
            hwnd,
            HMENU(id as isize as *mut _),
            hinst,
            None,
        ) {
            let _ = SendMessageW(child, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }
    };

    add_child(w!("STATIC"), w!(""), WINDOW_STYLE(0), 18, 48, IDC_STATUS_LABEL);
    add_child(w!("BUTTON"), w!(""), WS_TABSTOP, 76, 40, IDC_BTN_NOTIFY);
    add_child(w!("BUTTON"), w!(""), WS_TABSTOP, 126, 40, IDC_BTN_LED);
    add_child(w!("BUTTON"), w!(""), WS_TABSTOP, 176, 40, IDC_BTN_FIX);
    add_child(w!("BUTTON"), w!(""), WS_TABSTOP, 226, 40, IDC_BTN_AUTOSTART);
    add_child(
        w!("BUTTON"),
        w!(""),
        WS_TABSTOP,
        276,
        40,
        IDC_BTN_ELEVATED_TASK,
    );
    add_child(
        w!("BUTTON"),
        w!("Open config.toml in Editor"),
        WS_TABSTOP,
        330,
        38,
        IDC_BTN_OPEN_CONFIG,
    );
}

unsafe fn refresh_controls(hwnd: HWND) {
    let cfg: Config = config::get_runtime_config();
    let paused = hooks::is_paused();
    let elevated = autostart::is_process_elevated();
    let run_enabled = autostart::is_registry_autostart_enabled();
    let task_registered = autostart::is_elevated_task_registered();

    let status_text = format!(
        "Engine: {}   |   Selection Hotkey: Ctrl+Alt+Z\r\nPrivilege: {}   |   Exclusions: {} apps",
        if paused { "Paused" } else { "Active" },
        if elevated {
            "Elevated (Admin / UIPI active)"
        } else {
            "Standard user (not running elevated)"
        },
        cfg.blacklist.len()
    );
    set_dlg_text(hwnd, IDC_STATUS_LABEL, &status_text);
    set_dlg_text(hwnd, IDC_BTN_NOTIFY, &format_notify_button(cfg.notify_mode));
    set_dlg_text(hwnd, IDC_BTN_LED, &format_led_button(cfg.led_mode));
    set_dlg_text(hwnd, IDC_BTN_FIX, &format_fix_button(cfg.fix_trigger));

    let autostart_label = if run_enabled {
        "Run at Windows Startup (HKCU):  Enabled (Click to Disable)"
    } else {
        "Run at Windows Startup (HKCU):  Disabled (Click to Enable)"
    };
    set_dlg_text(hwnd, IDC_BTN_AUTOSTART, autostart_label);

    let task_label = if task_registered {
        "Admin Windows Task (ONLOGON Highest):  Registered (Click to Remove)"
    } else {
        "Admin Windows Task (ONLOGON Highest):  Not Registered (Click to Create)"
    };
    set_dlg_text(hwnd, IDC_BTN_ELEVATED_TASK, task_label);
}

unsafe fn set_dlg_text(hwnd: HWND, id: i32, text: &str) {
    if let Ok(ctrl) = GetDlgItem(hwnd, id) {
        if !ctrl.0.is_null() {
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let _ = SetWindowTextW(ctrl, PCWSTR(wide.as_ptr()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_cycle_helpers_cover_all_variants() {
        assert_eq!(next_notify_mode(NotifyMode::Both), NotifyMode::Sound);
        assert_eq!(next_notify_mode(NotifyMode::Sound), NotifyMode::Led);
        assert_eq!(next_notify_mode(NotifyMode::Led), NotifyMode::Silent);
        assert_eq!(next_notify_mode(NotifyMode::Silent), NotifyMode::Both);

        assert_eq!(next_led_mode(LedMode::ScrollLock), LedMode::CapsLockGuarded);
        assert_eq!(next_led_mode(LedMode::CapsLockGuarded), LedMode::ScrollLock);

        assert_eq!(
            next_fix_trigger(FixTrigger::DoubleLeftShift),
            FixTrigger::DoubleLeftCtrl
        );
        assert_eq!(
            next_fix_trigger(FixTrigger::DoubleLeftCtrl),
            FixTrigger::PauseBreak
        );
        assert_eq!(
            next_fix_trigger(FixTrigger::PauseBreak),
            FixTrigger::DoubleLeftShift
        );
    }
}
