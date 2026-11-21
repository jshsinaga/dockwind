use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    Bounds, ClickEvent, Context, Entity, MouseButton, MouseDownEvent, Pixels, Point, Render,
    RenderImage, Rgba, ScrollHandle, Size, Timer, Window, div, prelude::*, px, size,
};
use tracing::{debug, error, info, warn};
use windows::Win32::Foundation::RECT;

use super::app_item::render_app_item;
use super::clock::ClockView;
use super::theme::Theme;
use crate::app::AppController;
use crate::config::{AlignmentAnchor, Appearance, BlurMode, Clock, Config, DockEdge, WidthMode};
use crate::dock::{AutoHide, AutoHideInput, AutoHideSettings, DockItem};
use crate::win;
use crate::win::appbar::{AppBarEdge, AppBarHandle};
use crate::win::menu::{MenuAction, TrayAction};
use crate::win::monitors::MonitorInfo;

#[derive(Debug, Clone, Copy)]
pub struct DockGeometry {
    pub bounds: Bounds<Pixels>,
    pub trigger: Bounds<Pixels>,
    pub scale_factor: f32,
    pub hwnd: isize,
    pub edge: DockEdge,
    pub monitor: MonitorInfo,
}

#[derive(Default)]
struct CachedIcon {
    image: Option<Arc<RenderImage>>,
    attempted_windows: HashSet<Option<isize>>,
}

impl CachedIcon {
    fn needs_lookup(&self, window: Option<isize>) -> bool {
        self.image.is_none() && !self.attempted_windows.contains(&window)
    }

    fn record_result(&mut self, window: Option<isize>, image: Option<Arc<RenderImage>>) {
        if let Some(image) = image {
            self.image = Some(image);
            self.attempted_windows.clear();
        } else {
            self.attempted_windows.insert(window);
        }
    }
}

const APP_HOVER_DURATION: Duration = Duration::from_millis(120);
const TRAY_BUTTON_WIDTH: f32 = 44.0;

struct AppHover {
    from: f32,
    target: f32,
    started: Instant,
}

impl AppHover {
    fn progress(&self, now: Instant, animated: bool) -> f32 {
        if !animated {
            return self.target;
        }
        let fraction = (now.saturating_duration_since(self.started).as_secs_f32()
            / APP_HOVER_DURATION.as_secs_f32())
        .min(1.0);
        let eased = 1.0 - (1.0 - fraction).powi(3);
        self.from + (self.target - self.from) * eased
    }
}

pub struct DockView {
    controller: Entity<AppController>,
    applied_config: Config,
    theme: Theme,
    clock: Entity<ClockView>,
    items: Vec<Arc<DockItem>>,
    icons: HashMap<PathBuf, CachedIcon>,
    loading_icons: bool,
    app_scroll: ScrollHandle,
    icon_size: Pixels,
    geometry: DockGeometry,
    auto_hide: AutoHide,
    auto_hide_settings: AutoHideSettings,
    animations_enabled: bool,
    app_hover: HashMap<usize, AppHover>,
    appbar: Option<AppBarHandle>,
    native_taskbar_hidden: bool,
    menu_open: bool,
    progress: f32,
    window_region: Option<[i32; 4]>,
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
        let (config, items, native_taskbar_hidden) = {
            let controller = controller.read(cx);
            let items = controller
                .items()
                .iter()
                .cloned()
                .map(Arc::new)
                .collect::<Vec<_>>();
            (
                controller.config().clone(),
                items,
                controller.native_taskbar_hidden(),
            )
        };
        let clock = cx.new(|cx| ClockView::new(cx, &config.clock));
        let auto_hide = AutoHide::new(config.behavior.auto_hide);
        let progress = auto_hide.progress();
        let start_time = Instant::now();
        let animations_enabled = win::motion::animations_enabled().unwrap_or_else(|error| {
            warn!(%error, "using configured dock animation");
            true
        });
        let mut this = Self {
            controller,
            applied_config: config.clone(),
            theme,
            clock,
            items,
            icons: HashMap::new(),
            loading_icons: false,
            app_scroll: ScrollHandle::default(),
            icon_size: px(config.appearance.icon_size as f32),
            geometry,
            auto_hide,
            auto_hide_settings: auto_hide_settings(&config, animations_enabled),
            animations_enabled,
            app_hover: HashMap::new(),
            appbar: None,
            native_taskbar_hidden,
            menu_open: false,
            progress,
            window_region: None,
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
        this.load_missing_icons(cx);

        let controller = this.controller.clone();
        cx.observe(&controller, |this, controller, cx| {
            this.sync_controller(&controller, cx);
        })
        .detach();

        cx.spawn(async move |this, cx| {
            loop {
                let delay = this
                    .read_with(cx, |this, _| this.poll_interval())
                    .unwrap_or(Duration::from_millis(100));
                Timer::after(delay).await;
                let now = start_time.elapsed();
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
        let controller = controller.read(cx);
        let config = controller.config();
        let native_taskbar_hidden = controller.native_taskbar_hidden();
        let native_taskbar_changed = self.native_taskbar_hidden != native_taskbar_hidden;
        self.native_taskbar_hidden = native_taskbar_hidden;
        let count_changed = self.items.len() != controller.items().len();
        let items_changed = self
            .items
            .iter()
            .map(Arc::as_ref)
            .ne(controller.items().iter());
        if items_changed {
            if self
                .items
                .iter()
                .map(|item| (&item.key, &item.args))
                .ne(controller
                    .items()
                    .iter()
                    .map(|item| (&item.key, &item.args)))
            {
                self.app_hover.clear();
            }
            self.items = controller.items().iter().cloned().map(Arc::new).collect();
        }
        let config = (config != &self.applied_config).then(|| config.clone());

        if items_changed {
            self.load_missing_icons(cx);
        }

        if let Some(config) = config {
            if !items_changed
                && !native_taskbar_changed
                && config.appearance == self.applied_config.appearance
                && config.behavior == self.applied_config.behavior
                && config.clock == self.applied_config.clock
                && config.pinned == self.applied_config.pinned
            {
                // A saved-layout-only edit changes the menu, which reads this on demand.
                self.applied_config = config;
            } else {
                self.apply_config(config, count_changed, native_taskbar_changed, cx);
            }
        } else if items_changed || native_taskbar_changed {
            if native_taskbar_changed {
                let config = self.applied_config.clone();
                self.update_geometry_and_appbar(&config);
            } else if count_changed && self.applied_config.appearance.width_mode == WidthMode::Auto
            {
                let config = self.applied_config.clone();
                self.apply_geometry(&config);
            }
            cx.notify();
        }
    }

    fn set_app_hover(&mut self, index: usize, hovered: bool, cx: &mut Context<Self>) {
        let now = Instant::now();
        let target = if hovered { 1.0 } else { 0.0 };
        let from = self
            .app_hover
            .get(&index)
            .map_or(0.0, |hover| hover.progress(now, self.animations_enabled));
        self.app_hover.insert(
            index,
            AppHover {
                from,
                target,
                started: now,
            },
        );
        cx.notify();
    }

    fn load_missing_icons(&mut self, cx: &mut Context<Self>) {
        if self.loading_icons {
            return;
        }
        let mut requests = HashMap::new();
        for item in &self.items {
            let hwnd = item.target_window();
            if self
                .icons
                .get(&item.key)
                .is_none_or(|icon| icon.needs_lookup(hwnd))
            {
                requests.insert(item.key.clone(), (item.executable_path().is_some(), hwnd));
            }
        }
        if requests.is_empty() {
            return;
        }

        self.loading_icons = true;
        let load = cx.background_executor().spawn(async move {
            requests
                .into_iter()
                .map(|(path, (has_path, hwnd))| {
                    let icon = win::icons::load_icon(has_path.then_some(path.as_path()), hwnd);
                    (path, hwnd, icon)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let loaded = load.await;
            let _ = this.update(cx, |this, cx| {
                for (path, hwnd, icon) in loaded {
                    this.icons
                        .entry(path)
                        .or_default()
                        .record_result(hwnd, icon);
                }
                this.loading_icons = false;
                this.load_missing_icons(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_config(
        &mut self,
        config: Config,
        count_changed: bool,
        native_taskbar_changed: bool,
        cx: &mut Context<Self>,
    ) {
        let theme = match Theme::from_config(&config.appearance) {
            Ok(theme) => theme,
            Err(error) => {
                error!(%error, "config appearance update rejected");
                return;
            }
        };

        let blur_changed = self.applied_config.appearance.blur != config.appearance.blur;
        let geometry_changed = [
            native_taskbar_changed,
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
            self.applied_config.clock.date_font_size != config.clock.date_font_size,
        ]
        .into_iter()
        .any(|changed| changed);

        self.theme = theme;
        self.icon_size = px(config.appearance.icon_size as f32);
        self.clock
            .update(cx, |clock, cx| clock.set_config(&config.clock, cx));
        self.auto_hide_settings = auto_hide_settings(&config, self.animations_enabled);
        if !config.behavior.auto_hide && self.applied_config.behavior.auto_hide {
            self.auto_hide = AutoHide::new(false);
        }
        if geometry_changed || (count_changed && config.appearance.width_mode == WidthMode::Auto) {
            self.update_geometry_and_appbar(&config);
        }
        if config.pinned != self.applied_config.pinned {
            self.app_scroll.set_offset(Point::default());
        }
        self.applied_config = config;

        if blur_changed && self.progress > 0.001 {
            self.apply_backdrop();
        }

        info!(
            corner_radius = self.applied_config.appearance.corner_radius,
            background = %self.applied_config.appearance.background,
            "dock appearance updated"
        );
        cx.notify();
    }

    fn update_geometry_and_appbar(&mut self, config: &Config) {
        if self.reserves_space(config) {
            self.apply_geometry(config);
            self.sync_appbar(config);
        } else {
            // Drop the reservation before reading the work area for placement.
            self.sync_appbar(config);
            self.apply_geometry(config);
        }
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
        let content_size = dock_size(config, self.items.len(), monitor.work_area.size.width);
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

        self.geometry.bounds = bounds;
        self.geometry.trigger = trigger;
        self.geometry.scale_factor = monitor.scale_factor;
        self.geometry.edge = config.appearance.orientation;
        self.geometry.monitor = *monitor;
        self.window_region = None;
        win::dwm::force_render(self.geometry.hwnd);
        true
    }

    fn reserves_space(&self, config: &Config) -> bool {
        config.behavior.reserve_space
            && (!config.behavior.hide_native_taskbar || self.native_taskbar_hidden)
    }

    fn sync_appbar(&mut self, config: &Config) {
        let reserve = self.reserves_space(config);
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
            match win::motion::animations_enabled() {
                Ok(enabled) if enabled != self.animations_enabled => {
                    self.animations_enabled = enabled;
                    self.auto_hide_settings = auto_hide_settings(&self.applied_config, enabled);
                    cx.notify();
                }
                Ok(_) => {}
                Err(error) => debug!(%error, "could not refresh Windows animation preference"),
            }
            match MonitorInfo::primary() {
                Ok(monitor)
                    if monitor.bounds != self.geometry.monitor.bounds
                        || monitor.scale_factor != self.geometry.monitor.scale_factor
                        || (!self.reserves_space(&self.applied_config)
                            && monitor.work_area != self.geometry.monitor.work_area) =>
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

        if !self.applied_config.behavior.auto_hide {
            return;
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
            enabled: true,
            force_hidden,
            pinned_open: self.menu_open,
        };
        if self.auto_hide.update_target(input, self.auto_hide_settings) {
            cx.notify();
        }
    }

    fn poll_interval(&self) -> Duration {
        let milliseconds = if !self.applied_config.behavior.auto_hide {
            100
        } else if self.progress == 0.0 && !self.auto_hide.is_animating() {
            32
        } else {
            16
        };
        Duration::from_millis(milliseconds)
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
        let layouts = self
            .applied_config
            .pinned_layouts
            .iter()
            .map(|(name, apps)| (name.clone(), *apps == self.applied_config.pinned))
            .collect::<Vec<_>>();
        let next_layout = self.applied_config.next_pinned_layout().map(str::to_owned);
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

            let result = win::menu::show(hwnd, &layouts, next_layout.as_deref());
            let _ = this.update(cx, |this, cx| {
                this.menu_open = false;
                this.handle_menu_result(result, cx);
            });
        })
        .detach();
    }

    fn on_tray_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu_open {
            return;
        }
        self.menu_open = true;
        cx.notify();
        let hwnd = self.geometry.hwnd;
        let hidden = self
            .applied_config
            .behavior
            .hide_native_taskbar
            .then_some(self.native_taskbar_hidden);
        cx.spawn(async move |this, cx| {
            Timer::after(Duration::ZERO).await;
            let result = win::menu::show_tray(hwnd, hidden);
            let _ = this.update(cx, |this, cx| {
                this.menu_open = false;
                match result {
                    Ok(Some(TrayAction::ToggleNativeTaskbar)) => {
                        this.controller
                            .update(cx, |controller, cx| controller.toggle_taskbar_for_tray(cx));
                    }
                    Ok(Some(TrayAction::OpenSettings(uri))) => {
                        if let Err(error) = win::launch::launch(uri, &[]) {
                            warn!(%error, uri, "failed to open Windows settings from the dock");
                        }
                    }
                    Ok(None) => {}
                    Err(error) => warn!(%error, "failed to show the dock tray menu"),
                }
                cx.notify();
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
            MenuAction::UsePinnedLayout(name) => {
                self.controller
                    .update(cx, |controller, cx| controller.use_pinned_layout(&name, cx));
            }
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
}

impl DockView {
    fn render_scroll_control(
        &self,
        forward: bool,
        width: Pixels,
        overflow: Pixels,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let text = self.theme.text;
        let offset = self.app_scroll.offset().x;
        let enabled = if forward {
            offset > -overflow + px(0.5)
        } else {
            offset < px(-0.5)
        };
        div()
            .id(if forward {
                "dock-scroll-right"
            } else {
                "dock-scroll-left"
            })
            .w(width)
            .h(px(32.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .text_color(Rgba {
                a: if enabled { 0.75 } else { 0.25 },
                ..text
            })
            .when(enabled, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(Rgba { a: 0.13, ..text }).text_color(text))
                    .active(move |style| style.bg(Rgba { a: 0.20, ..text }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let handle = &this.app_scroll;
                        let mut offset = handle.offset();
                        let width = handle.bounds().size.width;
                        let page = (width - this.icon_size - px(8.0)).max(width * 0.5);
                        let delta = if forward { -page } else { page };
                        let next = scroll_offset(offset.x, delta, handle.max_offset().width);
                        if next != offset.x {
                            offset.x = next;
                            handle.set_offset(offset);
                            cx.notify();
                        }
                    }))
            })
            .child(if forward { "›" } else { "‹" })
    }
}

fn scroll_offset(offset: Pixels, delta: Pixels, max_offset: Pixels) -> Pixels {
    (offset + delta).clamp(-max_offset, px(0.0))
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

        let scale = self.geometry.scale_factor;
        let region = if is_hidden {
            [0; 4]
        } else {
            [
                (f32::from(offset) * scale).round() as i32,
                (f32::from(self.geometry.bounds.size.width) * scale).round() as i32,
                (f32::from(self.geometry.bounds.size.height) * scale).round() as i32,
                (f32::from(self.theme.radius) * scale).round() as i32,
            ]
        };

        let first_frame = self.window_region.is_none();
        if self.window_region != Some(region) {
            let [y, width, height, radius] = region;
            match win::dwm::set_window_shape(self.geometry.hwnd, 0, y, width, height, radius) {
                Ok(()) => self.window_region = Some(region),
                Err(error) => warn!(%error, "failed to update the dock window region"),
            }
        }
        if is_hidden && (first_frame || !was_hidden) {
            if let Err(error) = win::dwm::apply_backdrop_to_hwnd(self.geometry.hwnd, BlurMode::None)
            {
                warn!(%error, "failed to clear the hidden dock backdrop");
            }
        } else if !is_hidden && was_hidden {
            self.apply_backdrop();
        }

        let hover_now = Instant::now();
        let hover_effects = self.theme.hover_zoom || self.theme.hover_background;
        if is_hidden || !hover_effects {
            self.app_hover.clear();
        }
        self.app_hover.retain(|_, hover| {
            hover.target > 0.0 || hover.progress(hover_now, self.animations_enabled) > 0.0
        });
        if self.animations_enabled
            && self.app_hover.values().any(|hover| {
                hover.from != hover.target
                    && hover_now.saturating_duration_since(hover.started) < APP_HOVER_DURATION
            })
        {
            window.request_animation_frame();
        }

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
            .opacity(visual_progress.clamp(0.0, 1.0))
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

        let strip_width = app_strip_width(&self.applied_config.appearance, self.items.len());
        let inner_width =
            (self.geometry.bounds.size.width - pl - pr - self.theme.border_width * 2.0)
                .max(px(0.0));
        let tray_width = px(TRAY_BUTTON_WIDTH).min(inner_width * 0.3);
        let content_width = inner_width - tray_width;
        let text = self.theme.text;
        let tray = div()
            .id("dock-tray")
            .w(tray_width)
            .h(px(28.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .text_xs()
            .cursor_pointer()
            .hover(move |style| style.bg(Rgba { a: 0.13, ..text }))
            .active(move |style| style.bg(Rgba { a: 0.20, ..text }))
            .on_click(cx.listener(|this, _, _, cx| this.on_tray_menu(cx)))
            .child("Tray");
        let clock_gap = px(12.0);
        let clock_width = preferred_clock_width(&self.applied_config.clock)
            .min((content_width * 0.45 - clock_gap).max(px(0.0)));

        let mut apps_div = div()
            .flex()
            .w(strip_width)
            .min_w_full()
            .flex_shrink_0()
            .gap(self.theme.item_spacing);

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

        let items_elements = self.items.iter().enumerate().map(|(index, item)| {
            let icon = self
                .icons
                .get(&item.key)
                .and_then(|icon| icon.image.clone());
            let clicked = Arc::clone(item);
            let middle_clicked = Arc::clone(item);
            let on_click = cx.listener(move |this, event: &ClickEvent, _, cx| {
                this.controller.update(cx, |controller, _| {
                    if clicked.is_pinned && event.modifiers().shift {
                        controller.launch_second_instance(&clicked);
                    } else {
                        controller.activate_or_launch(&clicked);
                    }
                });
            });
            let on_middle = cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                this.controller.update(cx, |controller, _| {
                    controller.launch_second_instance(&middle_clicked);
                });
            });
            let hover = self.app_hover.get(&index).map_or(0.0, |hover| {
                hover.progress(hover_now, self.animations_enabled)
            });
            render_app_item(index, item, icon, &self.theme, self.icon_size, hover)
                .on_click(on_click)
                .on_mouse_down(MouseButton::Middle, on_middle)
                .when(hover_effects, |tile| {
                    tile.on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        this.set_app_hover(index, *hovered, cx);
                    }))
                })
        });

        let clock_space = if self.applied_config.clock.show {
            clock_width + clock_gap
        } else {
            px(0.0)
        };
        let trailing_space = clock_space + tray_width;
        let balance_dock = self.theme.alignment_anchor == AlignmentAnchor::Dock
            && self.theme.item_alignment == crate::config::ItemAlignment::Center
            && strip_width + trailing_space * 2.0 <= inner_width;
        let app_width = inner_width - trailing_space;
        let control_width = px(24.0);
        let show_controls = strip_width > app_width && app_width >= control_width * 3.0;
        let overflow = strip_width - (app_width - control_width * 2.0);
        let viewport = div()
            .id("dock-apps")
            .flex()
            .flex_1()
            .min_w(px(0.0))
            .track_scroll(&self.app_scroll)
            .overflow_x_scroll()
            .child(apps_div.children(items_elements));

        let apps = div()
            .flex()
            .flex_1()
            .min_w(px(0.0))
            .items_center()
            .when(show_controls, |row| {
                row.child(self.render_scroll_control(false, control_width, overflow, cx))
            })
            .child(viewport)
            .when(show_controls, |row| {
                row.child(self.render_scroll_control(true, control_width, overflow, cx))
            });

        let mut dock_bar = dock_bar
            .when(balance_dock, |bar| {
                bar.child(div().w(trailing_space).flex_shrink_0())
            })
            .child(apps);
        if self.applied_config.clock.show {
            let mut clock = div()
                .w(clock_width)
                .h_full()
                .ml(clock_gap)
                .flex_shrink_0()
                .overflow_hidden()
                .flex()
                .flex_col();
            clock = match self.applied_config.clock.vertical_align {
                crate::config::VerticalAlign::Center => clock.justify_center(),
                crate::config::VerticalAlign::Top => clock.justify_start(),
                crate::config::VerticalAlign::Bottom => clock.justify_end(),
            };
            dock_bar = dock_bar.child(clock.child(self.clock.clone()));
        }

        div().size_full().child(dock_bar.child(tray))
    }
}

pub(super) fn dock_size(
    config: &Config,
    item_count: usize,
    work_area_width: Pixels,
) -> Size<Pixels> {
    let appearance = &config.appearance;
    let width = match appearance.width_mode {
        WidthMode::Manual => px(appearance.width as f32),
        WidthMode::Auto => {
            let (_, _, left, right) = appearance.effective_padding();
            let padding = px((left + right + appearance.border_width * 2) as f32);
            let apps = app_strip_width(appearance, item_count.max(1));
            (padding + apps + preferred_clock_width(&config.clock) + px(24.0 + TRAY_BUTTON_WIDTH))
                .max(px(120.0))
        }
    };
    size(width.min(work_area_width), px(appearance.height as f32))
}

fn app_strip_width(appearance: &Appearance, item_count: usize) -> Pixels {
    let tiles = (appearance.icon_size + 8) as f32 * item_count as f32;
    let gaps = appearance.item_spacing as f32 * item_count.saturating_sub(1) as f32;
    px(tiles + gaps)
}

fn preferred_clock_width(clock: &Clock) -> Pixels {
    if !clock.show {
        return px(0.0);
    }
    px((clock.time_font_size * 7)
        .max(clock.date_font_size * 11)
        .max(130) as f32)
}

fn contains(bounds: Bounds<Pixels>, point: Point<Pixels>) -> bool {
    point.x >= bounds.origin.x
        && point.x <= bounds.origin.x + bounds.size.width
        && point.y >= bounds.origin.y
        && point.y <= bounds.origin.y + bounds.size.height
}

fn auto_hide_settings(config: &Config, animations_enabled: bool) -> AutoHideSettings {
    AutoHideSettings {
        show_delay: Duration::from_millis(config.behavior.show_delay_ms),
        hide_delay: Duration::from_millis(config.behavior.hide_delay_ms),
        animation: if animations_enabled {
            Duration::from_millis(config.behavior.animation_ms)
        } else {
            Duration::ZERO
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_hover_interpolates_and_respects_reduced_motion() {
        let started = Instant::now();
        let entering = AppHover {
            from: 0.0,
            target: 1.0,
            started,
        };
        let mid = entering.progress(started + Duration::from_millis(60), true);
        assert!(mid > 0.0 && mid < 1.0);
        assert_eq!(entering.progress(started, false), 1.0);
        let leaving = AppHover {
            from: mid,
            target: 0.0,
            started,
        };
        assert_eq!(leaving.progress(started, true), mid);
        assert_eq!(leaving.progress(started + APP_HOVER_DURATION, true), 0.0);
    }

    #[test]
    fn reduced_motion_keeps_delays_and_disables_the_slide() {
        let config = Config::default();
        let standard = auto_hide_settings(&config, true);
        let reduced = auto_hide_settings(&config, false);
        assert!(standard.animation > Duration::ZERO);
        assert_eq!(reduced.animation, Duration::ZERO);
        assert_eq!(reduced.show_delay, standard.show_delay);
        assert_eq!(reduced.hide_delay, standard.hide_delay);
    }

    #[test]
    fn a_failed_icon_lookup_can_retry_for_a_new_window() {
        let mut icon = CachedIcon::default();
        icon.record_result(None, None);
        assert!(!icon.needs_lookup(None));
        assert!(icon.needs_lookup(Some(1)));

        icon.record_result(Some(1), None);
        icon.record_result(Some(2), None);
        assert!(!icon.needs_lookup(Some(1)));
        assert!(icon.needs_lookup(Some(3)));
    }

    #[test]
    fn scroll_buttons_stop_at_both_ends_of_the_strip() {
        assert_eq!(scroll_offset(px(-10.0), px(50.0), px(120.0)), px(0.0));
        assert_eq!(scroll_offset(px(-10.0), px(-50.0), px(120.0)), px(-60.0));
        assert_eq!(scroll_offset(px(-95.0), px(-50.0), px(120.0)), px(-120.0));
    }

    #[test]
    fn width_is_capped_to_the_work_area_in_both_modes() {
        let mut config = Config::default();
        assert_eq!(dock_size(&config, 2, px(320.0)).width, px(320.0));

        config.appearance.width_mode = WidthMode::Auto;
        assert_eq!(dock_size(&config, 100, px(800.0)).width, px(800.0));
        assert!(dock_size(&config, 2, px(800.0)).width < px(800.0));
    }
}
