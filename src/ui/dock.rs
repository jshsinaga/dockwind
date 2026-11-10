use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    AnyWindowHandle, ClickEvent, Context, Entity, Pixels, Render, Timer, Window, div, prelude::*,
    px, rgba,
};
use tracing::{debug, error, info, warn};

use super::app_item::render_app_item;
use super::clock::ClockView;
use crate::app_state::{self, DockItem};
use crate::config::Config;
use crate::theme::Theme;
use crate::win::{self, icons::IconCache};

pub struct DockView {
    theme: Theme,
    blur: crate::config::BlurMode,
    clock: Entity<ClockView>,
    pinned: Vec<crate::config::PinnedApp>,
    items: Vec<DockItem>,
    icons: IconCache,
    icon_size: Pixels,
}

impl DockView {
    pub fn new(
        cx: &mut Context<Self>,
        theme: Theme,
        window_handle: AnyWindowHandle,
        config: Config,
        mut config_updates: UnboundedReceiver<Config>,
        mut window_events: UnboundedReceiver<()>,
    ) -> Self {
        let clock = cx.new(|cx| ClockView::new(cx, &config.clock));
        let mut this = Self {
            theme,
            blur: config.appearance.blur,
            clock,
            pinned: config.pinned.clone(),
            items: Vec::new(),
            icons: IconCache::default(),
            icon_size: px(config.appearance.icon_size as f32),
        };
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
