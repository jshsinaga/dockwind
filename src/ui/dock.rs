use std::time::{Duration, Instant};

use gpui::{
    Bounds, ClickEvent, Context, Entity, MouseButton, MouseDownEvent, Pixels, Point, Render, Timer,
    Window, div, prelude::*, px,
};
use tracing::{debug, error, info, warn};
use windows::Win32::Foundation::RECT;

use super::app_item::render_app_item;
use super::clock::ClockView;
use super::theme::Theme;
use crate::app::AppController;
use crate::config::{BlurMode, Config, DockEdge};
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
        let count_changed = self.items.len() != items.len();
        self.items = items;
        if self.applied_config.appearance.width_mode == crate::config::WidthMode::Auto
            && count_changed
        {
            let config = self.applied_config.clone();
            self.apply_geometry(&config);
        }

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
            self.applied_config.appearance.width_mode != config.appearance.width_mode,
            self.applied_config.appearance.icon_size != config.appearance.icon_size,
            self.applied_config.appearance.item_spacing != config.appearance.item_spacing,
            self.applied_config.appearance.padding != config.appearance.padding,
            self.applied_config.appearance.padding_top != config.appearance.padding_top,
            self.applied_config.appearance.padding_bottom != config.appearance.padding_bottom,
            self.applied_config.appearance.padding_left != config.appearance.padding_left,
            self.applied_config.appearance.padding_right != config.appearance.padding_right,
            self.applied_config.appearance.corner_radius != config.appearance.corner_radius,
            self.applied_config.appearance.border_width != config.appearance.border_width,
            self.applied_config.behavior.reserve_space != config.behavior.reserve_space,
            self.applied_config.behavior.reserve_offset != config.behavior.reserve_offset,
            self.applied_config.behavior.trigger_size != config.behavior.trigger_size,
            self.applied_config.behavior.hide_native_taskbar != config.behavior.hide_native_taskbar,
            self.applied_config.clock.show != config.clock.show,
            self.applied_config.clock.time_font_size != config.clock.time_font_size,
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
        let width = if config.appearance.width_mode == crate::config::WidthMode::Auto {
            let (_, _, left, right) = config.appearance.effective_padding();
            let app_count = self.items.len().max(1) as u32;
            let icon_total = app_count * (config.appearance.icon_size + 8);
            let spacing_total = app_count.saturating_sub(1) * config.appearance.item_spacing;
            let clock_width = if config.clock.show {
                (config.clock.time_font_size * 7).max(130)
            } else {
                0u32
            };
            (left + right + icon_total + spacing_total + clock_width + 24).max(120)
        } else {
            config.appearance.width
        };
        let content_size = gpui::size(px(width as f32), px(config.appearance.height as f32));
        let bounds = win::monitors::dock_rect(
            monitor,
            content_size,
            config.appearance.offset as f32,
            config.appearance.orientation,
        );
        let trigger = win::monitors::trigger_rect(
            monitor,
            config.appearance.orientation,
            config.behavior.trigger_size as f32,
        );

        if let Err(error) = win::dwm::set_bounds(self.geometry.hwnd, bounds, monitor.scale_factor) {
            warn!(%error, "failed to apply the dock geometry");
            return false;
        }

        let p_width = (f32::from(bounds.size.width) * monitor.scale_factor).round() as i32;
        let p_height = (f32::from(bounds.size.height) * monitor.scale_factor).round() as i32;
        let p_radius = (f32::from(self.theme.radius) * monitor.scale_factor).round() as i32;
        let _ = win::dwm::set_window_shape(self.geometry.hwnd, 0, 0, p_width, p_height, p_radius);

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
        let extra_offset = config.behavior.reserve_offset as f32;
        match config.appearance.orientation {
            DockEdge::Bottom => {
                let reserved_top = (f32::from(bounds.origin.y) - extra_offset) * scale;
                RECT {
                    left,
                    top: reserved_top.round() as i32,
                    right,
                    bottom,
                }
            }
            DockEdge::Top => {
                let reserved_bottom =
                    (f32::from(bounds.origin.y + bounds.size.height) + extra_offset) * scale;
                RECT {
                    left,
                    top,
                    right,
                    bottom: reserved_bottom.round() as i32,
                }
            }
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
        let was_hidden = self.progress <= 0.001;
        let is_hidden = progress <= 0.001;
        if was_hidden != is_hidden {
            self.set_click_through(is_hidden);
        }
        self.progress = progress;

        if self.auto_hide.is_animating() {
            window.request_animation_frame();
        }

        let direction = match self.geometry.edge {
            DockEdge::Bottom => 1.0,
            DockEdge::Top => -1.0,
        };
        let visual_progress = self
            .applied_config
            .behavior
            .animation_curve
            .apply(self.progress);
        let offset = self.geometry.bounds.size.height * (1.0 - visual_progress) * direction;

        if is_hidden {
            let _ = win::dwm::set_window_shape(self.geometry.hwnd, 0, 0, 0, 0, 0);
            let _ = win::dwm::apply_backdrop_to_hwnd(self.geometry.hwnd, BlurMode::None);
        } else {
            let y_offset = (f32::from(offset) * self.geometry.scale_factor).round() as i32;
            let p_width = (f32::from(self.geometry.bounds.size.width) * self.geometry.scale_factor)
                .round() as i32;
            let p_height = (f32::from(self.geometry.bounds.size.height)
                * self.geometry.scale_factor)
                .round() as i32;
            let p_radius =
                (f32::from(self.theme.radius) * self.geometry.scale_factor).round() as i32;
            let _ = win::dwm::set_window_shape(
                self.geometry.hwnd,
                0,
                y_offset,
                p_width,
                p_height,
                p_radius,
            );
            if was_hidden {
                let _ = win::dwm::apply_backdrop_to_hwnd(
                    self.geometry.hwnd,
                    self.applied_config.appearance.blur,
                );
            }
        }

        let icons = {
            let cache = &mut self.icons;
            self.items
                .iter()
                .map(|item| cache.get(&item.key, item.target_window()))
                .collect::<Vec<_>>()
        };

        let (pt, pb, pl, pr) = (
            self.theme.padding_top,
            self.theme.padding_bottom,
            self.theme.padding_left,
            self.theme.padding_right,
        );

        let mut dock_bar = div()
            .id("dock-bar")
            .relative()
            .top(offset)
            .size_full()
            .rounded(self.theme.radius)
            .bg(self.theme.background)
            .border(self.theme.border_width)
            .border_color(self.theme.border_color)
            .flex()
            .justify_between()
            .pt(pt)
            .pb(pb)
            .pl(pl)
            .pr(pr)
            .text_color(self.theme.text)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _event, _window, cx| this.on_context_menu(cx)),
            );

        dock_bar = match self.theme.vertical_align {
            crate::config::VerticalAlign::Center => dock_bar.items_center(),
            crate::config::VerticalAlign::Top => dock_bar.items_start(),
            crate::config::VerticalAlign::Bottom => dock_bar.items_end(),
        };

        if self.theme.shadow {
            dock_bar = dock_bar.shadow_lg();
        }

        let mut apps_div = div()
            .id("dock-apps")
            .flex()
            .flex_1()
            .min_w(px(0.0))
            .gap(self.theme.item_spacing)
            .overflow_x_scroll();

        apps_div = match self.theme.item_alignment {
            crate::config::ItemAlignment::Center => apps_div.justify_center(),
            crate::config::ItemAlignment::Left => apps_div.justify_start(),
            crate::config::ItemAlignment::Right => apps_div.justify_end(),
        };

        apps_div = match self.theme.vertical_align {
            crate::config::VerticalAlign::Center => apps_div.items_center(),
            crate::config::VerticalAlign::Top => apps_div.items_start(),
            crate::config::VerticalAlign::Bottom => apps_div.items_end(),
        };

        let show_clock = self.applied_config.clock.show;

        let mut clock_container = div().flex().flex_col();
        clock_container = match self.applied_config.clock.vertical_align {
            crate::config::VerticalAlign::Center => clock_container.justify_center(),
            crate::config::VerticalAlign::Top => clock_container.justify_start(),
            crate::config::VerticalAlign::Bottom => clock_container.justify_end(),
        };

        let items_elements =
            self.items
                .iter()
                .zip(icons)
                .enumerate()
                .map(|(index, (item, icon))| {
                    let on_click = cx.listener(move |this, event: &ClickEvent, window, cx| {
                        this.on_item_click(index, event, window, cx);
                    });
                    let on_middle =
                        cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                            this.controller.update(cx, |controller, _| {
                                controller.launch_second_instance(index);
                            });
                        });
                    render_app_item(item, icon, &self.theme, self.icon_size, on_click, on_middle)
                });

        let apps_children = apps_div.children(items_elements);

        let dock_bar = if show_clock {
            match self.theme.alignment_anchor {
                crate::config::AlignmentAnchor::Dock => {
                    let absolute_clock = clock_container
                        .absolute()
                        .right(pr)
                        .top_0()
                        .bottom_0()
                        .pt(pt)
                        .pb(pb)
                        .child(self.clock.clone());
                    dock_bar.child(apps_children).child(absolute_clock)
                }
                crate::config::AlignmentAnchor::Clock => {
                    let relative_clock = clock_container.h_full().child(self.clock.clone());
                    dock_bar.child(apps_children).child(relative_clock)
                }
            }
        } else {
            dock_bar.child(apps_children)
        };

        div().size_full().child(dock_bar)
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
