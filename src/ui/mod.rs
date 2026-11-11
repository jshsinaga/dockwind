mod app_item;
mod clock;
mod dock;
mod settings;
mod theme;

pub use dock::{DockGeometry, DockView};
pub use settings::{open_settings_standalone, open_settings_window};

use std::cell::Cell;

use anyhow::{Context as _, Result};
use gpui::{
    App, AppContext, Entity, Render, SharedString, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, prelude::*, px, size,
};
use tracing::{info, warn};

use crate::app::AppController;
use crate::config::Config;
use crate::win;

use self::theme::Theme;

/// Window root that owns the dock view once creation has fully succeeded.
struct DockRoot {
    view: Option<Entity<DockView>>,
}

impl Render for DockRoot {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        match &self.view {
            Some(view) => view.clone().into_any_element(),
            None => div().size_full().into_any_element(),
        }
    }
}

pub fn open_dock_window(
    cx: &mut App,
    controller: Entity<AppController>,
    config: &Config,
) -> Result<()> {
    let monitor =
        win::monitors::MonitorInfo::primary().context("failed to read the primary monitor")?;
    let initial_width = if config.appearance.width_mode == crate::config::WidthMode::Auto {
        let (_, _, left, right) = config.appearance.effective_padding();
        let app_count = controller.read(cx).items().len().max(1) as u32;
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
    let content_size = size(
        px(initial_width as f32),
        px(config.appearance.height as f32),
    );
    let bounds = win::monitors::dock_rect(
        &monitor,
        content_size,
        config.appearance.offset as f32,
        config.appearance.orientation,
    );
    let trigger = win::monitors::trigger_rect(
        &monitor,
        config.appearance.orientation,
        config.behavior.trigger_size as f32,
    );
    let scale_factor = monitor.scale_factor;
    let theme = Theme::from_config(&config.appearance).context("failed to build the dock theme")?;
    let blur = config.appearance.blur;

    info!(?monitor, ?bounds, "positioning dock");
    let previous_foreground = win::dwm::foreground_window();
    let mut startup_error = None;
    let hwnd_slot = Cell::new(0isize);

    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Dockwind")),
                    appears_transparent: true,
                    ..Default::default()
                }),
                focus: false,
                show: true,
                kind: WindowKind::PopUp,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |window, cx| {
                let view = (|| -> Result<Entity<DockView>> {
                    win::dwm::apply_dock_styles(window, previous_foreground)?;
                    let hwnd = win::dwm::window_hwnd(window)?;
                    hwnd_slot.set(hwnd);
                    win::dwm::set_bounds(hwnd, bounds, scale_factor)?;
                    let p_width = (f32::from(bounds.size.width) * scale_factor).round() as i32;
                    let p_height = (f32::from(bounds.size.height) * scale_factor).round() as i32;
                    let p_radius = (f32::from(theme.radius) * scale_factor).round() as i32;
                    let _ = win::dwm::set_window_shape(hwnd, 0, 0, p_width, p_height, p_radius);
                    if let Err(error) = win::dwm::apply_backdrop(window, blur) {
                        warn!(%error, "backdrop configuration failed; keeping transparent");
                    }
                    let geometry = DockGeometry {
                        bounds,
                        trigger,
                        scale_factor,
                        hwnd,
                        edge: config.appearance.orientation,
                        monitor,
                    };
                    Ok(cx.new(|cx| DockView::new(cx, controller, geometry, theme)))
                })();

                match view {
                    Ok(view) => cx.new(|_| DockRoot { view: Some(view) }),
                    Err(error) => {
                        startup_error = Some(error);
                        cx.new(|_| DockRoot { view: None })
                    }
                }
            },
        )
        .context("failed to open the dock window")?;

    if let Some(error) = startup_error {
        return Err(error);
    }

    let hwnd = hwnd_slot.get();
    win::dwm::set_bounds(hwnd, bounds, scale_factor).context("failed to set dock bounds")?;
    win::dwm::show_without_activating(hwnd).context("failed to show the dock window")?;
    win::dwm::force_render(hwnd);
    let _ = handle.update(cx, |_, window, _| {
        window.refresh();
    });
    Ok(())
}
