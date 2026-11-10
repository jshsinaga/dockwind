use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    AnyWindowHandle, Bounds, ClickEvent, Context, Entity, Pixels, Render, Timer, Window, div,
    prelude::*, px, rgba,
};
use tracing::{debug, error, info, warn};

use super::app_item::render_app_item;
use super::clock::ClockView;
use crate::app_state::{self, DockItem};
use crate::autohide::{AutoHide, AutoHideSettings};
use crate::config::Config;
use crate::theme::Theme;
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
    progress: f32,
}

impl DockView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cx: &mut Context<Self>,
        theme: Theme,
        window_handle: AnyWindowHandle,
        geometry: DockGeometry,
        config: Config,
        mut config_updates: UnboundedReceiver<Config>,
        mut window_events: UnboundedReceiver<()>,
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
            progress,
        };
        this.set_click_through(progress <= 0.001);
        this.refresh();

        cx.spawn(async move |this, cx| {
            while let Some(config) = config_updates.next().await {
                match Theme::from_config(&config.appearance) {
                    Ok(theme) => {
                        let blur_changed = this.update(cx, |this, cx| {
                            let blur_changed = this.blur != config.appearance.blur;
                            this.blur = config.appearance.blur;
                            this.theme = theme;
                            this.icon_size = px(config.appearance.icon_size as f32);
                            this.clock
                                .update(cx, |clock, cx| clock.set_config(&config.clock, cx));

                            if this.pinned != config.pinned {
                                this.pinned = config.pinned.clone();
                                this.refresh();
                            }

                            this.auto_hide_enabled = config.behavior.auto_hide;
                            this.hide_in_fullscreen = config.behavior.hide_in_fullscreen;
                            this.auto_hide_settings = auto_hide_settings(&config);
                            if !config.behavior.auto_hide {
                                this.progress = 1.0;
                                this.set_click_through(false);
                            }

                            cx.notify();
                            blur_changed
                        });

                        let blur_changed = match blur_changed {
                            Ok(changed) => changed,
                            Err(_) => break,
                        };

                        if blur_changed {
                            let blur = config.appearance.blur;
                            if let Err(error) = cx.update_window(window_handle, |_, window, _| {
                                if let Err(error) = win::dwm::apply_backdrop(window, blur) {
                                    warn!(%error, "backdrop update failed; keeping transparent");
                                }
                            }) {
                                warn!(%error, "could not update the backdrop on the dock window");
                            }
                        }

                        info!(
                            corner_radius = config.appearance.corner_radius,
                            background = %config.appearance.background,
                            "dock appearance updated"
                        );
                    }
                    Err(error) => error!(%error, "config appearance update rejected"),
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
            false,
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
