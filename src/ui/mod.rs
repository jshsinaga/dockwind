mod dock;

pub use dock::DockView;

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
) -> Result<()> {
    let monitor =
        win::monitors::MonitorInfo::primary().context("failed to read the primary monitor")?;
    let content_size = size(px(DOCK_WIDTH), px(DOCK_HEIGHT));
    let bounds = win::monitors::dock_rect(
        &monitor,
        content_size,
        config.appearance.margin_bottom as f32,
    );
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
            cx.new(|cx| DockView::new(cx, theme, blur, window_handle, config_updates))
        },
    )
    .context("failed to open the dock window")?;

    Ok(())
}
