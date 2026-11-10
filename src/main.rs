mod config;
mod logging;
mod theme;
mod ui;
mod win;

use gpui::{App, Application};

fn main() {
    if let Err(error) = run() {
        eprintln!("dockwind failed: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let _log_guard = logging::init()?;
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

    let config_updates = config::watch(&config_path, loaded.config.clone())?;

    Application::new().run(move |cx: &mut App| {
        ui::open_dock_window(cx, loaded.config, config_updates)
            .expect("failed to open the dock window");
    });

    Ok(())
}
