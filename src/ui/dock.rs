use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    Bounds, ClickEvent, Context, Entity, MouseButton, Pixels, Render, Timer, Window, div,
    prelude::*, px, rgba,
};
use tracing::{debug, error, info, warn};

use super::app_item::render_app_item;
use super::clock::ClockView;
use crate::app_state::{self, DockItem};
use crate::autohide::{AutoHide, AutoHideSettings};
use crate::config::Config;
use crate::theme::Theme;
use crate::win::menu::MenuAction;
use crate::win::{self, icons::IconCache};

#[derive(Debug, Clone, Copy)]
pub struct DockGeometry {
    pub bounds: Bounds<Pixels>,
    pub trigger: Bounds<Pixels>,
    pub scale_factor: f32,
    pub hwnd: isize,
}

pub struct DockView {
    theme: Theme,
    blur: crate::config::BlurMode,
    clock: Entity<ClockView>,
    pinned: Vec<crate::config::PinnedApp>,
    items: Vec<DockItem>,
    icons: IconCache,
    icon_size: Pixels,
    geometry: DockGeometry,
    auto_hide: AutoHide,
    auto_hide_settings: AutoHideSettings,
    auto_hide_enabled: bool,
    hide_in_fullscreen: bool,
    menu_open: bool,
    progress: f32,
}

impl DockView {
    pub fn new(
        cx: &mut Context<Self>,
        theme: Theme,
        geometry: DockGeometry,
        config: Config,
        mut config_updates: UnboundedReceiver<Config>,
        mut window_events: UnboundedReceiver<()>,
        hotkey_events: Option<UnboundedReceiver<()>>,
    ) -> Self {
        let clock = cx.new(|cx| ClockView::new(cx, &config.clock));
        let auto_hide_enabled = config.behavior.auto_hide;
        let auto_hide = AutoHide::new(auto_hide_enabled);
        let progress = auto_hide.progress();
        let mut this = Self {
            theme,
            blur: config.appearance.blur,
            clock,
            pinned: config.pinned.clone(),
            items: Vec::new(),
            icons: IconCache::default(),
            icon_size: px(config.appearance.icon_size as f32),
            geometry,
            auto_hide,
            auto_hide_settings: auto_hide_settings(&config),
            auto_hide_enabled,
            hide_in_fullscreen: config.behavior.hide_in_fullscreen,
            menu_open: false,
            progress,
        };
        this.set_click_through(progress <= 0.001);
        this.refresh();

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

                let updated = this.update(cx, |this, cx| {
                    this.refresh();
                    cx.notify();
                });
                if updated.is_err() {
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

        let start = Instant::now();
        cx.spawn(async move |this, cx| {
            loop {
                let delay = this
                    .read_with(cx, |this, _| this.poll_interval())
                    .unwrap_or(Duration::from_millis(100));
                Timer::after(delay).await;
                let now = start.elapsed();
                if this.update(cx, |this, cx| this.tick(now, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        this
    }

    fn refresh(&mut self) {
        let started = std::time::Instant::now();
        match win::windows_enum::list_app_windows() {
            Ok(windows) => {
                let mut items = app_state::merge(&self.pinned, &windows);
                for item in &mut items {
                    item.icon = self.icons.get(&item.key, item.target_window());
                }
                self.items = items;
                debug!(
                    items = self.items.len(),
                    elapsed_ms = started.elapsed().as_millis(),
                    "app list refreshed"
                );
            }
            Err(error) => error!(%error, "failed to enumerate app windows"),
        }
    }

    fn tick(&mut self, now: Duration, cx: &mut Context<Self>) {
        let cursor = match win::cursor::cursor_pos(self.geometry.scale_factor) {
            Ok(cursor) => cursor,
            Err(error) => {
                warn!(%error, "failed to read the cursor position");
                return;
            }
        };

        let force_hidden = self.hide_in_fullscreen && win::cursor::fullscreen_active();
        let progress = self.auto_hide.update(
            now,
            cursor,
            self.geometry.bounds,
            self.geometry.trigger,
            self.auto_hide_settings,
            self.auto_hide_enabled,
            force_hidden,
            self.menu_open,
        );

        if (progress - self.progress).abs() > f32::EPSILON {
            self.progress = progress;
            self.set_click_through(progress <= 0.001);
            cx.notify();
        }
    }

    fn poll_interval(&self) -> Duration {
        if self.auto_hide_enabled && self.progress > 0.001 {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(100)
        }
    }

    fn set_click_through(&self, enabled: bool) {
        if let Err(error) = win::dwm::set_click_through(self.geometry.hwnd, enabled) {
            warn!(%error, "failed to set dock click-through");
        }
    }

    fn apply_config(&mut self, config: Config, cx: &mut Context<Self>) {
        let theme = match Theme::from_config(&config.appearance) {
            Ok(theme) => theme,
            Err(error) => {
                error!(%error, "config appearance update rejected");
                return;
            }
        };

        let blur_changed = self.blur != config.appearance.blur;
        self.blur = config.appearance.blur;
        self.theme = theme;
        self.icon_size = px(config.appearance.icon_size as f32);
        self.clock
            .update(cx, |clock, cx| clock.set_config(&config.clock, cx));

        if self.pinned != config.pinned {
            self.pinned = config.pinned.clone();
            self.refresh();
        }

        self.auto_hide_enabled = config.behavior.auto_hide;
        self.hide_in_fullscreen = config.behavior.hide_in_fullscreen;
        self.auto_hide_settings = auto_hide_settings(&config);
        if !config.behavior.auto_hide {
            self.progress = 1.0;
            self.set_click_through(false);
        }

        if blur_changed
            && let Err(error) = win::dwm::apply_backdrop_to_hwnd(self.geometry.hwnd, self.blur)
        {
            warn!(%error, "backdrop update failed; keeping transparent");
        }

        info!(
            corner_radius = config.appearance.corner_radius,
            background = %config.appearance.background,
            "dock appearance updated"
        );
        cx.notify();
    }

    fn reload_config(&mut self, cx: &mut Context<Self>) {
        let path = match crate::config::default_path() {
            Ok(path) => path,
            Err(error) => {
                error!(%error, "failed to resolve the config path");
                return;
            }
        };

        match Config::load_existing(&path) {
            Ok(loaded) => {
                for warning in &loaded.warnings {
                    warn!("{warning}");
                }
                self.apply_config(loaded.config, cx);
            }
            Err(error) => error!(%error, "config reload failed; keeping the last valid config"),
        }
    }

    fn toggle_autostart(&mut self) {
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

    fn on_context_menu(&mut self, cx: &mut Context<Self>) {
        let hwnd = self.geometry.hwnd;
        cx.spawn(async move |this, cx| {
            Timer::after(Duration::from_millis(0)).await;
            if this
                .update(cx, |this, cx| {
                    this.menu_open = true;
                    cx.notify();
                })
                .is_err()
            {
                return;
            }

            let result = win::menu::show(hwnd);
            let _ = this.update(cx, |this, cx| {
                this.menu_open = false;
                this.handle_menu_result(result, cx);
            });
        })
        .detach();
    }

    fn handle_menu_result(
        &mut self,
        result: anyhow::Result<Option<MenuAction>>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Some(MenuAction::ReloadConfig)) => self.reload_config(cx),
            Ok(Some(MenuAction::ToggleAutostart)) => self.toggle_autostart(),
            Ok(Some(MenuAction::RestoreAndQuit)) => {
                if let Err(error) = win::taskbar::restore_native(true) {
                    warn!(%error, "failed to restore the taskbar before quitting");
                }
                cx.quit();
            }
            Ok(None) => {}
            Err(error) => warn!(%error, "failed to show the dock menu"),
        }
    }

    fn on_item_click(
        &mut self,
        index: usize,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
}

impl Render for DockView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let offset = self.geometry.bounds.size.height * (1.0 - self.progress);

        div().size_full().child(
            div()
                .id("dock-bar")
                .relative()
                .top(offset)
                .opacity(self.progress)
                .size_full()
                .rounded(self.theme.radius)
                .bg(self.theme.background)
                .border_1()
                .border_color(rgba(0xFFFFFF33))
                .flex()
                .items_center()
                .justify_between()
                .p(self.theme.padding)
                .text_color(self.theme.text)
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _event, _window, cx| this.on_context_menu(cx)),
                )
                .child(div().flex().items_center().gap_1().children(
                    self.items.iter().enumerate().map(|(index, item)| {
                        let on_click = cx.listener(move |this, event: &ClickEvent, window, cx| {
                            this.on_item_click(index, event, window, cx);
                        });
                        render_app_item(item, &self.theme, self.icon_size, on_click)
                    }),
                ))
                .child(self.clock.clone()),
        )
    }
}

fn auto_hide_settings(config: &Config) -> AutoHideSettings {
    AutoHideSettings {
        show_delay: Duration::from_millis(config.behavior.show_delay_ms),
        hide_delay: Duration::from_millis(config.behavior.hide_delay_ms),
        animation: Duration::from_millis(config.behavior.animation_ms),
    }
}
