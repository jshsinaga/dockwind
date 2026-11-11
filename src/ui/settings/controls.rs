use gpui::{
    AnyElement, App, ClickEvent, Context, FontWeight, IntoElement, SharedString, Window, div,
    prelude::*, px, rgba,
};

use super::{ActiveInput, SettingsView};
use crate::config::{BlurMode, IndicatorStyle};

pub const AVAILABLE_FONTS: &[(&str, &str)] = &[
    ("Default (System)", ""),
    ("Segoe UI", "Segoe UI"),
    ("Segoe UI Variable", "Segoe UI Variable"),
    ("Arial", "Arial"),
    ("Calibri", "Calibri"),
    ("Cascadia Code", "Cascadia Code"),
    ("Consolas", "Consolas"),
    ("Courier New", "Courier New"),
    ("Georgia", "Georgia"),
    ("Impact", "Impact"),
    ("Lucida Console", "Lucida Console"),
    ("Tahoma", "Tahoma"),
    ("Times New Roman", "Times New Roman"),
    ("Trebuchet MS", "Trebuchet MS"),
    ("Verdana", "Verdana"),
    ("Comic Sans MS", "Comic Sans MS"),
    ("Bahnschrift", "Bahnschrift"),
    ("Century Gothic", "Century Gothic"),
    ("Franklin Gothic Medium", "Franklin Gothic Medium"),
    ("Palatino Linotype", "Palatino Linotype"),
];

pub const COLOR_PRESETS: &[(&str, &str)] = &[
    ("Catppuccin Mocha", "#1E1E2ECC"),
    ("Slate Dark", "#0F172ACC"),
    ("Midnight Blue", "#090D16EE"),
    ("Jet Black", "#000000D0"),
    ("Zinc Gray", "#18181BEE"),
];

pub const TIME_FORMAT_PRESETS: &[(&str, &str)] = &[
    ("24-Hour (14:30)", "%H:%M"),
    ("24-Hour + Seconds (14:30:45)", "%H:%M:%S"),
    ("12-Hour (02:30 PM)", "%I:%M %p"),
    ("12-Hour + Seconds (02:30:45 PM)", "%I:%M:%S %p"),
];

pub const DATE_FORMAT_PRESETS: &[(&str, &str)] = &[
    ("Short Day (Mon, 11 Nov 2026)", "%a, %d %b %Y"),
    ("Full Day (Monday, 11 November 2026)", "%A, %d %B %Y"),
    ("Numeric Date (11/11/2026)", "%d/%m/%Y"),
    ("ISO Format (2026-11-11)", "%Y-%m-%d"),
    ("Hide Date", ""),
];

pub const HOTKEY_PRESETS: &[&str] = &[
    "Ctrl+Alt+Shift+Q",
    "Ctrl+Shift+Q",
    "Win+Alt+Q",
    "Ctrl+Alt+Q",
];

#[derive(Debug, Clone, Copy)]
pub struct ThemePreset {
    pub name: &'static str,
    pub desc: &'static str,
    pub bg: &'static str,
    pub opacity: u8,
    pub blur: BlurMode,
    pub border_color: &'static str,
    pub border_width: u32,
    pub corner_radius: u32,
    pub shadow: bool,
    pub indicator: IndicatorStyle,
}

pub const THEME_PRESETS: &[ThemePreset] = &[
    ThemePreset {
        name: "Catppuccin Mocha",
        desc: "Deep slate background with lavender accents and soft blur",
        bg: "#1E1E2E",
        opacity: 85,
        blur: BlurMode::Acrylic,
        border_color: "#CBA6F740",
        border_width: 1,
        corner_radius: 16,
        shadow: true,
        indicator: IndicatorStyle::Line,
    },
    ThemePreset {
        name: "Frosted Glass",
        desc: "Translucent modern acrylic glass with crisp light borders",
        bg: "#FFFFFF",
        opacity: 20,
        blur: BlurMode::Acrylic,
        border_color: "#FFFFFF60",
        border_width: 1,
        corner_radius: 20,
        shadow: true,
        indicator: IndicatorStyle::Line,
    },
    ThemePreset {
        name: "Nordic Frost",
        desc: "Cool arctic slate with ice-blue border and dot indicators",
        bg: "#2E3440",
        opacity: 90,
        blur: BlurMode::Acrylic,
        border_color: "#88C0D050",
        border_width: 1,
        corner_radius: 16,
        shadow: true,
        indicator: IndicatorStyle::Dot,
    },
    ThemePreset {
        name: "OLED Pure Dark",
        desc: "Pitch-black background with sharp borders and no blur overhead",
        bg: "#000000",
        opacity: 100,
        blur: BlurMode::None,
        border_color: "#333333FF",
        border_width: 1,
        corner_radius: 12,
        shadow: false,
        indicator: IndicatorStyle::Line,
    },
    ThemePreset {
        name: "Midnight Azure",
        desc: "Deep navy glass with sky-blue highlights and subtle mica sheen",
        bg: "#0F172A",
        opacity: 80,
        blur: BlurMode::Mica,
        border_color: "#38BDF840",
        border_width: 1,
        corner_radius: 18,
        shadow: true,
        indicator: IndicatorStyle::Dot,
    },
];

pub fn render_card(
    title: &'static str,
    description: &'static str,
    rows: Vec<AnyElement>,
) -> AnyElement {
    div()
        .p_4()
        .rounded_xl()
        .bg(rgba(0x1E1E2EFF))
        .border_1()
        .border_color(rgba(0x313244FF))
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_sm()
                        .text_color(rgba(0xCDD6F4FF))
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgba(0x6C7086FF))
                        .child(description),
                ),
        )
        .children(rows)
        .into_any_element()
}

pub fn render_setting_row(
    title: &'static str,
    description: &'static str,
    control: impl IntoElement,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py_1()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(0xCDD6F4FF))
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgba(0x6C7086FF))
                        .child(description),
                ),
        )
        .child(control)
        .into_any_element()
}

pub fn render_text_input(
    id: &'static str,
    field: ActiveInput,
    current_value: &str,
    placeholder: &'static str,
    active_field: Option<ActiveInput>,
    input_buffer: &str,
    cx: &mut Context<SettingsView>,
) -> AnyElement {
    let is_active = active_field == Some(field);
    let display_text = if is_active {
        if input_buffer.is_empty() {
            "|".to_string()
        } else {
            format!("{input_buffer}|")
        }
    } else if current_value.is_empty() {
        placeholder.to_string()
    } else {
        current_value.to_string()
    };
    let is_empty = !is_active && current_value.is_empty();

    div()
        .id(id)
        .px_3()
        .py_1p5()
        .min_w(px(140.0))
        .rounded_md()
        .bg(rgba(0x11111BFF))
        .border_1()
        .border_color(if is_active {
            rgba(0x89B4FAFF)
        } else {
            rgba(0x313244FF)
        })
        .hover(|s| s.border_color(rgba(0x89B4FA80)))
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_between()
        .on_click(cx.listener(move |this: &mut SettingsView, _, _, cx| {
            this.active_input = Some(field);
            this.input_buffer = match field {
                ActiveInput::BackgroundHex => {
                    let hex = this.draft.appearance.background.clone();
                    let clean = hex.strip_prefix('#').unwrap_or(&hex);
                    if clean.len() >= 6 {
                        format!("#{}", &clean[..6])
                    } else {
                        hex
                    }
                }
                ActiveInput::BorderColorHex => this.draft.appearance.border_color.clone(),
                ActiveInput::ClockTimeFormat => this.draft.clock.time_format.clone(),
                ActiveInput::ClockDateFormat => this.draft.clock.date_format.clone(),
                ActiveInput::ClockFontColor => {
                    this.draft.clock.font_color.clone().unwrap_or_default()
                }
                ActiveInput::AppPickerSearch => this.app_picker_search.clone(),
            };
            this.status_message = None;
            cx.notify();
        }))
        .child(
            div()
                .text_xs()
                .font_family(SharedString::from("monospace"))
                .text_color(if is_active {
                    rgba(0xFFFFFFFF)
                } else if is_empty {
                    rgba(0x6C7086FF)
                } else {
                    rgba(0xCDD6F4FF)
                })
                .child(display_text),
        )
        .into_any_element()
}

pub fn render_stepper(
    id_prefix: &'static str,
    value_display: String,
    on_minus: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_plus: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .id(SharedString::from(format!("{id_prefix}-minus")))
                .w(px(28.0))
                .h(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(rgba(0x313244FF))
                .text_color(rgba(0xCDD6F4FF))
                .cursor_pointer()
                .hover(|s| s.bg(rgba(0x45475AFF)))
                .on_click(on_minus)
                .child("-"),
        )
        .child(
            div()
                .w(px(80.0))
                .h(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(rgba(0x181825FF))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(0xFFFFFFFF))
                .child(value_display),
        )
        .child(
            div()
                .id(SharedString::from(format!("{id_prefix}-plus")))
                .w(px(28.0))
                .h(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(rgba(0x313244FF))
                .text_color(rgba(0xCDD6F4FF))
                .cursor_pointer()
                .hover(|s| s.bg(rgba(0x45475AFF)))
                .on_click(on_plus)
                .child("+"),
        )
        .into_any_element()
}

pub fn render_toggle(
    id: &'static str,
    checked: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let (track_bg, thumb_x) = if checked {
        (rgba(0x89B4FAFF), px(21.0))
    } else {
        (rgba(0x45475AFF), px(3.0))
    };
    div()
        .id(id)
        .w(px(44.0))
        .h(px(24.0))
        .rounded_full()
        .bg(track_bg)
        .relative()
        .cursor_pointer()
        .on_click(on_toggle)
        .child(
            div()
                .absolute()
                .top(px(2.0))
                .left(thumb_x)
                .w(px(20.0))
                .h(px(20.0))
                .rounded_full()
                .bg(rgba(0x11111BFF)),
        )
        .into_any_element()
}

pub fn render_choice_pill(
    id: &'static str,
    label: &'static str,
    is_active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .bg(if is_active {
            rgba(0x313244FF)
        } else {
            rgba(0x00000000)
        })
        .text_color(if is_active {
            rgba(0xFFFFFFFF)
        } else {
            rgba(0xA6ADC8FF)
        })
        .hover(|s| {
            if !is_active {
                s.bg(rgba(0x1E1E2EFF))
            } else {
                s
            }
        })
        .on_click(on_click)
        .child(label)
        .into_any_element()
}
