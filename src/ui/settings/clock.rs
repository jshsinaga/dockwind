use chrono::Local;
use gpui::{Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgba};

use super::controls::*;
use super::{ActiveInput, SettingsView};
use crate::config::{ClockAlignment, VerticalAlign, parse_rgba_hex};

impl SettingsView {
    pub(super) fn render_clock_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Local::now();
        let time_display = now.format(&self.draft.clock.time_format).to_string();
        let date_display = if self.draft.clock.date_format.is_empty() {
            String::new()
        } else {
            now.format(&self.draft.clock.date_format).to_string()
        };

        let mut preview_root = div().h(px(80.0)).flex().flex_col();
        preview_root = match self.draft.clock.alignment {
            ClockAlignment::Left => preview_root.items_start(),
            ClockAlignment::Center => preview_root.items_center(),
            ClockAlignment::Right => preview_root.items_end(),
        };
        preview_root = match self.draft.clock.vertical_align {
            VerticalAlign::Center => preview_root.justify_center(),
            VerticalAlign::Top => preview_root.justify_start(),
            VerticalAlign::Bottom => preview_root.justify_end(),
        };

        let custom_color = self
            .draft
            .clock
            .font_color
            .as_ref()
            .and_then(|c| parse_rgba_hex(c))
            .map(|[r, g, b, a]| rgba(u32::from_be_bytes([r, g, b, a])));

        let mut preview_time_el = div()
            .text_size(px(self.draft.clock.time_font_size as f32))
            .text_color(custom_color.unwrap_or_else(|| rgba(0xFFFFFFFF)))
            .child(time_display);

        let mut preview_date_el = div()
            .text_size(px(self.draft.clock.date_font_size as f32))
            .text_color(custom_color.unwrap_or_else(|| rgba(0xA6ADC8FF)))
            .child(date_display);

        if self.draft.clock.bold {
            preview_time_el = preview_time_el.font_weight(FontWeight::BOLD);
            preview_date_el = preview_date_el.font_weight(FontWeight::BOLD);
        }
        if self.draft.clock.italic {
            preview_time_el = preview_time_el.italic();
            preview_date_el = preview_date_el.italic();
        }
        if self.draft.clock.underline {
            preview_time_el = preview_time_el.underline();
            preview_date_el = preview_date_el.underline();
        }

        if !self.draft.clock.font_family.trim().is_empty() {
            let fam = SharedString::from(self.draft.clock.font_family.trim().to_string());
            preview_time_el = preview_time_el.font_family(fam.clone());
            preview_date_el = preview_date_el.font_family(fam);
        }

        match self.draft.clock.alignment {
            ClockAlignment::Left => {
                preview_time_el = preview_time_el.text_left();
                preview_date_el = preview_date_el.text_left();
            }
            ClockAlignment::Center => {
                preview_time_el = preview_time_el.text_center();
                preview_date_el = preview_date_el.text_center();
            }
            ClockAlignment::Right => {
                preview_time_el = preview_time_el.text_right();
                preview_date_el = preview_date_el.text_right();
            }
        }

        let half_gap = self.draft.clock.line_gap as f32 / 2.0;
        if self.draft.clock.line_gap != 0 {
            preview_time_el = preview_time_el.relative().top(px(-half_gap));
            preview_date_el = preview_date_el.relative().top(px(half_gap));
        }

        let preview_content = preview_root.child(preview_time_el).child(preview_date_el);

        let cheat_sheet_tokens = [
            ("%H", "24-hr hour (00-23)"),
            ("%I", "12-hr hour (01-12)"),
            ("%M", "Minute (00-59)"),
            ("%S", "Second (00-59)"),
            ("%p", "AM / PM marker"),
            ("%d", "Day of month (01-31)"),
            ("%m", "Month number (01-12)"),
            ("%b", "Short month (Nov)"),
            ("%B", "Full month (November)"),
            ("%Y", "4-digit year (2026)"),
            ("%a", "Short weekday (Wed)"),
            ("%A", "Full weekday (Wednesday)"),
        ];

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .p_5()
                    .rounded_xl()
                    .bg(rgba(0x11111BFF))
                    .border_1()
                    .border_color(rgba(0x89B4FA40))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgba(0x6C7086FF))
                            .child(if self.draft.clock.show {
                                "Clock Widget Live Preview"
                            } else {
                                "Clock Widget Live Preview (Currently Hidden on Dock)"
                            }),
                    )
                    .child(preview_content),
            )
            .child(render_card(
                "Clock Widget Visibility",
                "Enable or disable the time and date display on the dock.",
                vec![
                    render_setting_row(
                        "Show Clock Widget",
                        "Display the time and date widget beside your dock applications.",
                        render_toggle(
                            "toggle-clock-show",
                            self.draft.clock.show,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.clock.show = !this.draft.clock.show;
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                ],
            ))
            .child(render_card(
                "Time Format & Custom Pattern",
                "Select a preset format or customize the strftime time pattern.",
                vec![
                    render_setting_row(
                        "Time Pattern",
                        "Current pattern used to format the time line.",
                        render_text_input(
                            "input-time-pattern",
                            ActiveInput::ClockTimeFormat,
                            &self.draft.clock.time_format,
                            "%H:%M",
                            self.active_input,
                            &self.input_buffer,
                            cx,
                        ),
                    ),
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .pt_2()
                        .children(TIME_FORMAT_PRESETS.iter().map(|(label, format)| {
                            let is_active = self.draft.clock.time_format == *format;
                            let fmt = (*format).to_string();
                            div()
                                .id(SharedString::from(format!("time-fmt-{format}")))
                                .px_3()
                                .py_1p5()
                                .rounded_lg()
                                .bg(if is_active {
                                    rgba(0x313244FF)
                                } else {
                                    rgba(0x18182580)
                                })
                                .border_1()
                                .border_color(if is_active {
                                    rgba(0x89B4FA80)
                                } else {
                                    rgba(0x31324440)
                                })
                                .hover(|s| s.bg(rgba(0x313244FF)))
                                .cursor_pointer()
                                .text_xs()
                                .text_color(if is_active {
                                    rgba(0xFFFFFFFF)
                                } else {
                                    rgba(0xCDD6F4FF)
                                })
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.draft.clock.time_format = fmt.clone();
                                    this.status_message = None;
                                    cx.notify();
                                }))
                                .child(*label)
                        }))
                        .into_any_element(),
                ],
            ))
            .child(render_card(
                "Date Format & Custom Pattern",
                "Select a preset format or customize the strftime date pattern.",
                vec![
                    render_setting_row(
                        "Date Pattern",
                        "Current pattern used to format the date line.",
                        render_text_input(
                            "input-date-pattern",
                            ActiveInput::ClockDateFormat,
                            &self.draft.clock.date_format,
                            "%a, %d %b %Y",
                            self.active_input,
                            &self.input_buffer,
                            cx,
                        ),
                    ),
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .pt_2()
                        .children(DATE_FORMAT_PRESETS.iter().map(|(label, format)| {
                            let is_active = self.draft.clock.date_format == *format;
                            let fmt = (*format).to_string();
                            div()
                                .id(SharedString::from(format!("date-fmt-{label}")))
                                .px_3()
                                .py_1p5()
                                .rounded_lg()
                                .bg(if is_active {
                                    rgba(0x313244FF)
                                } else {
                                    rgba(0x18182580)
                                })
                                .border_1()
                                .border_color(if is_active {
                                    rgba(0x89B4FA80)
                                } else {
                                    rgba(0x31324440)
                                })
                                .hover(|s| s.bg(rgba(0x313244FF)))
                                .cursor_pointer()
                                .text_xs()
                                .text_color(if is_active {
                                    rgba(0xFFFFFFFF)
                                } else {
                                    rgba(0xCDD6F4FF)
                                })
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.draft.clock.date_format = fmt.clone();
                                    this.status_message = None;
                                    cx.notify();
                                }))
                                .child(*label)
                        }))
                        .into_any_element(),
                ],
            ))
            .child(render_card(
                "Format Pattern Cheat Sheet & Guide",
                "Common strftime format specifiers for customizing your patterns.",
                vec![
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .children(cheat_sheet_tokens.iter().map(|(token, desc)| {
                            div()
                                .px_2p5()
                                .py_1p5()
                                .rounded_md()
                                .bg(rgba(0x181825FF))
                                .border_1()
                                .border_color(rgba(0x31324480))
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .font_family(SharedString::from("monospace"))
                                        .font_weight(FontWeight::BOLD)
                                        .text_xs()
                                        .text_color(rgba(0x89B4FAFF))
                                        .child(*token),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgba(0xA6ADC8FF))
                                        .child(*desc),
                                )
                        }))
                        .into_any_element(),
                ],
            ))
            .child(render_card(
                "Layout & Line Spacing",
                "Adjust vertical gap and text alignment between time and date lines.",
                vec![
                    render_setting_row(
                        "Text Alignment",
                        "Align clock and date text to the Left, Center, or Right.",
                        div()
                            .flex()
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .bg(rgba(0x181825FF))
                            .child(render_choice_pill(
                                "pill-clock-align-left",
                                "Left",
                                self.draft.clock.alignment == ClockAlignment::Left,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.alignment = ClockAlignment::Left;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-clock-align-center",
                                "Center",
                                self.draft.clock.alignment == ClockAlignment::Center,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.alignment = ClockAlignment::Center;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-clock-align-right",
                                "Right",
                                self.draft.clock.alignment == ClockAlignment::Right,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.alignment = ClockAlignment::Right;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            )),
                    ),
                    render_setting_row(
                        "Vertical Alignment",
                        "Align clock and date vertically to the Top, Center, or Bottom of the dock.",
                        div()
                            .flex()
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .bg(rgba(0x181825FF))
                            .child(render_choice_pill(
                                "pill-clock-valign-top",
                                "Top",
                                self.draft.clock.vertical_align == VerticalAlign::Top,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.vertical_align = VerticalAlign::Top;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-clock-valign-center",
                                "Center",
                                self.draft.clock.vertical_align == VerticalAlign::Center,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.vertical_align = VerticalAlign::Center;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-clock-valign-bottom",
                                "Bottom",
                                self.draft.clock.vertical_align == VerticalAlign::Bottom,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.vertical_align = VerticalAlign::Bottom;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            )),
                    ),
                    render_setting_row(
                        "Line Gap",
                        "Spacing between time and date lines (-16 to 24 px). Negative values overlap closer.",
                        render_stepper(
                            "clock-line-gap",
                            format!("{} px", self.draft.clock.line_gap),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.clock.line_gap = (this.draft.clock.line_gap - 1).max(-16);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.clock.line_gap = (this.draft.clock.line_gap + 1).min(24);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                ],
            ))
            .child(render_card(
                "Typography & Font Styling",
                "Customize font family, custom text color, and style variations.",
                vec![
                    render_setting_row(
                        "Font Family",
                        "Choose a system font with real-time typography preview.",
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .id("font-family-select-btn")
                                    .px_3()
                                    .py_1p5()
                                    .min_w(px(200.0))
                                    .rounded_lg()
                                    .bg(rgba(0x181825FF))
                                    .border_1()
                                    .border_color(if self.font_dropdown_open {
                                        rgba(0x89B4FAFF)
                                    } else {
                                        rgba(0x313244FF)
                                    })
                                    .hover(|s| s.bg(rgba(0x313244FF)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.font_dropdown_open = !this.font_dropdown_open;
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgba(0xCDD6F4FF))
                                            .child(if self.draft.clock.font_family.is_empty() {
                                                "Default (System)".to_string()
                                            } else {
                                                self.draft.clock.font_family.clone()
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgba(0x6C7086FF))
                                            .child(if self.font_dropdown_open {
                                                "▲"
                                            } else {
                                                "▼"
                                            }),
                                    ),
                            )
                            .children(if self.font_dropdown_open {
                                Some(
                                    div()
                                        .id("font-family-dropdown-scroll")
                                        .max_h(px(220.0))
                                        .w(px(280.0))
                                        .overflow_y_scroll()
                                        .rounded_lg()
                                        .bg(rgba(0x11111BFF))
                                        .border_1()
                                        .border_color(rgba(0x313244FF))
                                        .p_1()
                                        .flex()
                                        .flex_col()
                                        .gap_0p5()
                                        .children(AVAILABLE_FONTS.iter().map(|(label, font_name)| {
                                            let is_selected = self.draft.clock.font_family == *font_name;
                                            let fname = (*font_name).to_string();
                                            let row = div()
                                                .id(SharedString::from(format!("font-opt-{label}")))
                                                .px_2p5()
                                                .py_1p5()
                                                .rounded_md()
                                                .bg(if is_selected {
                                                    rgba(0x313244FF)
                                                } else {
                                                    rgba(0x00000000)
                                                })
                                                .hover(|s| s.bg(rgba(0x45475AFF)))
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                                    this.draft.clock.font_family = fname.clone();
                                                    this.font_dropdown_open = false;
                                                    this.status_message = None;
                                                    cx.notify();
                                                }));

                                            let mut label_el = div()
                                                .text_xs()
                                                .text_color(if is_selected {
                                                    rgba(0xFFFFFFFF)
                                                } else {
                                                    rgba(0xCDD6F4FF)
                                                });
                                            if !font_name.is_empty() {
                                                label_el = label_el.font_family(SharedString::from(font_name.to_string()));
                                            }

                                            let mut sample_el = div()
                                                .text_xs()
                                                .text_color(rgba(0x6C7086FF));
                                            if !font_name.is_empty() {
                                                sample_el = sample_el.font_family(SharedString::from(font_name.to_string()));
                                            }

                                            row.child(label_el.child(*label))
                                                .child(sample_el.child("12:34 PM"))
                                        })),
                                )
                            } else {
                                None
                            }),
                    ),
                    render_setting_row(
                        "Font Color",
                        "Hex color code for clock text (e.g. #FFFFFF). Leave empty for adaptive contrast.",
                        render_text_input(
                            "input-font-color",
                            ActiveInput::ClockFontColor,
                            self.draft.clock.font_color.as_deref().unwrap_or(""),
                            "Adaptive Contrast",
                            self.active_input,
                            &self.input_buffer,
                            cx,
                        ),
                    ),
                    render_setting_row(
                        "Bold Weight",
                        "Render clock text with bold stroke weight.",
                        div()
                            .flex()
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .bg(rgba(0x181825FF))
                            .child(render_choice_pill(
                                "pill-bold-normal",
                                "Normal",
                                !self.draft.clock.bold,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.bold = false;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-bold-bold",
                                "Bold",
                                self.draft.clock.bold,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.bold = true;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            )),
                    ),
                    render_setting_row(
                        "Italic Style",
                        "Render clock text with italic slant.",
                        div()
                            .flex()
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .bg(rgba(0x181825FF))
                            .child(render_choice_pill(
                                "pill-italic-normal",
                                "Normal",
                                !self.draft.clock.italic,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.italic = false;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-italic-italic",
                                "Italic",
                                self.draft.clock.italic,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.italic = true;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            )),
                    ),
                    render_setting_row(
                        "Underline Style",
                        "Render clock text with underline decoration.",
                        div()
                            .flex()
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .bg(rgba(0x181825FF))
                            .child(render_choice_pill(
                                "pill-underline-normal",
                                "Normal",
                                !self.draft.clock.underline,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.underline = false;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-underline-underline",
                                "Underline",
                                self.draft.clock.underline,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.clock.underline = true;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            )),
                    ),
                    render_setting_row(
                        "Time Font Size",
                        "Size of the time display text in pixels (8 - 64 px).",
                        render_stepper(
                            "clock-time-font-size",
                            format!("{} px", self.draft.clock.time_font_size),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.clock.time_font_size =
                                    this.draft.clock.time_font_size.saturating_sub(1).max(8);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.clock.time_font_size =
                                    (this.draft.clock.time_font_size + 1).min(64);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Date Font Size",
                        "Size of the date display text in pixels (8 - 40 px).",
                        render_stepper(
                            "clock-date-font-size",
                            format!("{} px", self.draft.clock.date_font_size),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.clock.date_font_size =
                                    this.draft.clock.date_font_size.saturating_sub(1).max(8);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.clock.date_font_size =
                                    (this.draft.clock.date_font_size + 1).min(40);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                ],
            ))
    }
}
