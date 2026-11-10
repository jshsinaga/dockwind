#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod app;
mod config;
mod dock;
mod logging;
mod ui;
mod win;

fn main() {
    // The recovery command must stay first and independent of the mutex, the config,
    // and GPUI so a crashed or killed instance can always be recovered.
    if std::env::args().any(|argument| argument == "--restore-taskbar") {
        if let Err(error) = win::taskbar::restore_native_recovery() {
            eprintln!("failed to restore the native taskbar: {error:#}");
            std::process::exit(1);
        }
        return;
    }

    if std::env::args().any(|argument| argument == "--enable-autostart") {
        if let Err(error) = win::autostart::enable() {
            eprintln!("failed to enable autostart: {error:#}");
            std::process::exit(1);
        }
        return;
    }

    if std::env::args().any(|argument| argument == "--disable-autostart") {
        if let Err(error) = win::autostart::disable() {
            eprintln!("failed to disable autostart: {error:#}");
            std::process::exit(1);
        }
        return;
    }

    if std::env::args().any(|argument| argument == "--settings") {
        let path = match config::default_path() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("failed to resolve the config path: {error:#}");
                std::process::exit(1);
            }
        };
        if let Err(error) = win::settings::open_blocking(path) {
            eprintln!("failed to open settings: {error:#}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(error) = app::runtime::run() {
        eprintln!("dockwind failed: {error:#}");
        std::process::exit(1);
    }
}
