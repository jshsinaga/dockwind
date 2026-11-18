use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

pub mod runtime;

use futures::channel::mpsc::{Receiver, Sender, UnboundedReceiver, channel};
use futures::{FutureExt, StreamExt};
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
    windows: Vec<dock::WindowInfo>,
    refresh_window_list: Sender<()>,
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
        window_events: Receiver<()>,
        hotkey_events: Option<UnboundedReceiver<()>>,
        tray_events: Option<UnboundedReceiver<TrayEvent>>,
        taskbar: Rc<RefCell<TaskbarRuntime>>,
    ) -> Self {
        let (refresh_window_list, refresh_requests) = channel(1);
        let this = Self {
            items: dock::merge(&config.pinned, &[], config.behavior.show_running_apps),
            config,
            windows: Vec::new(),
            refresh_window_list,
            config_path,
            taskbar,
        };

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
            let mut refresh_signals = futures::stream::select(window_events, refresh_requests);
            loop {
                let started = std::time::Instant::now();
                let result = cx
                    .background_executor()
                    .spawn(async { win::windows_enum::list_app_windows() })
                    .await;
                if this
                    .update(cx, |this, cx| match result {
                        Ok(windows) => {
                            this.windows = windows;
                            if this.rebuild_items() {
                                debug!(
                                    items = this.items.len(),
                                    elapsed_ms = started.elapsed().as_millis(),
                                    "app list refreshed"
                                );
                                cx.notify();
                            }
                        }
                        Err(error) => error!(%error, "failed to enumerate app windows"),
                    })
                    .is_err()
                {
                    break;
                }

                futures::select! {
                    signal = refresh_signals.next().fuse() => {
                        if signal.is_none() {
                            break;
                        }
                        Timer::after(Duration::from_millis(50)).await;
                        // Each of the two bounded channels holds at most two notifications.
                        for _ in 0..4 {
                            if !matches!(refresh_signals.next().now_or_never(), Some(Some(()))) {
                                break;
                            }
                        }
                    }
                    _ = FutureExt::fuse(Timer::after(Duration::from_millis(250))) => {}
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

    pub fn use_pinned_layout(&mut self, name: &str, cx: &mut Context<Self>) {
        match Config::use_pinned_layout(&self.config_path, name) {
            Ok(loaded) => self.apply_config(loaded.config, cx),
            Err(error) => warn!(%error, layout = name, "failed to switch pinned app layout"),
        }
    }

    fn apply_config(&mut self, config: Config, cx: &mut Context<Self>) {
        if self.config == config {
            return;
        }
        info!(
            background = %config.appearance.background,
            corner_radius = config.appearance.corner_radius,
            "config applied"
        );
        let taskbar_changed =
            self.config.behavior.hide_native_taskbar != config.behavior.hide_native_taskbar;
        let app_list_changed = self.config.pinned != config.pinned
            || self.config.behavior.show_running_apps != config.behavior.show_running_apps;
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

        if app_list_changed {
            self.rebuild_items();
        }
        cx.notify();
    }

    fn rebuild_items(&mut self) -> bool {
        let items = dock::merge(
            &self.config.pinned,
            &self.windows,
            self.config.behavior.show_running_apps,
        );
        if items == self.items {
            return false;
        }
        self.items = items;
        true
    }

    fn current_item(&self, rendered: &DockItem) -> Option<&DockItem> {
        self.items.iter().find(|item| {
            item.key == rendered.key
                && item.is_pinned == rendered.is_pinned
                && item.args == rendered.args
        })
    }

    pub fn activate_or_launch(&mut self, rendered: &DockItem) {
        let Some(item) = self.current_item(rendered) else {
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

        let _ = self.refresh_window_list.try_send(());
    }

    /// Middle-click launches a pinned app again instead of activating its window.
    pub fn launch_second_instance(&mut self, rendered: &DockItem) {
        let Some(item) = self.current_item(rendered) else {
            return;
        };
        if !item.is_pinned {
            return;
        }

        let path = item.key.to_string_lossy();
        if let Err(error) = win::launch::launch(&path, &item.args) {
            warn!(%error, app = %item.name, "failed to launch a second instance");
        }
        let _ = self.refresh_window_list.try_send(());
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
            windows: Vec::new(),
            refresh_window_list: channel(1).0,
            config_path: PathBuf::from("config.toml"),
            taskbar: Rc::new(RefCell::new(TaskbarRuntime::default())),
        };
        assert_eq!(controller.config(), &config);
        assert!(controller.items().is_empty());
    }

    #[test]
    fn click_resolves_the_same_app_after_items_are_reordered() {
        let mut config = Config::default();
        config.pinned = vec![
            crate::config::PinnedApp {
                name: "Editor profile".to_string(),
                path: "editor.exe".to_string(),
                args: vec!["--profile".to_string(), "work".to_string()],
            },
            crate::config::PinnedApp {
                name: "Editor".to_string(),
                path: "editor.exe".to_string(),
                args: Vec::new(),
            },
        ];
        let rendered = dock::merge(&config.pinned, &[], true);
        let mut items = rendered.clone();
        items.reverse();
        let mut controller = AppController {
            config,
            items,
            windows: Vec::new(),
            refresh_window_list: channel(1).0,
            config_path: PathBuf::from("config.toml"),
            taskbar: Rc::new(RefCell::new(TaskbarRuntime::default())),
        };

        assert_eq!(controller.current_item(&rendered[0]), Some(&rendered[0]));
        controller.items.pop();
        assert!(controller.current_item(&rendered[0]).is_none());
    }

    #[test]
    fn rebuild_returns_false_when_items_are_unchanged() {
        let config = Config::default();
        let mut controller = AppController {
            config,
            items: Vec::new(),
            windows: Vec::new(),
            refresh_window_list: channel(1).0,
            config_path: PathBuf::from("config.toml"),
            taskbar: Rc::new(RefCell::new(TaskbarRuntime::default())),
        };
        let changed = controller.rebuild_items();
        assert!(!changed);
    }
}
