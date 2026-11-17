#![cfg_attr(windows, windows_subsystem = "windows")]

mod about;
mod autostart;
mod config;
mod controller;
mod engine;
mod i18n;
mod icon;
mod instance;
#[cfg_attr(not(windows), allow(dead_code))]
mod packet;
mod proto;
mod tray;
mod ui;
mod update;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        #[cfg(target_os = "linux")]
        Some("--engine") => std::process::exit(engine::linux::engine_main()),
        Some("--window") => {
            if let Err(e) = ui::run() {
                eprintln!("pencere açılamadı: {e}");
                std::process::exit(1);
            }
        }
        _ => {
            let hidden = args.iter().any(|a| a == "--hidden");
            if args.iter().any(|a| a == "--updated") {
                let t = std::time::Instant::now();
                while instance::signal_existing() && t.elapsed() < std::time::Duration::from_secs(15) {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            } else if instance::signal_existing() {
                return;
            }
            controller::run(hidden);
        }
    }
}
