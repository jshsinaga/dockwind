use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{AppContext, Application, Entity};
use tracing::{error, info, warn};
use tracing_appender::non_blocking::WorkerGuard;

use crate::app::AppController;
use crate::config::{self, Config, ConfigWatcher};
use crate::logging;
use crate::win::{
    self,
    events::EventWorker,
    hotkey::HotkeyWorker,
    single_instance::SingleInstanceGuard,
    taskbar::TaskbarRuntime,
    tray::{TrayEvent, TrayIcon},
};

pub fn run() -> Result<()> {
    let mut runtime = Runtime::start()?;
    let app_error = runtime.run_application();
    let cleanup_errors = runtime.shutdown();
    finish(app_error, cleanup_errors)
}

/// Owns every startup resource that must outlive the GPUI event loop and releases
/// them in a safe order.
struct Runtime {
    _single_instance: SingleInstanceGuard,
    config_path: PathBuf,
    config: Option<Config>,
    config_updates: Option<UnboundedReceiver<Config>>,
    window_events: Option<UnboundedReceiver<()>>,
    hotkey_events: Option<UnboundedReceiver<()>>,
    tray_events: Option<UnboundedReceiver<TrayEvent>>,
    config_watcher: Option<ConfigWatcher>,
    events: Option<EventWorker>,
    hotkey: Option<HotkeyWorker>,
    tray: Option<TrayIcon>,
    taskbar: Rc<RefCell<TaskbarRuntime>>,
    // Declared last so it is dropped last: fields drop in declaration order, and
    // cleanup diagnostics must reach the log writer.
    _logging: WorkerGuard,
}

impl Runtime {
    fn start() -> Result<Self> {
        let logging = logging::init()?;
        let single_instance = SingleInstanceGuard::acquire()?;
        let config_path = config::default_path()?;
        let loaded = config::Config::load_or_create(&config_path)?;

        for warning in &loaded.warnings {
            warn!("{warning}");
        }
        info!(path = %config_path.display(), "config loaded");

        let hide_taskbar = loaded.config.behavior.hide_native_taskbar;

        // The quit hotkey and the taskbar recovery handlers are registered before the
        // taskbar is hidden so a failed startup never strands the user without one.
        let (hotkey, hotkey_events) = match win::hotkey::spawn(&loaded.config.behavior.quit_hotkey)
        {
            Ok((worker, receiver)) => {
                info!(
                    hotkey = %loaded.config.behavior.quit_hotkey,
                    "quit hotkey registered"
                );
                (Some(worker), Some(receiver))
            }
            Err(error) if hide_taskbar => return Err(error),
            Err(error) => {
                warn!(%error, "global quit hotkey unavailable");
                (None, None)
            }
        };

        let (events, window_events) = win::events::spawn()?;

        let taskbar = Rc::new(RefCell::new(TaskbarRuntime::default()));
        if hide_taskbar {
            taskbar.borrow_mut().enable()?;
        }

        let (config_watcher, config_updates) = config::watch(&config_path, loaded.config.clone())?;

        // A missing tray icon is a logged degradation. The dock and its config
        // remain usable, so it must not fail startup.
        let (tray, tray_events) = match win::tray::spawn() {
            Ok((tray, events)) => (Some(tray), Some(events)),
            Err(error) => {
                warn!(%error, "tray icon unavailable");
                (None, None)
            }
        };

        Ok(Self {
            _logging: logging,
            _single_instance: single_instance,
            config_path,
            config: Some(loaded.config),
            config_updates: Some(config_updates),
            window_events: Some(window_events),
            hotkey_events,
            tray_events,
            config_watcher: Some(config_watcher),
            events: Some(events),
            hotkey,
            tray,
            taskbar,
        })
    }

    /// Runs GPUI and returns the startup error, if any. Window creation failures are
    /// reported through a shared slot and end the event loop instead of panicking.
    fn run_application(&mut self) -> Option<anyhow::Error> {
        let Some(config) = self.config.take() else {
            return Some(anyhow!("runtime configuration was already consumed"));
        };
        let Some(config_updates) = self.config_updates.take() else {
            return Some(anyhow!("config updates were already consumed"));
        };
        let Some(window_events) = self.window_events.take() else {
            return Some(anyhow!("window events were already consumed"));
        };

        let config_path = self.config_path.clone();
        let hotkey_events = self.hotkey_events.take();
        let tray_events = self.tray_events.take();
        let taskbar = Rc::clone(&self.taskbar);
        let startup_error: Arc<Mutex<Option<anyhow::Error>>> = Arc::new(Mutex::new(None));
        let error_slot = Arc::clone(&startup_error);
        // Holds a strong controller reference for the whole event loop, independent of
        // the dock view and of weak references inside spawned tasks.
        let controller_slot: Rc<RefCell<Option<Entity<AppController>>>> =
            Rc::new(RefCell::new(None));
        let retained_controller = Rc::clone(&controller_slot);

        Application::new().run(move |cx| {
            let controller = cx.new(|cx| {
                AppController::new(
                    cx,
                    config,
                    config_path,
                    config_updates,
                    window_events,
                    hotkey_events,
                    tray_events,
                    taskbar,
                )
            });
            *retained_controller.borrow_mut() = Some(controller.clone());
            let config = controller.read(cx).config().clone();
            if let Err(error) = crate::ui::open_dock_window(cx, controller, &config) {
                error!(%error, "failed to open the dock window");
                *error_slot
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error);
                cx.quit();
            }
        });

        // The application lifetime has ended; release the retained strong reference.
        let _ = controller_slot.borrow_mut().take();

        startup_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    /// Stops the taskbar watcher and all workers, restoring the taskbar first. Logs
    /// stay open until the runtime itself is dropped.
    fn shutdown(&mut self) -> Vec<anyhow::Error> {
        let mut errors = Vec::new();

        errors.extend(self.taskbar.borrow_mut().shutdown());

        if let Some(mut watcher) = self.config_watcher.take()
            && let Err(error) = watcher.shutdown()
        {
            errors.push(anyhow!("config watcher shutdown failed: {error}"));
        }

        if let Some(events) = self.events.as_mut()
            && let Err(error) = events.shutdown()
        {
            errors.push(error.context("window event worker shutdown failed"));
        }

        if let Some(hotkey) = self.hotkey.as_mut()
            && let Err(error) = hotkey.shutdown()
        {
            errors.push(error.context("hotkey worker shutdown failed"));
        }

        if let Some(mut tray) = self.tray.take()
            && let Err(error) = tray.shutdown()
        {
            errors.push(error.context("tray icon shutdown failed"));
        }

        errors
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        for error in self.shutdown() {
            error!(%error, "failed to release a runtime resource on drop");
        }
    }
}

/// Cleanup failures never replace the startup error that caused the shutdown.
fn finish(app_error: Option<anyhow::Error>, cleanup_errors: Vec<anyhow::Error>) -> Result<()> {
    for error in &cleanup_errors {
        error!(%error, "resource shutdown failed");
    }

    if let Some(error) = app_error {
        return Err(error);
    }

    match cleanup_errors.into_iter().next() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_error_is_preserved_when_cleanup_fails() {
        let error = finish(
            Some(anyhow!("dock window failed")),
            vec![anyhow!("taskbar restore failed")],
        )
        .expect_err("startup error must be returned");
        assert_eq!(error.to_string(), "dock window failed");
    }

    #[test]
    fn cleanup_error_is_returned_when_startup_succeeded() {
        let error = finish(None, vec![anyhow!("hotkey shutdown failed")])
            .expect_err("cleanup error must be returned");
        assert_eq!(error.to_string(), "hotkey shutdown failed");
    }

    #[test]
    fn successful_shutdown_returns_ok() {
        finish(None, Vec::new()).expect("clean shutdown");
    }
}
