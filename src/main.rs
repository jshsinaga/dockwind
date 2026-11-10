mod app_state;
mod autohide;
mod config;
mod logging;
mod theme;
mod ui;
mod win;

use gpui::{App, Application};

fn main() {
    if std::env::args().any(|argument| argument == "--restore-taskbar") {
        if let Err(error) = win::taskbar::restore_native(true) {
            eprintln!("failed to restore the native taskbar: {error:#}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(error) = run() {
        eprintln!("dockwind failed: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let _log_guard = logging::init()?;
    let _single_instance = win::single_instance::SingleInstanceGuard::acquire()?;
    let config_path = config::default_path()?;
    let loaded = config::Config::load_or_create(&config_path)?;

    for warning in &loaded.warnings {
        tracing::warn!("{warning}");
    }

    tracing::info!(
        path = %config_path.display(),
        config = ?loaded.config,
        "config loaded"
    );

    let hide_taskbar = loaded.config.behavior.hide_native_taskbar;
    let _taskbar_guard = if hide_taskbar {
        win::taskbar::install_panic_hook();
        if let Err(error) = win::taskbar::install_console_handler() {
            tracing::warn!(%error, "console shutdown handler unavailable");
        }
        win::taskbar::install_watch()?;
        Some(win::taskbar::TaskbarGuard::new()?)
    } else {
        None
    };

    let hotkey_events = match win::hotkey::spawn(&loaded.config.behavior.quit_hotkey) {
        Ok(receiver) => {
            tracing::info!(
                hotkey = %loaded.config.behavior.quit_hotkey,
                "quit hotkey registered"
            );
            Some(receiver)
        }
        Err(error) => {
            if hide_taskbar {
                return Err(error);
            }
            tracing::warn!(%error, "global quit hotkey unavailable");
            None
        }
    };

    let config_updates = config::watch(&config_path, loaded.config.clone())?;
    let window_events = win::events::spawn()?;

    Application::new().run(move |cx: &mut App| {
        ui::open_dock_window(
            cx,
            loaded.config,
            config_updates,
            window_events,
            hotkey_events,
        )
        .expect("failed to open the dock window");
    });

    Ok(())
}
