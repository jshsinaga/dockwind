use std::time::{Duration, Instant};

use gpui::{
    Bounds, ClickEvent, Context, Entity, MouseButton, MouseDownEvent, Pixels, Point, Render, Timer,
    Window, div, prelude::*, px, rgba,
};
use tracing::{debug, error, info, warn};
use windows::Win32::Foundation::RECT;

use super::app_item::render_app_item;
use super::clock::ClockView;
use super::theme::Theme;
use crate::app::AppController;
use crate::config::{Config, DockEdge};
use crate::dock::{AutoHide, AutoHideInput, AutoHideSettings, DockItem};
use crate::win::appbar::{AppBarEdge, AppBarHandle};
use crate::win::menu::MenuAction;
use crate::win::monitors::MonitorInfo;
use crate::win::{self, icons::IconCache};

#[derive(Debug, Clone, Copy)]
pub struct DockGeometry {
    pub bounds: Bounds<Pixels>,
    pub trigger: Bounds<Pixels>,
    pub scale_factor: f32,
    pub hwnd: isize,
    pub edge: DockEdge,
    pub monitor: MonitorInfo,
}

pub struct DockView {
    controller: Entity<AppController>,
    applied_config: Config,
    theme: Theme,
    clock: Entity<ClockView>,
    items: Vec<DockItem>,
    icons: IconCache,
    icon_size: Pixels,
    geometry: DockGeometry,
    auto_hide: AutoHide,
    auto_hide_settings: AutoHideSettings,
    appbar: Option<AppBarHandle>,
    menu_open: bool,
    progress: f32,
    last_monitor_check: Instant,
    start_time: Instant,
}

impl DockView {
    pub fn new(
        cx: &mut Context<Self>,
        controller: Entity<AppController>,
        geometry: DockGeometry,
        theme: Theme,
    ) -> Self {
        let (config, items) = {
            let controller = controller.read(cx);
            (controller.config().clone(), controller.items().to_vec())
        };
        let clock = cx.new(|cx| ClockView::new(cx, &config.clock));
        let auto_hide = AutoHide::new(config.behavior.auto_hide);
        let progress = auto_hide.progress();
        let start_time = Instant::now();
        let mut this = Self {
            controller,
            applied_config: config.clone(),
            theme,
            clock,
            items,
            icons: IconCache::default(),
            icon_size: px(config.appearance.icon_size as f32),
            geometry,
            auto_hide,
            auto_hide_settings: auto_hide_settings(&config),
            appbar: None,
            menu_open: false,
            progress,
            last_monitor_check: start_time,
            start_time,
        };
        this.set_click_through(progress <= 0.001);
        this.appbar = match AppBarHandle::spawn() {
            Ok(handle) => Some(handle),
            Err(error) => {
                warn!(%error, "appbar reservation unavailable");
                None
            }
        };
        this.sync_appbar(&config);

        let controller = this.controller.clone();
        cx.observe(&controller, |this, controller, cx| {
            this.sync_controller(&controller, cx);
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

        cx.spawn(async move |this, cx| {
            Timer::after(Duration::from_millis(0)).await;
            let _ = this.update(cx, |_, cx| {
                cx.notify();
            });
        })
        .detach();

        this
    }

    /// Reads the controller state. Config-derived view state is only recomputed when
    /// the active configuration actually changed.
    fn sync_controller(&mut self, controller: &Entity<AppController>, cx: &mut Context<Self>) {
        let (config, items) = {
            let controller = controller.read(cx);
            (controller.config().clone(), controller.items().to_vec())
        };
        self.items = items;

        if config == self.applied_config {
            cx.notify();
            return;
        }

        self.apply_config(config, cx);
    }

    fn apply_config(&mut self, config: Config, cx: &mut Context<Self>) {
        let theme = match Theme::from_config(&config.appearance) {
            Ok(theme) => theme,
            Err(error) => {
                error!(%error, "config appearance update rejected");
                return;
            }
        };

        let blur_changed = self.applied_config.appearance.blur != config.appearance.blur;
        let geometry_changed = [
            self.applied_config.appearance.width != config.appearance.width,
            self.applied_config.appearance.height != config.appearance.height,
            self.applied_config.appearance.offset != config.appearance.offset,
            self.applied_config.appearance.orientation != config.appearance.orientation,
            self.applied_config.behavior.reserve_space != config.behavior.reserve_space,
            self.applied_config.behavior.hide_native_taskbar != config.behavior.hide_native_taskbar,
        ]
        .into_iter()
        .any(|changed| changed);

        self.theme = theme;
        self.icon_size = px(config.appearance.icon_size as f32);
        self.clock
            .update(cx, |clock, cx| clock.set_config(&config.clock, cx));
        self.auto_hide_settings = auto_hide_settings(&config);
        if !config.behavior.auto_hide {
            self.progress = 1.0;
            self.set_click_through(false);
        }
        if geometry_changed {
            self.apply_geometry(&config);
            self.sync_appbar(&config);
        }
        self.applied_config = config;

        if blur_changed {
            self.apply_backdrop();
        }

        info!(
            corner_radius = self.applied_config.appearance.corner_radius,
            background = %self.applied_config.appearance.background,
            "dock appearance updated"
        );
        cx.notify();
    }

    fn apply_geometry(&mut self, config: &Config) {
        let monitor = match MonitorInfo::primary() {
            Ok(monitor) => monitor,
            Err(error) => {
                warn!(%error, "failed to read the primary monitor for a geometry update");
                return;
            }
        };
        self.apply_geometry_with_monitor(config, &monitor);
    }

    fn apply_geometry_with_monitor(&mut self, config: &Config, monitor: &MonitorInfo) -> bool {
        let content_size = gpui::size(
            px(config.appearance.width as f32),
            px(config.appearance.height as f32),
        );
        let bounds = win::monitors::dock_rect(
            monitor,
            content_size,
            config.appearance.offset as f32,
            config.appearance.orientation,
        );
        let trigger = win::monitors::trigger_rect(monitor, config.appearance.orientation);

        if let Err(error) = win::dwm::set_bounds(self.geometry.hwnd, bounds, monitor.scale_factor) {
            warn!(%error, "failed to apply the dock geometry");
            return false;
        }
        win::dwm::force_render(self.geometry.hwnd);

        self.geometry.bounds = bounds;
        self.geometry.trigger = trigger;
        self.geometry.scale_factor = monitor.scale_factor;
        self.geometry.edge = config.appearance.orientation;
        self.geometry.monitor = *monitor;
        true
    }

    fn sync_appbar(&mut self, config: &Config) {
        let reserve = config.behavior.reserve_space;
        let rect = if reserve {
            Some(self.appbar_rect(config))
        } else {
            None
        };
        let edge = match config.appearance.orientation {
            DockEdge::Bottom => AppBarEdge::Bottom,
            DockEdge::Top => AppBarEdge::Top,
        };

        let Some(appbar) = self.appbar.as_mut() else {
            return;
        };
        if !reserve {
            if let Err(error) = appbar.disable() {
                warn!(%error, "failed to release appbar space");
            }
            return;
        }

        if let Some(rect) = rect {
            let result = if appbar.is_active() {
                appbar.update(rect, edge)
            } else {
                appbar.enable(rect, edge)
            };
            if let Err(error) = result {
                warn!(%error, "failed to reserve appbar space");
            }
        }
    }

    fn appbar_rect(&self, config: &Config) -> RECT {
        let scale = self.geometry.scale_factor;
        let monitor = self.geometry.monitor;
        let bounds = self.geometry.bounds;
        let left = (f32::from(monitor.bounds.origin.x) * scale).round() as i32;
        let right =
            (f32::from(monitor.bounds.origin.x + monitor.bounds.size.width) * scale).round() as i32;
        let top = (f32::from(monitor.bounds.origin.y) * scale).round() as i32;
        let bottom = (f32::from(monitor.bounds.origin.y + monitor.bounds.size.height) * scale)
            .round() as i32;
        match config.appearance.orientation {
            DockEdge::Bottom => RECT {
                left,
                top: (f32::from(bounds.origin.y) * scale).round() as i32,
                right,
                bottom,
            },
            DockEdge::Top => RECT {
                left,
                top,
                right,
                bottom: (f32::from(bounds.origin.y + bounds.size.height) * scale).round() as i32,
            },
        }
    }

    fn tick(&mut self, now: Duration, cx: &mut Context<Self>) {
        if self.last_monitor_check.elapsed() >= Duration::from_secs(1) {
            self.last_monitor_check = Instant::now();
            match MonitorInfo::primary() {
                Ok(monitor)
                    if monitor.bounds != self.geometry.monitor.bounds
                        || monitor.scale_factor != self.geometry.monitor.scale_factor =>
                {
                    let config = self.applied_config.clone();
                    if self.apply_geometry_with_monitor(&config, &monitor) {
                        self.sync_appbar(&config);
                        cx.notify();
                    }
                }
                Ok(_) => {}
                Err(error) => debug!(%error, "failed to read the primary monitor"),
            }
        }

        let cursor = match win::cursor::cursor_pos(self.geometry.scale_factor) {
            Ok(cursor) => cursor,
            Err(error) => {
                warn!(%error, "failed to read the cursor position");
                return;
            }
        };

        let dock_interaction_bounds = match self.geometry.edge {
            DockEdge::Bottom => Bounds::new(
                self.geometry.bounds.origin,
                gpui::size(
                    self.geometry.bounds.size.width,
                    self.geometry.monitor.bounds.origin.y
                        + self.geometry.monitor.bounds.size.height
                        - self.geometry.bounds.origin.y,
                ),
            ),
            DockEdge::Top => Bounds::new(
                gpui::point(
                    self.geometry.bounds.origin.x,
                    self.geometry.monitor.bounds.origin.y,
                ),
                gpui::size(
                    self.geometry.bounds.size.width,
                    self.geometry.bounds.origin.y + self.geometry.bounds.size.height
                        - self.geometry.monitor.bounds.origin.y,
                ),
            ),
        };

        let force_hidden =
            self.applied_config.behavior.hide_in_fullscreen && win::cursor::fullscreen_active();
        let input = AutoHideInput {
            now,
            pointer_in_dock: contains(dock_interaction_bounds, cursor),
            pointer_in_trigger: contains(self.geometry.trigger, cursor),
            enabled: self.applied_config.behavior.auto_hide,
            force_hidden,
            pinned_open: self.menu_open,
        };
        let target_changed = self.auto_hide.update_target(input, self.auto_hide_settings);
        let progress = self.auto_hide.advance(now, self.auto_hide_settings);

        if target_changed || (progress - self.progress).abs() > f32::EPSILON {
            self.progress = progress;
            self.set_click_through(progress <= 0.001);
            cx.notify();
        }
    }

    fn poll_interval(&self) -> Duration {
        if self.applied_config.behavior.auto_hide {
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

    fn apply_backdrop(&self) {
        if let Err(error) = win::dwm::apply_backdrop_to_hwnd(
            self.geometry.hwnd,
            self.applied_config.appearance.blur,
        ) {
            warn!(%error, "backdrop update failed; keeping transparent");
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
        let action = match result {
            Ok(Some(action)) => action,
            Ok(None) => return,
            Err(error) => {
                warn!(%error, "failed to show the dock menu");
                return;
            }
        };

        match action {
            MenuAction::OpenSettings => {
                let path = self.controller.read(cx).config_path().to_path_buf();
                if let Err(error) = super::open_settings_window(cx, path) {
                    warn!(%error, "failed to open the settings window");
                }
            }
            MenuAction::ReloadConfig => {
                self.controller
                    .update(cx, |controller, cx| controller.reload_config(cx));
            }
            MenuAction::ToggleAutostart => {
                self.controller
                    .update(cx, |controller, _| controller.toggle_autostart());
            }
            MenuAction::RestoreAndQuit => {
                self.controller
                    .update(cx, |controller, cx| controller.restore_and_quit(cx));
            }
        }
    }

    fn on_item_click(
        &mut self,
        index: usize,
        _event: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.controller.update(cx, |controller, cx| {
            controller.activate_or_launch(index, cx)
        });
    }
}

impl Render for DockView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = self.start_time.elapsed();
        let progress = self.auto_hide.advance(now, self.auto_hide_settings);
        let was_click_through = self.progress <= 0.001;
        let is_click_through = progress <= 0.001;
        if was_click_through != is_click_through {
            self.set_click_through(is_click_through);
        }
        self.progress = progress;

        if self.auto_hide.is_animating() {
            window.request_animation_frame();
        }

        let direction = match self.geometry.edge {
            DockEdge::Bottom => 1.0,
            DockEdge::Top => -1.0,
        };
        let visual_progress = 1.0 - (1.0 - self.progress) * (1.0 - self.progress);
        let offset = self.geometry.bounds.size.height * (1.0 - visual_progress) * direction;
        let icons = {
            let cache = &mut self.icons;
            self.items
                .iter()
                .map(|item| cache.get(&item.key, item.target_window()))
                .collect::<Vec<_>>()
        };

        div().size_full().child(
            div()
                .id("dock-bar")
                .relative()
                .top(offset)
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
                .child(
                    div()
                        .id("dock-apps")
                        .flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .items_center()
                        .gap_1()
                        .overflow_x_scroll()
                        .children(self.items.iter().zip(icons).enumerate().map(
                            |(index, (item, icon))| {
                                let on_click =
                                    cx.listener(move |this, event: &ClickEvent, window, cx| {
                                        this.on_item_click(index, event, window, cx);
                                    });
                                let on_middle = cx.listener(
                                    move |this, _event: &MouseDownEvent, _window, cx| {
                                        this.controller.update(cx, |controller, _| {
                                            controller.launch_second_instance(index);
                                        });
                                    },
                                );
                                render_app_item(
                                    item,
                                    icon,
                                    &self.theme,
                                    self.icon_size,
                                    on_click,
                                    on_middle,
                                )
                            },
                        )),
                )
                .child(self.clock.clone()),
        )
    }
}

fn contains(bounds: Bounds<Pixels>, point: Point<Pixels>) -> bool {
    point.x >= bounds.origin.x
        && point.x <= bounds.origin.x + bounds.size.width
        && point.y >= bounds.origin.y
        && point.y <= bounds.origin.y + bounds.size.height
}

fn auto_hide_settings(config: &Config) -> AutoHideSettings {
    AutoHideSettings {
        show_delay: Duration::from_millis(config.behavior.show_delay_ms),
        hide_delay: Duration::from_millis(config.behavior.hide_delay_ms),
        animation: Duration::from_millis(config.behavior.animation_ms),
    }
}
