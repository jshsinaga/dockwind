mod app_item;
mod clock;
mod dock;

pub use dock::{DockGeometry, DockView};

use anyhow::{Context as _, Result};
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, AppContext, SharedString, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, px, size,
};
use tracing::{info, warn};

use crate::config::Config;
use crate::theme::Theme;
use crate::win;

const DOCK_WIDTH: f32 = 560.0;
const DOCK_HEIGHT: f32 = 92.0;

pub fn open_dock_window(
    cx: &mut App,
    config: Config,
    config_updates: UnboundedReceiver<Config>,
    window_events: UnboundedReceiver<()>,
    hotkey_events: Option<UnboundedReceiver<()>>,
) -> Result<()> {
    let monitor =
        win::monitors::MonitorInfo::primary().context("failed to read the primary monitor")?;
    let content_size = size(px(DOCK_WIDTH), px(DOCK_HEIGHT));
    let bounds = win::monitors::dock_rect(
        &monitor,
        content_size,
        config.appearance.margin_bottom as f32,
    );
    let trigger = win::monitors::trigger_rect(&monitor);
    let scale_factor = monitor.scale_factor;
    let theme = Theme::from_config(&config.appearance).context("failed to build the dock theme")?;
    let blur = config.appearance.blur;

    info!(?monitor, ?bounds, "positioning dock");
    let previous_foreground = win::dwm::foreground_window();

    cx.open_window(
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
            win::dwm::apply_dock_styles(window, previous_foreground)
                .expect("failed to apply dock window styles");
            if let Err(error) = win::dwm::apply_backdrop(window, blur) {
                warn!(%error, "backdrop configuration failed; keeping transparent");
            }
            let window_handle = window.window_handle();
            let hwnd = win::dwm::window_hwnd(window).expect("failed to read the dock HWND");
            let geometry = DockGeometry {
                bounds,
                trigger,
                scale_factor,
                hwnd,
            };
            cx.new(|cx| {
                DockView::new(
                    cx,
                    theme,
                    window_handle,
                    geometry,
                    config,
                    config_updates,
                    window_events,
                    hotkey_events,
                )
            })
        },
    )
    .context("failed to open the dock window")?;

    Ok(())
}
