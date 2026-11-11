use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

pub mod runtime;

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{Context, Timer};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::dock::{self, DockItem};
use crate::win;
use crate::win::taskbar::TaskbarRuntime;
use crate::win::tray::TrayEvent;

/// Owns the active configuration, the app list, and the application-level commands.
/// The dock view renders this state and delegates commands back here.
pub struct AppController {
    config: Config,
    items: Vec<DockItem>,
    config_path: PathBuf,
    taskbar: Rc<RefCell<TaskbarRuntime>>,
}

impl AppController {
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor wires independent receivers and handles at the process boundary"
    )]
    pub fn new(
        cx: &mut Context<Self>,
        config: Config,
        config_path: PathBuf,
        mut config_updates: UnboundedReceiver<Config>,
        mut window_events: UnboundedReceiver<()>,
        hotkey_events: Option<UnboundedReceiver<()>>,
        tray_events: Option<UnboundedReceiver<TrayEvent>>,
        taskbar: Rc<RefCell<TaskbarRuntime>>,
    ) -> Self {
        let mut this = Self {
            config,
            items: Vec::new(),
            config_path,
            taskbar,
        };
        this.refresh();

        cx.spawn(async move |this, cx| {
            Timer::after(Duration::from_millis(0)).await;
            let _ = this.update(cx, |this, cx| {
                this.refresh();
                cx.notify();
            });
        })
        .detach();

        cx.spawn(async move |this, cx| {
            while let Some(config) = config_updates.next().await {
                if this
                    .update(cx, |this, cx| this.apply_config(config, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            while window_events.next().await.is_some() {
                Timer::after(Duration::from_millis(120)).await;
                while window_events.try_recv().is_ok() {}
                if this
                    .update(cx, |this, cx| {
                        this.refresh();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        if let Some(mut hotkey_events) = hotkey_events {
            cx.spawn(async move |_this, cx| {
                if hotkey_events.next().await.is_some() {
                    info!("quit hotkey received");
                    if let Err(error) = cx.update(|cx| cx.quit()) {
                        warn!(%error, "failed to request application quit");
                    }
                }
            })
            .detach();
        }

        if let Some(mut tray_events) = tray_events {
            cx.spawn(async move |this, cx| {
                while let Some(event) = tray_events.next().await {
                    if this
                        .update(cx, |this, cx| this.handle_tray_event(event, cx))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }

        this
    }

    fn handle_tray_event(&mut self, event: TrayEvent, cx: &mut Context<Self>) {
        match event {
            TrayEvent::OpenSettings => {
                if let Err(error) = crate::ui::open_settings_window(cx, self.config_path.clone()) {
                    warn!(%error, "failed to open settings from the tray");
                }
            }
            TrayEvent::ReloadConfig => self.reload_config(cx),
            TrayEvent::ToggleAutostart => self.toggle_autostart(),
            TrayEvent::RestoreTaskbar => {
                if let Err(error) = win::taskbar::restore_native(false) {
                    warn!(%error, "failed to restore the taskbar from the tray");
                }
            }
            TrayEvent::Quit => self.restore_and_quit(cx),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn config_path(&self) -> &std::path::Path {
        &self.config_path
    }

    pub fn items(&self) -> &[DockItem] {
        &self.items
    }

    /// Reloads the config file through the same path as the watcher. A failed reload
    /// keeps the last valid config.
    pub fn reload_config(&mut self, cx: &mut Context<Self>) {
        match Config::load_existing(&self.config_path) {
            Ok(loaded) => {
                for warning in &loaded.warnings {
                    warn!("{warning}");
                }
                self.apply_config(loaded.config, cx);
            }
            Err(error) => error!(%error, "config reload failed; keeping the last valid config"),
        }
    }

    fn apply_config(&mut self, config: Config, cx: &mut Context<Self>) {
        info!(
            background = %config.appearance.background,
            corner_radius = config.appearance.corner_radius,
            "config applied"
        );
        let taskbar_changed =
            self.config.behavior.hide_native_taskbar != config.behavior.hide_native_taskbar;
        self.config = config;

        if taskbar_changed {
            let enabled = self.config.behavior.hide_native_taskbar;
            let result = if enabled {
                self.taskbar.borrow_mut().enable()
            } else {
                self.taskbar.borrow_mut().disable()
            };
            match result {
                Ok(()) => info!(enabled, "taskbar mode updated"),
                Err(error) => error!(%error, enabled, "failed to update the taskbar mode"),
            }
        }

        self.refresh();
        cx.notify();
    }

    fn refresh(&mut self) {
        let started = std::time::Instant::now();
        match win::windows_enum::list_app_windows() {
            Ok(windows) => {
                self.items = dock::merge(
                    &self.config.pinned,
                    &windows,
                    self.config.behavior.show_running_apps,
                );
                debug!(
                    items = self.items.len(),
                    elapsed_ms = started.elapsed().as_millis(),
                    "app list refreshed"
                );
            }
            Err(error) => error!(%error, "failed to enumerate app windows"),
        }
    }

    pub fn activate_or_launch(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(index) else {
            return;
        };

        if item.is_running {
            if let Some(hwnd) = item.target_window()
                && let Err(error) = win::windows_enum::activate_or_minimize(hwnd)
            {
                warn!(%error, app = %item.name, "failed to activate app");
            }
        } else if item.is_pinned {
            let path = item.key.to_string_lossy();
            if let Err(error) = win::launch::launch(&path, &item.args) {
                warn!(%error, app = %item.name, "failed to launch app");
            }
        }

        self.refresh();
        cx.notify();
    }

    /// Middle-click launches a pinned app again instead of activating its window.
    pub fn launch_second_instance(&mut self, index: usize) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        if !item.is_pinned {
            return;
        }

        let path = item.key.to_string_lossy();
        if let Err(error) = win::launch::launch(&path, &item.args) {
            warn!(%error, app = %item.name, "failed to launch a second instance");
        }
        self.refresh();
    }

    pub fn toggle_autostart(&mut self) {
        let enabled = match win::autostart::is_enabled() {
            Ok(enabled) => enabled,
            Err(error) => {
                warn!(%error, "failed to query autostart state");
                return;
            }
        };

        let result = if enabled {
            win::autostart::disable()
        } else {
            win::autostart::enable()
        };
        match result {
            Ok(()) => info!(enabled = !enabled, "autostart updated"),
            Err(error) => warn!(%error, "failed to update autostart"),
        }
    }

    pub fn restore_and_quit(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = win::taskbar::restore_native(true) {
            warn!(%error, "failed to restore the taskbar before quitting");
        }
        cx.quit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_state_starts_empty_and_keeps_the_initial_config() {
        // The controller owns data that the view reads; construction is covered by the
        // application smoke path. This test only guards the stable accessors.
        let config = Config::default();
        let items: Vec<DockItem> = Vec::new();
        let controller = AppController {
            config: config.clone(),
            items,
            config_path: PathBuf::from("config.toml"),
            taskbar: Rc::new(RefCell::new(TaskbarRuntime::default())),
        };
        assert_eq!(controller.config(), &config);
        assert!(controller.items().is_empty());
    }
}
