use std::sync::Arc;

use gpui::{
    Div, Pixels, RenderImage, Rgba, SharedString, Stateful, div, img, prelude::*, px, rgba,
};

use super::theme::Theme;
use crate::dock::DockItem;

pub fn render_app_item(
    index: usize,
    item: &DockItem,
    icon: Option<Arc<RenderImage>>,
    theme: &Theme,
    icon_size: Pixels,
    hover: f32,
) -> Stateful<Div> {
    let indicator = if item.is_active {
        Some(theme.active_indicator)
    } else if item.is_running {
        Some(theme.running_indicator)
    } else {
        None
    };

    let drawn_size = if theme.hover_zoom {
        icon_size + (icon_size * 0.2).min(px(8.0)) * hover
    } else {
        icon_size
    };
    let icon_offset = (icon_size - drawn_size) / 2.0;

    let icon = match icon {
        Some(icon) => img(icon).size(drawn_size).into_any_element(),
        None => div()
            .size(drawn_size)
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
            .w(px(18.0).min(icon_size))
            .rounded_full()
            .bg(indicator.unwrap_or_else(|| rgba(0x00000000))),
        crate::config::IndicatorStyle::Dot => div()
            .size(px(5.0))
            .rounded_full()
            .bg(indicator.unwrap_or_else(|| rgba(0x00000000))),
    };

    let tile_tint = |alpha| Rgba {
        a: alpha,
        ..theme.text
    };
    let resting_alpha = if item.is_active { 0.08 } else { 0.0 };
    let tile_alpha = if theme.hover_background {
        resting_alpha + (0.13 - resting_alpha) * hover
    } else {
        resting_alpha
    };

    div()
        .id((
            SharedString::from(item.key.to_string_lossy().into_owned()),
            index,
        ))
        .flex()
        .flex_col()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap_1()
        .p(px(4.0))
        .rounded(px(10.0_f32.min(f32::from(icon_size) / 3.0)))
        .bg(tile_tint(tile_alpha))
        .cursor_pointer()
        .active(|style| style.bg(tile_tint(0.20)))
        .child(
            div().relative().size(icon_size).child(
                div()
                    .absolute()
                    .left(icon_offset)
                    .top(icon_offset)
                    .child(icon),
            ),
        )
        .child(indicator_element)
}
