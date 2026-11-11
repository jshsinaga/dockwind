use gpui::{Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgba};

use super::controls::*;
use super::{ActiveInput, SettingsView};
use crate::config::{
    AlignmentAnchor, BlurMode, DockEdge, IndicatorStyle, ItemAlignment, VerticalAlign, WidthMode,
    parse_rgba_hex,
};
use crate::win;

impl SettingsView {
    pub(super) fn render_appearance_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current_orientation = self.draft.appearance.orientation;
        let current_blur = self.draft.appearance.blur;
        let current_width_mode = self.draft.appearance.width_mode;
        let current_vertical_align = self.draft.appearance.vertical_align;
        let current_indicator = self.draft.appearance.indicator_style;
        let current_alignment = self.draft.appearance.item_alignment;
        let current_anchor = self.draft.appearance.alignment_anchor;

        let background_hex = {
            let full = &self.draft.appearance.background;
            let clean = full.strip_prefix('#').unwrap_or(full);
            if clean.len() >= 6 {
                format!("#{}", &clean[..6])
            } else {
                format!("#{clean}")
            }
        };
        let parsed_color = parse_rgba_hex(&self.draft.appearance.background)
            .map(|[r, g, b, _]| rgba(u32::from_be_bytes([r, g, b, 0xFF])))
            .unwrap_or_else(|| rgba(0x1E1E2ECC));

        let border_hex = self.draft.appearance.border_color.clone();
        let parsed_border_color = parse_rgba_hex(&border_hex)
            .map(|[r, g, b, a]| rgba(u32::from_be_bytes([r, g, b, a])))
            .unwrap_or_else(|| rgba(0xFFFFFF33));

        let (pt, pb, pl, pr) = self.draft.appearance.effective_padding();

        let mut dimension_rows = vec![render_setting_row(
            "Width Mode",
            "Choose fixed pixel width or automatic width that hugs icons.",
            div()
                .flex()
                .gap_1()
                .p_1()
                .rounded_lg()
                .bg(rgba(0x181825FF))
                .child(render_choice_pill(
                    "pill-width-manual",
                    "Manual",
                    current_width_mode == WidthMode::Manual,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.appearance.width_mode = WidthMode::Manual;
                        this.status_message = None;
                        cx.notify();
                    }),
                ))
                .child(render_choice_pill(
                    "pill-width-auto",
                    "Auto",
                    current_width_mode == WidthMode::Auto,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.appearance.width_mode = WidthMode::Auto;
                        this.status_message = None;
                        cx.notify();
                    }),
                )),
        )];

        if current_width_mode == WidthMode::Manual {
            dimension_rows.push(render_setting_row(
                "Dock Width",
                "Total dock width in pixels (200 - 3840 px).",
                render_stepper(
                    "dock-width",
                    format!("{} px", self.draft.appearance.width),
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.appearance.width =
                            this.draft.appearance.width.saturating_sub(20).max(200);
                        this.status_message = None;
                        cx.notify();
                    }),
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.appearance.width = (this.draft.appearance.width + 20).min(3840);
                        this.status_message = None;
                        cx.notify();
                    }),
                ),
            ));
        }

        dimension_rows.push(render_setting_row(
            "Dock Height",
            "Total dock container height (40 - 200 px).",
            render_stepper(
                "dock-height",
                format!("{} px", self.draft.appearance.height),
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.draft.appearance.height =
                        this.draft.appearance.height.saturating_sub(4).max(40);
                    this.status_message = None;
                    cx.notify();
                }),
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.draft.appearance.height = (this.draft.appearance.height + 4).min(200);
                    this.status_message = None;
                    cx.notify();
                }),
            ),
        ));

        dimension_rows.push(render_setting_row(
            "Content Vertical Placement",
            "Vertical alignment of icons and content within the dock height.",
            div()
                .flex()
                .gap_1()
                .p_1()
                .rounded_lg()
                .bg(rgba(0x181825FF))
                .child(render_choice_pill(
                    "pill-valign-top",
                    "Top",
                    current_vertical_align == VerticalAlign::Top,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.appearance.vertical_align = VerticalAlign::Top;
                        this.status_message = None;
                        cx.notify();
                    }),
                ))
                .child(render_choice_pill(
                    "pill-valign-center",
                    "Center",
                    current_vertical_align == VerticalAlign::Center,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.appearance.vertical_align = VerticalAlign::Center;
                        this.status_message = None;
                        cx.notify();
                    }),
                ))
                .child(render_choice_pill(
                    "pill-valign-bottom",
                    "Bottom",
                    current_vertical_align == VerticalAlign::Bottom,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.appearance.vertical_align = VerticalAlign::Bottom;
                        this.status_message = None;
                        cx.notify();
                    }),
                )),
        ));

        dimension_rows.push(render_setting_row(
            "Corner Radius",
            "Outer corner rounding of the dock container (0 - 64 px).",
            render_stepper(
                "corner-radius",
                format!("{} px", self.draft.appearance.corner_radius),
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.draft.appearance.corner_radius =
                        this.draft.appearance.corner_radius.saturating_sub(2);
                    this.status_message = None;
                    cx.notify();
                }),
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.draft.appearance.corner_radius =
                        (this.draft.appearance.corner_radius + 2).min(64);
                    this.status_message = None;
                    cx.notify();
                }),
            ),
        ));

        dimension_rows.push(render_setting_row(
            "Screen Offset",
            "Distance between the dock and monitor edge (0 - 120 px).",
            render_stepper(
                "dock-offset",
                format!("{} px", self.draft.appearance.offset),
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.draft.appearance.offset = this.draft.appearance.offset.saturating_sub(2);
                    this.status_message = None;
                    cx.notify();
                }),
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.draft.appearance.offset = (this.draft.appearance.offset + 2).min(120);
                    this.status_message = None;
                    cx.notify();
                }),
            ),
        ));

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(render_card(
                "Theme & Style Presets",
                "Apply curated aesthetic presets across color, backdrop, corners, and indicators.",
                vec![
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .children(THEME_PRESETS.iter().map(|preset| {
                            let is_active = self.draft.appearance.background == preset.bg
                                && self.draft.appearance.blur == preset.blur
                                && self.draft.appearance.indicator_style == preset.indicator;
                            let p = *preset;
                            div()
                                .id(SharedString::from(format!("theme-preset-{}", preset.name)))
                                .px_3()
                                .py_2()
                                .rounded_lg()
                                .bg(if is_active {
                                    rgba(0x313244FF)
                                } else {
                                    rgba(0x181825FF)
                                })
                                .border_1()
                                .border_color(if is_active {
                                    rgba(0x89B4FA80)
                                } else {
                                    rgba(0x313244FF)
                                })
                                .hover(|s| s.bg(rgba(0x313244FF)))
                                .cursor_pointer()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.draft.appearance.background = p.bg.to_string();
                                    this.draft.appearance.opacity = p.opacity;
                                    this.draft.appearance.blur = p.blur;
                                    this.draft.appearance.border_color = p.border_color.to_string();
                                    this.draft.appearance.border_width = p.border_width;
                                    this.draft.appearance.corner_radius = p.corner_radius;
                                    this.draft.appearance.shadow = p.shadow;
                                    this.draft.appearance.indicator_style = p.indicator;
                                    this.status_message = None;
                                    cx.notify();
                                }));
                            div()
                                .id(SharedString::from(format!("theme-preset-{}", preset.name)))
                                .px_3()
                                .py_2()
                                .rounded_lg()
                                .bg(if is_active {
                                    rgba(0x313244FF)
                                } else {
                                    rgba(0x181825FF)
                                })
                                .border_1()
                                .border_color(if is_active {
                                    rgba(0x89B4FA80)
                                } else {
                                    rgba(0x313244FF)
                                })
                                .hover(|s| s.bg(rgba(0x313244FF)))
                                .cursor_pointer()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.draft.appearance.background = p.bg.to_string();
                                    this.draft.appearance.opacity = p.opacity;
                                    this.draft.appearance.blur = p.blur;
                                    this.draft.appearance.border_color = p.border_color.to_string();
                                    this.draft.appearance.border_width = p.border_width;
                                    this.draft.appearance.corner_radius = p.corner_radius;
                                    this.draft.appearance.shadow = p.shadow;
                                    this.draft.appearance.indicator_style = p.indicator;
                                    this.status_message = None;
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .w(px(14.0))
                                                .h(px(14.0))
                                                .rounded_full()
                                                .bg(parse_rgba_hex(preset.bg)
                                                    .map(|[r, g, b, _]| {
                                                        rgba(u32::from_be_bytes([r, g, b, 0xFF]))
                                                    })
                                                    .unwrap_or_else(|| rgba(0x1E1E2EFF)))
                                                .border_1()
                                                .border_color(rgba(0xFFFFFF33)),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(if is_active {
                                                    rgba(0xFFFFFFFF)
                                                } else {
                                                    rgba(0xCDD6F4FF)
                                                })
                                                .child(preset.name),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgba(0x6C7086FF))
                                        .child(preset.desc),
                                )
                        }))
                        .into_any_element(),
                ],
            ))
            .child(render_card(
                "Dock Dimensions & Placement",
                "Configure dimensions, alignment, and physical placement of the dock.",
                dimension_rows,
            ))
            .child(render_card(
                "Position & Orientation",
                "Select dock edge placement on your primary display.",
                vec![render_setting_row(
                    "Screen Edge",
                    "Position the dock along the bottom or top of the screen.",
                    div()
                        .flex()
                        .gap_1()
                        .p_1()
                        .rounded_lg()
                        .bg(rgba(0x181825FF))
                        .child(render_choice_pill(
                            "pill-orientation-bottom",
                            "Bottom",
                            current_orientation == DockEdge::Bottom,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.orientation = DockEdge::Bottom;
                                this.status_message = None;
                                cx.notify();
                            }),
                        ))
                        .child(render_choice_pill(
                            "pill-orientation-top",
                            "Top",
                            current_orientation == DockEdge::Top,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.orientation = DockEdge::Top;
                                this.status_message = None;
                                cx.notify();
                            }),
                        )),
                )],
            ))
            .child(render_card(
                "Inner Padding",
                "Fine-tune spacing on each individual edge of the dock container.",
                vec![
                    render_setting_row(
                        "Top Padding",
                        "Padding between top dock border and content (0 - 48 px).",
                        render_stepper(
                            "dock-pad-top",
                            format!("{} px", pt),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                let (pt, pb, pl, pr) = this.draft.appearance.effective_padding();
                                this.draft.appearance.padding_top = Some(pt.saturating_sub(2));
                                this.draft.appearance.padding_bottom = Some(pb);
                                this.draft.appearance.padding_left = Some(pl);
                                this.draft.appearance.padding_right = Some(pr);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                let (pt, pb, pl, pr) = this.draft.appearance.effective_padding();
                                this.draft.appearance.padding_top = Some((pt + 2).min(48));
                                this.draft.appearance.padding_bottom = Some(pb);
                                this.draft.appearance.padding_left = Some(pl);
                                this.draft.appearance.padding_right = Some(pr);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Bottom Padding",
                        "Padding between bottom dock border and content (0 - 48 px).",
                        render_stepper(
                            "dock-pad-bottom",
                            format!("{} px", pb),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                let (pt, pb, pl, pr) = this.draft.appearance.effective_padding();
                                this.draft.appearance.padding_top = Some(pt);
                                this.draft.appearance.padding_bottom = Some(pb.saturating_sub(2));
                                this.draft.appearance.padding_left = Some(pl);
                                this.draft.appearance.padding_right = Some(pr);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                let (pt, pb, pl, pr) = this.draft.appearance.effective_padding();
                                this.draft.appearance.padding_top = Some(pt);
                                this.draft.appearance.padding_bottom = Some((pb + 2).min(48));
                                this.draft.appearance.padding_left = Some(pl);
                                this.draft.appearance.padding_right = Some(pr);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Left Padding",
                        "Padding between left dock border and content (0 - 64 px).",
                        render_stepper(
                            "dock-pad-left",
                            format!("{} px", pl),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                let (pt, pb, pl, pr) = this.draft.appearance.effective_padding();
                                this.draft.appearance.padding_top = Some(pt);
                                this.draft.appearance.padding_bottom = Some(pb);
                                this.draft.appearance.padding_left = Some(pl.saturating_sub(2));
                                this.draft.appearance.padding_right = Some(pr);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                let (pt, pb, pl, pr) = this.draft.appearance.effective_padding();
                                this.draft.appearance.padding_top = Some(pt);
                                this.draft.appearance.padding_bottom = Some(pb);
                                this.draft.appearance.padding_left = Some((pl + 2).min(64));
                                this.draft.appearance.padding_right = Some(pr);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Right Padding",
                        "Padding between right dock border and content (0 - 64 px).",
                        render_stepper(
                            "dock-pad-right",
                            format!("{} px", pr),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                let (pt, pb, pl, pr) = this.draft.appearance.effective_padding();
                                this.draft.appearance.padding_top = Some(pt);
                                this.draft.appearance.padding_bottom = Some(pb);
                                this.draft.appearance.padding_left = Some(pl);
                                this.draft.appearance.padding_right = Some(pr.saturating_sub(2));
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                let (pt, pb, pl, pr) = this.draft.appearance.effective_padding();
                                this.draft.appearance.padding_top = Some(pt);
                                this.draft.appearance.padding_bottom = Some(pb);
                                this.draft.appearance.padding_left = Some(pl);
                                this.draft.appearance.padding_right = Some((pr + 2).min(64));
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                ],
            ))
            .child(render_card(
                "Icons & Indicators",
                "Customize icon sizes, spacing, running indicators, and alignment.",
                vec![
                    render_setting_row(
                        "Icon Size",
                        "Application icon size on the dock (20 - 96 px).",
                        render_stepper(
                            "icon-size",
                            format!("{} px", self.draft.appearance.icon_size),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.icon_size =
                                    this.draft.appearance.icon_size.saturating_sub(4).max(20);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.icon_size =
                                    (this.draft.appearance.icon_size + 4).min(96);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Icon Spacing",
                        "Horizontal gap between adjacent app icons (0 - 32 px).",
                        render_stepper(
                            "icon-spacing",
                            format!("{} px", self.draft.appearance.item_spacing),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.item_spacing =
                                    this.draft.appearance.item_spacing.saturating_sub(2);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.item_spacing =
                                    (this.draft.appearance.item_spacing + 2).min(32);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Running Indicator Style",
                        "Visual marker beneath running and active applications.",
                        div()
                            .flex()
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .bg(rgba(0x181825FF))
                            .child(render_choice_pill(
                                "pill-indicator-line",
                                "Line",
                                current_indicator == IndicatorStyle::Line,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.appearance.indicator_style = IndicatorStyle::Line;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-indicator-dot",
                                "Dot",
                                current_indicator == IndicatorStyle::Dot,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.appearance.indicator_style = IndicatorStyle::Dot;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            )),
                    ),
                    render_setting_row(
                        "App Items Alignment",
                        "Horizontal alignment of app icons within the dock space.",
                        div()
                            .flex()
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .bg(rgba(0x181825FF))
                            .child(render_choice_pill(
                                "pill-align-left",
                                "Left",
                                current_alignment == ItemAlignment::Left,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.appearance.item_alignment = ItemAlignment::Left;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-align-center",
                                "Center",
                                current_alignment == ItemAlignment::Center,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.appearance.item_alignment = ItemAlignment::Center;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-align-right",
                                "Right",
                                current_alignment == ItemAlignment::Right,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.appearance.item_alignment = ItemAlignment::Right;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            )),
                    ),
                    render_setting_row(
                        "Alignment Reference",
                        "Align app items relative to the whole dock width or beside the clock.",
                        div()
                            .flex()
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .bg(rgba(0x181825FF))
                            .child(render_choice_pill(
                                "pill-anchor-dock",
                                "Dock (Absolute)",
                                current_anchor == AlignmentAnchor::Dock,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.appearance.alignment_anchor = AlignmentAnchor::Dock;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-anchor-clock",
                                "Clock (Relative)",
                                current_anchor == AlignmentAnchor::Clock,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.appearance.alignment_anchor = AlignmentAnchor::Clock;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            )),
                    ),
                ],
            ))
            .child(render_card(
                "Color & Transparency",
                "Customize the background color and transparency level.",
                vec![
                    render_setting_row(
                        "Opacity Level",
                        "Dock background opacity percentage (10% - 100%).",
                        render_stepper(
                            "dock-opacity",
                            format!("{}%", self.draft.appearance.opacity),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.opacity =
                                    this.draft.appearance.opacity.saturating_sub(5).max(10);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.opacity =
                                    (this.draft.appearance.opacity + 5).min(100);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Background Color",
                        "Input 6-digit hex code or choose using the color dialog.",
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .w(px(32.0))
                                    .h(px(32.0))
                                    .rounded_md()
                                    .bg(parsed_color)
                                    .border_1()
                                    .border_color(rgba(0xFFFFFF40)),
                            )
                            .child(render_text_input(
                                "input-bg-hex",
                                ActiveInput::BackgroundHex,
                                &background_hex,
                                "#1E1E2E",
                                self.active_input,
                                &self.input_buffer,
                                cx,
                            ))
                            .child(
                                div()
                                    .id("btn-pick-color")
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(rgba(0x313244FF))
                                    .hover(|s| s.bg(rgba(0x45475AFF)))
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgba(0xCDD6F4FF))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        let initial = this.draft.appearance.background.clone();
                                        let receiver = win::dialogs::pick_color_async(initial);
                                        cx.spawn(async move |this, cx| {
                                            if let Ok(Ok(Some(color))) = receiver.await {
                                                let _ = this.update(cx, |this, cx| {
                                                    this.draft.appearance.background = color;
                                                    this.status_message = None;
                                                    cx.notify();
                                                });
                                            }
                                        })
                                        .detach();
                                    }))
                                    .child("Choose Color..."),
                            ),
                    ),
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .pt_2()
                        .children(COLOR_PRESETS.iter().map(|(label, hex)| {
                            let color_hex = (*hex).to_string();
                            let is_active = self.draft.appearance.background == color_hex;
                            div()
                                .id(SharedString::from(format!("color-{label}")))
                                .px_2p5()
                                .py_1()
                                .rounded_md()
                                .bg(if is_active {
                                    rgba(0x313244FF)
                                } else {
                                    rgba(0x181825FF)
                                })
                                .border_1()
                                .border_color(if is_active {
                                    rgba(0x89B4FA80)
                                } else {
                                    rgba(0x313244FF)
                                })
                                .hover(|s| s.bg(rgba(0x313244FF)))
                                .cursor_pointer()
                                .text_xs()
                                .text_color(if is_active {
                                    rgba(0xFFFFFFFF)
                                } else {
                                    rgba(0xA6ADC8FF)
                                })
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.draft.appearance.background = color_hex.clone();
                                    this.status_message = None;
                                    cx.notify();
                                }))
                                .child(*label)
                        }))
                        .into_any_element(),
                ],
            ))
            .child(render_card(
                "Backdrop & Border",
                "Configure window transparency backdrop, drop shadow, and borders.",
                vec![
                    render_setting_row(
                        "Blur Mode",
                        "Material backdrop effect (Windows 10 Acrylic/Blur, Windows 11 DWM).",
                        div()
                            .flex()
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .bg(rgba(0x181825FF))
                            .child(render_choice_pill(
                                "pill-blur-none",
                                "None",
                                current_blur == BlurMode::None,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.appearance.blur = BlurMode::None;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-blur-mica",
                                "Mica",
                                current_blur == BlurMode::Mica,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.appearance.blur = BlurMode::Mica;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            ))
                            .child(render_choice_pill(
                                "pill-blur-acrylic",
                                "Acrylic",
                                current_blur == BlurMode::Acrylic,
                                cx.listener(|this: &mut Self, _, _, cx| {
                                    this.draft.appearance.blur = BlurMode::Acrylic;
                                    this.status_message = None;
                                    cx.notify();
                                }),
                            )),
                    ),
                    render_setting_row(
                        "Drop Shadow",
                        "Render an ambient soft drop shadow around the dock container.",
                        render_toggle(
                            "toggle-shadow",
                            self.draft.appearance.shadow,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.shadow = !this.draft.appearance.shadow;
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Border Width",
                        "Dock perimeter outline thickness (0 - 10 px).",
                        render_stepper(
                            "border-width",
                            format!("{} px", self.draft.appearance.border_width),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.border_width =
                                    this.draft.appearance.border_width.saturating_sub(1);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.border_width =
                                    (this.draft.appearance.border_width + 1).min(10);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Border Color",
                        "Perimeter outline color hex code.",
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .w(px(24.0))
                                    .h(px(24.0))
                                    .rounded_md()
                                    .bg(parsed_border_color)
                                    .border_1()
                                    .border_color(rgba(0xFFFFFF40)),
                            )
                            .child(render_text_input(
                                "input-border-hex",
                                ActiveInput::BorderColorHex,
                                &border_hex,
                                "#FFFFFF33",
                                self.active_input,
                                &self.input_buffer,
                                cx,
                            )),
                    ),
                ],
            ))
    }
}
