//! SmartBlink — wrong-layout typing detection and one-gesture fix.
//! M1: single instance, tray icon, config read/write. No hooks yet (M2+).

#![windows_subsystem = "windows"]

mod config;
mod single_instance;
mod tray;

fn main() {
    let _instance = match single_instance::acquire() {
        Some(guard) => guard,
        None => {
            // Another instance owns the mutex: exit quietly before touching config.
            return;
        }
    };

    let _cfg = config::load_or_create();

    let _ = tray::run();
}
