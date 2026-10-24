//! SmartBlink — wrong-layout typing detection and one-gesture fix.
//! M2: single instance, config, low-level keyboard/mouse hooks, lock-free SPSC ring,
//! foreground process immunity, worker word buffer, tray icon + Pause/Resume + Exit.

#![windows_subsystem = "windows"]

mod config;
mod hooks;
mod immunity;
mod layout;
mod profile;
mod ring;
mod single_instance;
mod tray;
mod worker;

fn main() {
    let _instance = match single_instance::acquire() {
        Some(guard) => guard,
        None => {
            // Another instance owns the mutex: exit quietly before touching config.
            return;
        }
    };

    let cfg = config::load_or_create();

    let _worker = match worker::start(cfg.blacklist) {
        Ok(guard) => guard,
        Err(_) => return,
    };

    let _hooks = match hooks::start() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    let _ = tray::run();
}
