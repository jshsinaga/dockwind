mod win;

use std::time::Duration;

use futures::StreamExt;
use gpui::{
    App, Application, Bounds, Context, Render, SharedString, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, point, prelude::*,
    px, rgba, size,
};

const DOCK_WIDTH: f32 = 560.0;
const DOCK_HEIGHT: f32 = 92.0;
const DOCK_MARGIN_BOTTOM: f32 = 12.0;
const CORNER_RADIUS: f32 = 20.0;

struct SpikeView {
    tick: u64,
}

impl SpikeView {
    fn new(cx: &mut Context<Self>) -> Self {
        let (tx, mut rx) = futures::channel::mpsc::unbounded();

        std::thread::Builder::new()
            .name("m0-ui-wake".into())
            .spawn(move || {
                let mut tick = 0u64;
                loop {
                    std::thread::sleep(Duration::from_millis(500));
                    tick += 1;
                    if tx.unbounded_send(tick).is_err() {
                        break;
                    }
                }
            })
            .expect("failed to spawn M0 wake thread");

        cx.spawn(async move |this, cx| {
            while let Some(tick) = rx.next().await {
                let updated = this.update(cx, |this, cx| {
                    this.tick = tick;
                    cx.notify();
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();

        Self { tick: 0 }
    }
}

impl Render for SpikeView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            div()
                .size_full()
                .rounded(px(CORNER_RADIUS))
                .bg(rgba(0x1E1E2EE6))
                .border_1()
                .border_color(rgba(0xFFFFFF33))
                .flex()
                .items_center()
                .justify_center()
                .gap_4()
                .text_color(rgba(0xFFFFFFFF))
                .child(div().size(px(48.)).rounded(px(12.)).bg(rgba(0x4C9AFFFF)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child("Dockwind M0")
                        .child(format!("UI wake tick: {}", self.tick)),
                ),
        )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let display = cx.primary_display().expect("no primary display");
        let display_bounds = display.bounds();
        let dock_size = size(px(DOCK_WIDTH), px(DOCK_HEIGHT));
        let dock_origin = point(
            display_bounds.origin.x + (display_bounds.size.width - dock_size.width) / 2.0,
            display_bounds.origin.y + display_bounds.size.height
                - dock_size.height
                - px(DOCK_MARGIN_BOTTOM),
        );
        let dock_bounds = Bounds::new(dock_origin, dock_size);
        let previous_foreground = win::foreground_window();

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(dock_bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Dockwind M0")),
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
                win::configure_dock_window(window, previous_foreground)
                    .expect("failed to apply M0 window styles");
                cx.new(SpikeView::new)
            },
        )
        .expect("failed to open M0 window");
    });
}
