use gpui::{App, ClickEvent, Pixels, SharedString, Window, div, img, prelude::*, px, rgba};

use crate::app_state::DockItem;
use crate::theme::Theme;

pub fn render_app_item(
    item: &DockItem,
    theme: &Theme,
    icon_size: Pixels,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let indicator = if item.is_active {
        Some(theme.active_indicator)
    } else if item.is_running {
        Some(theme.running_indicator)
    } else {
        None
    };

    let icon = match &item.icon {
        Some(icon) => img(icon.clone()).size(icon_size).into_any_element(),
        None => div()
            .size(icon_size)
            .rounded(px(8.0))
            .bg(theme.running_indicator)
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.text)
            .child(
                item.name
                    .chars()
                    .next()
                    .map(|character| character.to_uppercase().to_string())
                    .unwrap_or_else(|| "?".to_string()),
            )
            .into_any_element(),
    };

    div()
        .id(SharedString::from(item.key.to_string_lossy().to_string()))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .p(px(4.0))
        .rounded(px(8.0))
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xFFFFFF14)))
        .on_click(on_click)
        .child(icon)
        .child(
            div()
                .h(px(3.0))
                .w(px(18.0))
                .rounded_full()
                .bg(indicator.unwrap_or_else(|| rgba(0x00000000))),
        )
}
