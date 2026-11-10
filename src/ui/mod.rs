mod app_item;
mod clock;
mod dock;
mod theme;

pub use dock::{DockGeometry, DockView};

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
    let content_size = size(
        px(config.appearance.width as f32),
        px(config.appearance.height as f32),
    );
    let reserve_space = config.behavior.reserve_space && config.behavior.hide_native_taskbar;
    let bounds = win::monitors::dock_rect(
        &monitor,
        content_size,
        config.appearance.offset as f32,
        config.appearance.orientation,
        reserve_space,
    );
    let trigger =
        win::monitors::trigger_rect(&monitor, config.appearance.orientation, reserve_space);
    let scale_factor = monitor.scale_factor;
    let theme = Theme::from_config(&config.appearance).context("failed to build the dock theme")?;
    let blur = config.appearance.blur;

    info!(?monitor, ?bounds, "positioning dock");
    let previous_foreground = win::dwm::foreground_window();
    let mut startup_error = None;
    let hwnd_slot = Cell::new(0isize);

    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::from("Dockwind")),
                appears_transparent: true,
                ..Default::default()
            }),
            focus: false,
            // Keep the window hidden until GPUI has drawn the first frame. Showing
            // it earlier exposes the empty native surface as a white rectangle.
            show: false,
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

    win::dwm::show_without_activating(hwnd_slot.get()).context("failed to show the dock window")?;
    Ok(())
}
