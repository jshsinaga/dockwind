use std::sync::Arc;

use gpui::{
    App, ClickEvent, MouseButton, MouseDownEvent, Pixels, RenderImage, SharedString, Window, div,
    img, prelude::*, px, rgba,
};

use super::theme::Theme;
use crate::dock::DockItem;

pub fn render_app_item(
    item: &DockItem,
    icon: Option<Arc<RenderImage>>,
    theme: &Theme,
    icon_size: Pixels,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_middle_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let indicator = if item.is_active {
        Some(theme.active_indicator)
    } else if item.is_running {
        Some(theme.running_indicator)
    } else {
        None
    };

    let icon = match icon {
        Some(icon) => img(icon).size(icon_size).into_any_element(),
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

    let indicator_element = match theme.indicator_style {
        crate::config::IndicatorStyle::Line => div()
            .h(px(3.0))
            .w(px(18.0))
            .rounded_full()
            .bg(indicator.unwrap_or_else(|| rgba(0x00000000))),
        crate::config::IndicatorStyle::Dot => div()
            .size(px(5.0))
            .rounded_full()
            .bg(indicator.unwrap_or_else(|| rgba(0x00000000))),
    };

    div()
        .id(SharedString::from(item.key.to_string_lossy().to_string()))
        .relative()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .p(px(4.0))
        .rounded(px(8.0))
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xFFFFFF18)).top(px(-2.0)))
        .on_click(on_click)
        .on_mouse_down(MouseButton::Middle, on_middle_click)
        .child(icon)
        .child(indicator_element)
}
