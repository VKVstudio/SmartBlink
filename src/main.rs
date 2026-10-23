//! SmartBlink — wrong-layout typing detection and one-gesture fix.
//! M1: single instance, tray icon, config read/write. No hooks yet (M2+).

#![windows_subsystem = "windows"]

mod config;
mod single_instance;
mod tray;

use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

fn main() {
    // SAFETY: single-threaded init before any thread uses COM.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

    let cfg = config::load_or_create();

    let _instance = match single_instance::acquire() {
        Some(guard) => guard,
        None => {
            // Another instance owns the mutex: exit quietly.
            unsafe { CoUninitialize() };
            return;
        }
    };

    if tray::run(&cfg).is_err() {
        // Tray registration failed (e.g. Shell_NotifyIcon refusal): exit without crash.
    }

    unsafe { CoUninitialize() };
}
