use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{AnyWindowHandle, Context, Entity, Render, Window, div, prelude::*, rgba};
use tracing::{error, info, warn};

use super::clock::ClockView;
use crate::config::{BlurMode, Clock, Config};
use crate::theme::Theme;
use crate::win;

pub struct DockView {
    theme: Theme,
    blur: BlurMode,
    clock: Entity<ClockView>,
}

impl DockView {
    pub fn new(
        cx: &mut Context<Self>,
        theme: Theme,
        blur: BlurMode,
        window_handle: AnyWindowHandle,
        clock_config: Clock,
        mut updates: UnboundedReceiver<Config>,
    ) -> Self {
        let clock = cx.new(|cx| ClockView::new(cx, &clock_config));

        cx.spawn(async move |this, cx| {
            while let Some(config) = updates.next().await {
                match Theme::from_config(&config.appearance) {
                    Ok(theme) => {
                        let blur_changed = this.update(cx, |this, cx| {
                            let blur_changed = this.blur != config.appearance.blur;
                            this.blur = config.appearance.blur;
                            this.theme = theme;
                            this.clock
                                .update(cx, |clock, cx| clock.set_config(&config.clock, cx));
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

        Self { theme, blur, clock }
    }
}

impl Render for DockView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            div()
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
                .child(div().flex().flex_col().child("Dockwind").child("M3 clock"))
                .child(self.clock.clone()),
        )
    }
}
