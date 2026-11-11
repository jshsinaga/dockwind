use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use chrono::Local;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, FocusHandle, FontWeight, IntoElement,
    KeyDownEvent, Render, RenderImage, ScrollHandle, SharedString, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, img, point,
    prelude::*, px, rgba, size,
};
use tracing::error;

use crate::config::{
    AlignmentAnchor, AnimationCurve, BlurMode, ClockAlignment, Config, DockEdge, IndicatorStyle,
    ItemAlignment, PinnedApp, VerticalAlign, WidthMode, parse_rgba_hex,
};
use crate::win;

const AVAILABLE_FONTS: &[(&str, &str)] = &[
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

const COLOR_PRESETS: &[(&str, &str)] = &[
    ("Catppuccin Mocha", "#1E1E2ECC"),
    ("Slate Dark", "#0F172ACC"),
    ("Midnight Blue", "#090D16EE"),
    ("Jet Black", "#000000D0"),
    ("Zinc Gray", "#18181BEE"),
];

const TIME_FORMAT_PRESETS: &[(&str, &str)] = &[
    ("24-Hour (14:30)", "%H:%M"),
    ("24-Hour + Seconds (14:30:45)", "%H:%M:%S"),
    ("12-Hour (02:30 PM)", "%I:%M %p"),
    ("12-Hour + Seconds (02:30:45 PM)", "%I:%M:%S %p"),
];

const DATE_FORMAT_PRESETS: &[(&str, &str)] = &[
    ("Short Day (Mon, 11 Nov 2026)", "%a, %d %b %Y"),
    ("Full Day (Monday, 11 November 2026)", "%A, %d %B %Y"),
    ("Numeric Date (11/11/2026)", "%d/%m/%Y"),
    ("ISO Format (2026-11-11)", "%Y-%m-%d"),
    ("Hide Date", ""),
];

const HOTKEY_PRESETS: &[&str] = &[
    "Ctrl+Alt+Shift+Q",
    "Ctrl+Shift+Q",
    "Win+Alt+Q",
    "Ctrl+Alt+Q",
];

#[derive(Debug, Clone, Copy)]
struct ThemePreset {
    name: &'static str,
    desc: &'static str,
    bg: &'static str,
    opacity: u8,
    blur: BlurMode,
    border_color: &'static str,
    border_width: u32,
    corner_radius: u32,
    shadow: bool,
    indicator: IndicatorStyle,
}

const THEME_PRESETS: &[ThemePreset] = &[
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Clock,
    Behavior,
    Pinned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveInput {
    BackgroundHex,
    BorderColorHex,
    ClockTimeFormat,
    ClockDateFormat,
    ClockFontColor,
}

#[derive(Clone, Copy)]
pub struct DraggedPinnedApp {
    pub index: usize,
}

impl Render for DraggedPinnedApp {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1p5()
            .rounded_md()
            .bg(rgba(0x313244FA))
            .border_1()
            .border_color(rgba(0x89B4FAFF))
            .shadow_md()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba(0xCDD6F4FF))
                    .child(format!("Moving item #{}", self.index + 1)),
            )
    }
}

pub struct SettingsView {
    config_path: PathBuf,
    draft: Config,
    saved: Config,
    autostart_draft: bool,
    autostart_saved: bool,
    active_tab: SettingsTab,
    selected_pinned_index: Option<usize>,
    delete_confirm_index: Option<usize>,
    status_message: Option<(SharedString, bool)>,
    scroll_handle: ScrollHandle,
    showing_taskbar_confirm: bool,
    active_input: Option<ActiveInput>,
    input_buffer: String,
    focus_handle: Option<FocusHandle>,
    font_dropdown_open: bool,
    icon_cache: RefCell<HashMap<PathBuf, Option<Arc<RenderImage>>>>,
}

impl SettingsView {
    pub fn new(config_path: PathBuf, config: Config, autostart: bool) -> Self {
        Self {
            config_path,
            draft: config.clone(),
            saved: config,
            autostart_draft: autostart,
            autostart_saved: autostart,
            active_tab: SettingsTab::Appearance,
            selected_pinned_index: None,
            delete_confirm_index: None,
            status_message: None,
            scroll_handle: ScrollHandle::default(),
            showing_taskbar_confirm: false,
            active_input: None,
            input_buffer: String::new(),
            focus_handle: None,
            font_dropdown_open: false,
            icon_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.draft != self.saved || self.autostart_draft != self.autostart_saved
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.delete_confirm_index.is_some() && event.keystroke.key == "escape" {
            self.delete_confirm_index = None;
            cx.notify();
            return;
        }

        let Some(field) = self.active_input else {
            return;
        };

        let key = &event.keystroke.key;
        if key == "escape" || key == "enter" {
            self.commit_input(field);
            self.active_input = None;
            cx.notify();
            return;
        }

        if key == "backspace" {
            self.input_buffer.pop();
            self.live_update_input(field);
            cx.notify();
            return;
        }

        let char_to_add = if key == "space" {
            Some(' ')
        } else if key.chars().count() == 1
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
        {
            key.chars().next()
        } else {
            None
        };

        if let Some(ch) = char_to_add {
            match field {
                ActiveInput::BackgroundHex
                | ActiveInput::BorderColorHex
                | ActiveInput::ClockFontColor => {
                    let is_valid_hex_char =
                        (ch == '#' && self.input_buffer.is_empty()) || ch.is_ascii_hexdigit();
                    if is_valid_hex_char && self.input_buffer.len() < 9 {
                        self.input_buffer.push(ch.to_ascii_uppercase());
                        self.live_update_input(field);
                        cx.notify();
                    }
                }
                ActiveInput::ClockTimeFormat | ActiveInput::ClockDateFormat => {
                    if self.input_buffer.len() < 64 {
                        self.input_buffer.push(ch);
                        self.live_update_input(field);
                        cx.notify();
                    }
                }
            }
        }
    }

    fn live_update_input(&mut self, field: ActiveInput) {
        match field {
            ActiveInput::BackgroundHex => {
                let trimmed = self.input_buffer.trim();
                let clean = trimmed.strip_prefix('#').unwrap_or(trimmed);
                if clean.len() == 6 && clean.bytes().all(|b| b.is_ascii_hexdigit()) {
                    self.draft.appearance.background = format!("#{clean}");
                }
            }
            ActiveInput::BorderColorHex => {
                let trimmed = self.input_buffer.trim();
                let clean = trimmed.strip_prefix('#').unwrap_or(trimmed);
                if (clean.len() == 6 || clean.len() == 8)
                    && clean.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    self.draft.appearance.border_color = format!("#{clean}");
                }
            }
            ActiveInput::ClockTimeFormat => {
                if !self.input_buffer.is_empty() {
                    self.draft.clock.time_format = self.input_buffer.clone();
                }
            }
            ActiveInput::ClockDateFormat => {
                self.draft.clock.date_format = self.input_buffer.clone();
            }
            ActiveInput::ClockFontColor => {
                let trimmed = self.input_buffer.trim();
                let clean = trimmed.strip_prefix('#').unwrap_or(trimmed);
                if (clean.len() == 6 || clean.len() == 8)
                    && clean.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    self.draft.clock.font_color = Some(format!("#{clean}"));
                } else if trimmed.is_empty() {
                    self.draft.clock.font_color = None;
                }
            }
        }
    }

    fn commit_input(&mut self, field: ActiveInput) {
        self.live_update_input(field);
        match field {
            ActiveInput::BackgroundHex => {
                let trimmed = self.input_buffer.trim();
                let clean = trimmed.strip_prefix('#').unwrap_or(trimmed);
                if clean.len() == 6 && clean.bytes().all(|b| b.is_ascii_hexdigit()) {
                    self.draft.appearance.background = format!("#{clean}");
                }
            }
            ActiveInput::ClockFontColor if self.input_buffer.trim().is_empty() => {
                self.draft.clock.font_color = None;
            }
            _ => {}
        }
    }

    fn apply_and_save(&mut self, cx: &mut Context<Self>) {
        match self.draft.save_atomic_preserving(&self.config_path) {
            Ok(loaded) => {
                self.saved = loaded.config;
                self.draft = self.saved.clone();

                if self.autostart_draft != self.autostart_saved {
                    let autostart_result = if self.autostart_draft {
                        win::autostart::enable()
                    } else {
                        win::autostart::disable()
                    };
                    match autostart_result {
                        Ok(()) => {
                            self.autostart_saved = self.autostart_draft;
                        }
                        Err(error) => {
                            self.status_message = Some((
                                SharedString::from(format!(
                                    "Settings saved, but autostart update failed: {error:#}"
                                )),
                                true,
                            ));
                            cx.notify();
                            return;
                        }
                    }
                }

                self.status_message = Some((
                    SharedString::from("Settings saved and applied successfully!"),
                    false,
                ));
            }
            Err(error) => {
                self.status_message = Some((
                    SharedString::from(format!("Failed to save settings: {error}")),
                    true,
                ));
            }
        }
        cx.notify();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = [
            ("Appearance", SettingsTab::Appearance),
            ("Clock & Date", SettingsTab::Clock),
            ("Behavior", SettingsTab::Behavior),
            ("Pinned Apps", SettingsTab::Pinned),
        ];

        div()
            .px_6()
            .py_4()
            .border_b_1()
            .border_color(rgba(0x313244FF))
            .bg(rgba(0x181825FF))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgba(0xFFFFFFFF))
                            .child("Dockwind Settings"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgba(0x6C7086FF))
                            .child(self.config_path.display().to_string()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .p_1()
                    .rounded_lg()
                    .bg(rgba(0x11111BFF))
                    .children(tabs.into_iter().map(|(label, tab)| {
                        let is_active = self.active_tab == tab;
                        div()
                            .id(SharedString::from(format!("tab-{label}")))
                            .px_3()
                            .py_1p5()
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
                            .hover(|style| {
                                if !is_active {
                                    style.bg(rgba(0x1E1E2EFF)).text_color(rgba(0xCDD6F4FF))
                                } else {
                                    style
                                }
                            })
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.active_tab = tab;
                                this.delete_confirm_index = None;
                                this.showing_taskbar_confirm = false;
                                cx.notify();
                            }))
                            .child(label)
                    })),
            )
    }

    fn render_appearance_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
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

    fn render_behavior_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_autohide = self.draft.behavior.auto_hide;
        let mut autohide_rows = vec![render_setting_row(
            "Auto Hide",
            "Automatically hide the dock and reveal when pointer approaches.",
            render_toggle(
                "toggle-autohide",
                is_autohide,
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.draft.behavior.auto_hide = !this.draft.behavior.auto_hide;
                    this.status_message = None;
                    cx.notify();
                }),
            ),
        )];

        if is_autohide {
            autohide_rows.push(render_setting_row(
                "Show Delay",
                "Delay before the dock begins to appear (0 - 1000 ms).",
                render_stepper(
                    "show-delay",
                    format!("{} ms", self.draft.behavior.show_delay_ms),
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.behavior.show_delay_ms =
                            this.draft.behavior.show_delay_ms.saturating_sub(20);
                        this.status_message = None;
                        cx.notify();
                    }),
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.behavior.show_delay_ms =
                            (this.draft.behavior.show_delay_ms + 20).min(1000);
                        this.status_message = None;
                        cx.notify();
                    }),
                ),
            ));
            autohide_rows.push(render_setting_row(
                "Hide Delay",
                "Delay before the dock begins to hide (50 - 2000 ms).",
                render_stepper(
                    "hide-delay",
                    format!("{} ms", self.draft.behavior.hide_delay_ms),
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.behavior.hide_delay_ms =
                            this.draft.behavior.hide_delay_ms.saturating_sub(50).max(50);
                        this.status_message = None;
                        cx.notify();
                    }),
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.behavior.hide_delay_ms =
                            (this.draft.behavior.hide_delay_ms + 50).min(2000);
                        this.status_message = None;
                        cx.notify();
                    }),
                ),
            ));
            autohide_rows.push(render_setting_row(
                "Animation Duration",
                "Slide animation transition time (60 - 1000 ms).",
                render_stepper(
                    "animation-duration",
                    format!("{} ms", self.draft.behavior.animation_ms),
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.behavior.animation_ms =
                            this.draft.behavior.animation_ms.saturating_sub(20).max(60);
                        this.status_message = None;
                        cx.notify();
                    }),
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.behavior.animation_ms =
                            (this.draft.behavior.animation_ms + 20).min(1000);
                        this.status_message = None;
                        cx.notify();
                    }),
                ),
            ));
            let current_curve = self.draft.behavior.animation_curve;
            autohide_rows.push(render_setting_row(
                "Animation Curve",
                "Timing curve easing function for reveal and hide transitions.",
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .p_1()
                    .rounded_lg()
                    .bg(rgba(0x181825FF))
                    .child(render_choice_pill(
                        "pill-curve-ease-out",
                        "Ease Out",
                        current_curve == AnimationCurve::EaseOut,
                        cx.listener(|this: &mut Self, _, _, cx| {
                            this.draft.behavior.animation_curve = AnimationCurve::EaseOut;
                            this.status_message = None;
                            cx.notify();
                        }),
                    ))
                    .child(render_choice_pill(
                        "pill-curve-ease-in",
                        "Ease In",
                        current_curve == AnimationCurve::EaseIn,
                        cx.listener(|this: &mut Self, _, _, cx| {
                            this.draft.behavior.animation_curve = AnimationCurve::EaseIn;
                            this.status_message = None;
                            cx.notify();
                        }),
                    ))
                    .child(render_choice_pill(
                        "pill-curve-ease-in-out",
                        "Ease In-Out",
                        current_curve == AnimationCurve::EaseInOut,
                        cx.listener(|this: &mut Self, _, _, cx| {
                            this.draft.behavior.animation_curve = AnimationCurve::EaseInOut;
                            this.status_message = None;
                            cx.notify();
                        }),
                    ))
                    .child(render_choice_pill(
                        "pill-curve-linear",
                        "Linear",
                        current_curve == AnimationCurve::Linear,
                        cx.listener(|this: &mut Self, _, _, cx| {
                            this.draft.behavior.animation_curve = AnimationCurve::Linear;
                            this.status_message = None;
                            cx.notify();
                        }),
                    ))
                    .child(render_choice_pill(
                        "pill-curve-ease",
                        "Ease",
                        current_curve == AnimationCurve::Ease,
                        cx.listener(|this: &mut Self, _, _, cx| {
                            this.draft.behavior.animation_curve = AnimationCurve::Ease;
                            this.status_message = None;
                            cx.notify();
                        }),
                    ))
                    .child(render_choice_pill(
                        "pill-curve-bounce",
                        "Bounce",
                        current_curve == AnimationCurve::Bounce,
                        cx.listener(|this: &mut Self, _, _, cx| {
                            this.draft.behavior.animation_curve = AnimationCurve::Bounce;
                            this.status_message = None;
                            cx.notify();
                        }),
                    )),
            ));
            autohide_rows.push(render_setting_row(
                "Trigger Area Height",
                "Thickness of the screen edge hover zone that reveals the dock (1 - 64 px).",
                render_stepper(
                    "trigger-size",
                    format!("{} px", self.draft.behavior.trigger_size),
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.behavior.trigger_size =
                            this.draft.behavior.trigger_size.saturating_sub(1).max(1);
                        this.status_message = None;
                        cx.notify();
                    }),
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.draft.behavior.trigger_size =
                            (this.draft.behavior.trigger_size + 1).min(64);
                        this.status_message = None;
                        cx.notify();
                    }),
                ),
            ));
        }

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(render_card(
                "Auto Hide",
                "Slide animations and mouse hover trigger behavior.",
                autohide_rows,
            ))
            .child(render_card(
                "Display & System Integration",
                "Dock interactions with other applications and Windows environment.",
                vec![
                    render_setting_row(
                        "Hide in Fullscreen",
                        "Hide the dock when an active application is in fullscreen mode.",
                        render_toggle(
                            "toggle-fullscreen",
                            self.draft.behavior.hide_in_fullscreen,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.behavior.hide_in_fullscreen =
                                    !this.draft.behavior.hide_in_fullscreen;
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Show Running Applications",
                        "Display running application icons on the dock.",
                        render_toggle(
                            "toggle-running",
                            self.draft.behavior.show_running_apps,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.behavior.show_running_apps =
                                    !this.draft.behavior.show_running_apps;
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Hide Native Taskbar",
                        "Hide the default Windows taskbar while Dockwind runs.",
                        render_toggle(
                            "toggle-hide-taskbar",
                            self.draft.behavior.hide_native_taskbar,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                if !this.draft.behavior.hide_native_taskbar {
                                    this.showing_taskbar_confirm = true;
                                } else {
                                    this.draft.behavior.hide_native_taskbar = false;
                                }
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    if self.draft.behavior.hide_native_taskbar {
                        div()
                            .p_3()
                            .rounded_lg()
                            .bg(rgba(0x18182580))
                            .border_1()
                            .border_color(rgba(0xF9E2AF44))
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(0xF9E2AFFF))
                                    .child("Method & Advisory"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgba(0xCDD6F4FF))
                                    .child(
                                        "Method: Dockwind moves the Windows taskbar off-screen and suppresses hover popups while running.",
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgba(0x89B4FAFF))
                                    .child(
                                        "Recommended: For the cleanest native experience, you can also enable 'Automatically hide the taskbar' in Windows Settings (Personalization > Taskbar).",
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgba(0xA6ADC8FF))
                                    .child(
                                        "Warning: If Dockwind is terminated abnormally, run 'dockwind --restore-taskbar' or re-launch Dockwind to restore the taskbar position.",
                                    ),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .text_xs()
                            .text_color(rgba(0x6C7086FF))
                            .child(
                                "Tip: Enabling 'Automatically hide the taskbar' in Windows Settings is recommended alongside Dockwind.",
                            )
                            .into_any_element()
                    },
                    render_setting_row(
                        "Reserve Desktop Space (AppBar)",
                        "Prevent maximized application windows from covering the dock.",
                        render_toggle(
                            "toggle-reserve-space",
                            self.draft.behavior.reserve_space,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.behavior.reserve_space =
                                    !this.draft.behavior.reserve_space;
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Reserve Space Offset",
                        "Gap between the dock and the reserved desktop work area (0 - 120 px).",
                        render_stepper(
                            "reserve-offset",
                            format!("{} px", self.draft.behavior.reserve_offset),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.behavior.reserve_offset =
                                    this.draft.behavior.reserve_offset.saturating_sub(2);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.behavior.reserve_offset =
                                    (this.draft.behavior.reserve_offset + 2).min(120);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
                        "Start with Windows",
                        "Launch Dockwind automatically when signing in to Windows.",
                        render_toggle(
                            "toggle-autostart",
                            self.autostart_draft,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.autostart_draft = !this.autostart_draft;
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                ],
            ))
            .child(render_card(
                "Quit Hotkey",
                "Global keyboard shortcut to cleanly exit Dockwind.",
                vec![
                    div()
                        .flex()
                        .gap_2()
                        .children(HOTKEY_PRESETS.iter().map(|preset| {
                            let is_active = self.draft.behavior.quit_hotkey == *preset;
                            let hotkey_str = (*preset).to_string();
                            div()
                                .id(SharedString::from(format!("hotkey-{preset}")))
                                .px_3()
                                .py_1p5()
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
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if is_active {
                                    rgba(0xFFFFFFFF)
                                } else {
                                    rgba(0xA6ADC8FF)
                                })
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.draft.behavior.quit_hotkey = hotkey_str.clone();
                                    this.status_message = None;
                                    cx.notify();
                                }))
                                .child(*preset)
                        }))
                        .into_any_element(),
                ],
            ))
    }

    fn render_clock_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
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

    fn render_pinned_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let total_count = self.draft.pinned.len();

        let app_items: Vec<_> = self
            .draft
            .pinned
            .iter()
            .enumerate()
            .map(|(index, app)| {
                let is_selected = self.selected_pinned_index == Some(index);
                let app_path = PathBuf::from(&app.path);
                let icon = {
                    let mut cache = self.icon_cache.borrow_mut();
                    cache
                        .entry(app_path.clone())
                        .or_insert_with(|| crate::win::icons::icon_from_path(&app_path))
                        .clone()
                };

                let icon_element = match icon {
                    Some(img_data) => img(img_data).size(px(28.0)).rounded_md().into_any_element(),
                    None => div()
                        .size(px(28.0))
                        .rounded_md()
                        .bg(rgba(0x313244FF))
                        .border_1()
                        .border_color(rgba(0x45475A80))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(0xCDD6F4FF))
                        .child(
                            app.name
                                .chars()
                                .next()
                                .map(|c| c.to_uppercase().to_string())
                                .unwrap_or_else(|| "?".to_string()),
                        )
                        .into_any_element(),
                };

                div()
                    .id(SharedString::from(format!("pinned-app-{index}")))
                    .px_3()
                    .py_2()
                    .rounded_lg()
                    .bg(if is_selected {
                        rgba(0x313244FF)
                    } else {
                        rgba(0x18182580)
                    })
                    .border_1()
                    .border_color(if is_selected {
                        rgba(0x89B4FA80)
                    } else {
                        rgba(0x31324433)
                    })
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.selected_pinned_index = Some(index);
                        cx.notify();
                    }))
                    .on_drop(cx.listener(
                        move |this: &mut Self, info: &DraggedPinnedApp, _window, cx| {
                            let from = info.index;
                            let to = index;
                            if from < this.draft.pinned.len()
                                && to < this.draft.pinned.len()
                                && from != to
                            {
                                let item = this.draft.pinned.remove(from);
                                this.draft.pinned.insert(to, item);
                                this.selected_pinned_index = Some(to);
                                this.status_message = None;
                                cx.notify();
                            }
                        },
                    ))
                    .drag_over::<DraggedPinnedApp>(|style, _, _, _| {
                        style.bg(rgba(0x45475A80)).border_color(rgba(0x89B4FAFF))
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .flex_1()
                            .child(
                                div()
                                    .id(SharedString::from(format!("drag-handle-{index}")))
                                    .px_1()
                                    .py_0p5()
                                    .rounded_sm()
                                    .hover(|s| s.bg(rgba(0x313244FF)))
                                    .cursor_move()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(0x6C7086FF))
                                    .child("::")
                                    .on_drag(
                                        DraggedPinnedApp { index },
                                        |info: &DraggedPinnedApp, _, _, cx| cx.new(|_| *info),
                                    ),
                            )
                            .child(
                                div()
                                    .w(px(20.0))
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(0x6C7086FF))
                                    .child(format!("{}.", index + 1)),
                            )
                            .child(icon_element)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgba(0xCDD6F4FF))
                                            .child(app.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgba(0x6C7086FF))
                                            .child(app.path.clone()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                div()
                                    .id(SharedString::from(format!("btn-up-{index}")))
                                    .size(px(26.0))
                                    .rounded_md()
                                    .bg(rgba(0x31324480))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .when(index > 0, |s| {
                                        s.cursor_pointer()
                                            .hover(|h| h.bg(rgba(0x45475AFF)))
                                            .text_color(rgba(0xCDD6F4FF))
                                    })
                                    .when(index == 0, |s| s.text_color(rgba(0x6C708640)))
                                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                        if index > 0 && index < this.draft.pinned.len() {
                                            this.draft.pinned.swap(index, index - 1);
                                            this.selected_pinned_index = Some(index - 1);
                                            this.status_message = None;
                                            cx.notify();
                                        }
                                    }))
                                    .child("^"),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("btn-down-{index}")))
                                    .size(px(26.0))
                                    .rounded_md()
                                    .bg(rgba(0x31324480))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .when(index + 1 < total_count, |s| {
                                        s.cursor_pointer()
                                            .hover(|h| h.bg(rgba(0x45475AFF)))
                                            .text_color(rgba(0xCDD6F4FF))
                                    })
                                    .when(index + 1 >= total_count, |s| {
                                        s.text_color(rgba(0x6C708640))
                                    })
                                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                        if index + 1 < this.draft.pinned.len() {
                                            this.draft.pinned.swap(index, index + 1);
                                            this.selected_pinned_index = Some(index + 1);
                                            this.status_message = None;
                                            cx.notify();
                                        }
                                    }))
                                    .child("v"),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("btn-delete-{index}")))
                                    .size(px(26.0))
                                    .rounded_md()
                                    .bg(rgba(0x31324480))
                                    .hover(|s| s.bg(rgba(0xF38BA833)).text_color(rgba(0xF38BA8FF)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(0xF38BA8AA))
                                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                        this.delete_confirm_index = Some(index);
                                        cx.notify();
                                    }))
                                    .child("X"),
                            ),
                    )
                    .into_any_element()
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgba(0xCDD6F4FF))
                                    .child("Pinned Applications"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgba(0x6C7086FF))
                                    .child("Applications that stay on the dock for quick access. Drag handle (::) to reorder."),
                            ),
                    )
                    .child(
                        div()
                            .id("btn-add-pinned")
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .bg(rgba(0x89B4FAFF))
                            .hover(|s| s.bg(rgba(0xB4BEFEFF)))
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgba(0x11111BFF))
                            .cursor_pointer()
                            .on_click(cx.listener(|_this: &mut Self, _, _, cx| {
                                let receiver = win::dialogs::pick_file_async();
                                cx.spawn(async move |this, cx| {
                                    if let Ok(Ok(Some(path))) = receiver.await {
                                        let stem = path
                                            .file_stem()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("App");
                                        let mut chars = stem.chars();
                                        let name = match chars.next() {
                                            None => "App".to_string(),
                                            Some(first) => {
                                                first.to_uppercase().collect::<String>()
                                                    + chars.as_str()
                                            }
                                        };
                                        let _ = this.update(cx, |this, cx| {
                                            this.draft.pinned.push(PinnedApp {
                                                name,
                                                path: path.to_string_lossy().to_string(),
                                                args: Vec::new(),
                                            });
                                            this.selected_pinned_index =
                                                Some(this.draft.pinned.len() - 1);
                                            this.status_message = None;
                                            cx.notify();
                                        });
                                    }
                                })
                                .detach();
                            }))
                            .child("+ Add Application..."),
                    ),
            )
            .child(
                div()
                    .p_3()
                    .rounded_xl()
                    .bg(rgba(0x1E1E2EFF))
                    .border_1()
                    .border_color(rgba(0x313244FF))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(if app_items.is_empty() {
                        vec![
                            div()
                                .py_8()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_xs()
                                .text_color(rgba(0x6C7086FF))
                                .child("No pinned applications yet. Click '+ Add Application...' to add one.")
                                .into_any_element(),
                        ]
                    } else {
                        app_items
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgba(0x6C7086FF))
                    .child("Tip: Drag the :: handle to reorder apps, or use ^ / v buttons. Click X to remove an application."),
            )
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (status_text, status_color) = if let Some((msg, is_error)) = &self.status_message {
            (
                msg.clone(),
                if *is_error {
                    rgba(0xF38BA8FF)
                } else {
                    rgba(0xA6E3A1FF)
                },
            )
        } else if self.has_unsaved_changes() {
            (SharedString::from("● Unsaved changes"), rgba(0xF9E2AFFF))
        } else {
            (SharedString::from("All changes saved"), rgba(0x6C7086FF))
        };

        div()
            .px_6()
            .py_4()
            .border_t_1()
            .border_color(rgba(0x313244FF))
            .bg(rgba(0x181825FF))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(status_color)
                    .child(status_text),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .id("btn-close-settings")
                            .px_4()
                            .py_2()
                            .rounded_lg()
                            .bg(rgba(0x313244FF))
                            .hover(|s| s.bg(rgba(0x45475AFF)))
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgba(0xCDD6F4FF))
                            .cursor_pointer()
                            .on_click(cx.listener(|_, _, window, _| {
                                window.remove_window();
                            }))
                            .child("Close"),
                    )
                    .child(
                        div()
                            .id("btn-apply-settings")
                            .px_5()
                            .py_2()
                            .rounded_lg()
                            .bg(rgba(0x89B4FAFF))
                            .hover(|s| s.bg(rgba(0xB4BEFEFF)))
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgba(0x11111BFF))
                            .cursor_pointer()
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.apply_and_save(cx);
                            }))
                            .child("Apply & Save"),
                    ),
            )
    }

    fn render_taskbar_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("taskbar-confirm-backdrop")
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .bg(rgba(0x000000B0))
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .id("taskbar-confirm-modal")
                    .w(px(520.0))
                    .p_6()
                    .rounded_2xl()
                    .bg(rgba(0x181825FF))
                    .border_1()
                    .border_color(rgba(0xF9E2AF80))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgba(0xF9E2AF33))
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(0xF9E2AFFF))
                                    .child("WARNING"),
                            )
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(0xFFFFFFFF))
                                    .child("Hide Windows Taskbar?"),
                            ),
                    )
                    .child(
                        div()
                            .p_3p5()
                            .rounded_lg()
                            .bg(rgba(0x11111B80))
                            .border_1()
                            .border_color(rgba(0x31324480))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgba(0xCDD6F4FF))
                                    .child(
                                        "This feature repositions the Windows taskbar off-screen and suppresses mouse hover detection while Dockwind is running.",
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgba(0x89B4FAFF))
                                    .child(
                                        "Recommended: Enabling 'Automatically hide the taskbar' in Windows Settings is safer and provides full native system stability.",
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgba(0xA6ADC8FF))
                                    .child(
                                        "Recovery: If Dockwind closes unexpectedly, you can restore your taskbar by running 'dockwind --restore-taskbar' or re-opening Dockwind.",
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgba(0xCDD6F4FF))
                            .child("Are you sure you want to enable this feature?"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_3()
                            .pt_2()
                            .child(
                                div()
                                    .id("btn-cancel-taskbar-hide")
                                    .px_4()
                                    .py_2()
                                    .rounded_lg()
                                    .bg(rgba(0x313244FF))
                                    .hover(|s| s.bg(rgba(0x45475AFF)))
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgba(0xCDD6F4FF))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.showing_taskbar_confirm = false;
                                        cx.notify();
                                    }))
                                    .child("Cancel"),
                            )
                            .child(
                                div()
                                    .id("btn-confirm-taskbar-hide")
                                    .px_4()
                                    .py_2()
                                    .rounded_lg()
                                    .bg(rgba(0xF9E2AFFF))
                                    .hover(|s| s.bg(rgba(0xF5E0DCFF)))
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgba(0x11111BFF))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.draft.behavior.hide_native_taskbar = true;
                                        this.showing_taskbar_confirm = false;
                                        this.status_message = None;
                                        cx.notify();
                                    }))
                                    .child("Enable Hide Taskbar"),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_delete_confirm_dialog(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let app_name = self
            .draft
            .pinned
            .get(index)
            .map(|app| app.name.clone())
            .unwrap_or_else(|| "this application".to_string());

        div()
            .id("delete-confirm-backdrop")
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .bg(rgba(0x000000B0))
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .child(
                div()
                    .id("delete-confirm-modal")
                    .w(px(460.0))
                    .p_6()
                    .rounded_2xl()
                    .bg(rgba(0x181825FF))
                    .border_1()
                    .border_color(rgba(0xF38BA880))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgba(0xF38BA833))
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(0xF38BA8FF))
                                    .child("REMOVE"),
                            )
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(0xFFFFFFFF))
                                    .child("Remove Pinned Application?"),
                            ),
                    )
                    .child(div().text_sm().text_color(rgba(0xCDD6F4FF)).child(format!(
                        "Are you sure you want to remove \"{}\" from pinned applications?",
                        app_name
                    )))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_3()
                            .pt_2()
                            .child(
                                div()
                                    .id("btn-cancel-delete")
                                    .px_4()
                                    .py_2()
                                    .rounded_lg()
                                    .bg(rgba(0x313244FF))
                                    .hover(|s| s.bg(rgba(0x45475AFF)))
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgba(0xCDD6F4FF))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.delete_confirm_index = None;
                                        cx.notify();
                                    }))
                                    .child("Cancel"),
                            )
                            .child(
                                div()
                                    .id("btn-confirm-delete")
                                    .px_4()
                                    .py_2()
                                    .rounded_lg()
                                    .bg(rgba(0xF38BA8FF))
                                    .hover(|s| s.bg(rgba(0xEBA0ACFF)))
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgba(0x11111BFF))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                        if let Some(target_idx) = this.delete_confirm_index
                                            && target_idx < this.draft.pinned.len()
                                        {
                                            this.draft.pinned.remove(target_idx);
                                            if this.draft.pinned.is_empty() {
                                                this.selected_pinned_index = None;
                                            } else if let Some(selected) =
                                                this.selected_pinned_index
                                                && selected >= this.draft.pinned.len()
                                            {
                                                this.selected_pinned_index =
                                                    Some(this.draft.pinned.len() - 1);
                                            }
                                        }
                                        this.delete_confirm_index = None;
                                        this.status_message = None;
                                        cx.notify();
                                    }))
                                    .child("Remove"),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self
            .focus_handle
            .get_or_insert_with(|| cx.focus_handle())
            .clone();

        div()
            .relative()
            .size_full()
            .bg(rgba(0x11111BFF))
            .flex()
            .flex_col()
            .track_focus(&focus_handle)
            .on_key_down(
                cx.listener(|this: &mut Self, event: &KeyDownEvent, _window, cx| {
                    this.handle_key_down(event, cx);
                }),
            )
            .child(self.render_header(cx))
            .child(
                div()
                    .id("settings-body-scroll")
                    .track_scroll(&self.scroll_handle)
                    .overflow_y_scroll()
                    .flex_1()
                    .p_6()
                    .child(match self.active_tab {
                        SettingsTab::Appearance => {
                            self.render_appearance_tab(cx).into_any_element()
                        }
                        SettingsTab::Clock => self.render_clock_tab(cx).into_any_element(),
                        SettingsTab::Behavior => self.render_behavior_tab(cx).into_any_element(),
                        SettingsTab::Pinned => self.render_pinned_tab(cx).into_any_element(),
                    }),
            )
            .child(self.render_footer(cx))
            .children(if self.showing_taskbar_confirm {
                Some(self.render_taskbar_confirm_dialog(cx))
            } else {
                self.delete_confirm_index
                    .map(|idx| self.render_delete_confirm_dialog(idx, cx))
            })
    }
}

fn render_card(
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

fn render_setting_row(
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

fn render_text_input(
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

fn render_stepper(
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

fn render_toggle(
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

fn render_choice_pill(
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

pub fn open_settings_window(cx: &mut App, config_path: PathBuf) -> Result<()> {
    if win::dialogs::focus_existing_settings_window() {
        return Ok(());
    }

    let loaded = Config::load_or_create(&config_path)
        .context("failed to load configuration for settings window")?;

    let monitor = win::monitors::MonitorInfo::primary()
        .context("failed to query primary monitor for settings window")?;

    let window_width = px(780.0);
    let window_height = px(720.0);
    let origin_x = monitor.work_area.origin.x + (monitor.work_area.size.width - window_width) / 2.0;
    let origin_y =
        monitor.work_area.origin.y + (monitor.work_area.size.height - window_height) / 2.0;
    let bounds = Bounds::new(point(origin_x, origin_y), size(window_width, window_height));

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from("Dockwind Settings")),
            appears_transparent: false,
            ..Default::default()
        }),
        focus: true,
        show: true,
        kind: WindowKind::Normal,
        is_movable: true,
        is_resizable: true,
        is_minimizable: true,
        window_background: WindowBackgroundAppearance::Opaque,
        ..Default::default()
    };

    let autostart_enabled = win::autostart::is_enabled().unwrap_or(false);

    cx.open_window(options, |_window, cx| {
        cx.new(|_cx| SettingsView::new(config_path, loaded.config, autostart_enabled))
    })
    .context("failed to open GPUI settings window")?;

    Ok(())
}

pub fn open_settings_standalone(config_path: PathBuf) -> Result<()> {
    let startup_error: Arc<Mutex<Option<anyhow::Error>>> = Arc::new(Mutex::new(None));
    let error_slot = Arc::clone(&startup_error);

    gpui::Application::new().run(move |cx| {
        if let Err(error) = open_settings_window(cx, config_path) {
            error!(%error, "failed to open settings window");
            *error_slot
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error);
            cx.quit();
        }
    });

    startup_error
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
        .map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_presets_are_valid() {
        for preset in HOTKEY_PRESETS {
            assert!(
                win::hotkey::validate(preset).is_ok(),
                "preset {preset} must be valid"
            );
        }
    }

    #[test]
    fn settings_view_detects_changes() {
        let path = PathBuf::from("config.toml");
        let config = Config::default();
        let mut view = SettingsView::new(path, config, false);
        assert!(!view.has_unsaved_changes());

        view.draft.appearance.width += 20;
        assert!(view.has_unsaved_changes());

        view.draft.appearance.width -= 20;
        assert!(!view.has_unsaved_changes());

        view.autostart_draft = true;
        assert!(view.has_unsaved_changes());
    }

    #[test]
    fn settings_view_tab_switching() {
        let path = PathBuf::from("config.toml");
        let config = Config::default();
        let mut view = SettingsView::new(path, config, false);
        assert_eq!(view.active_tab, SettingsTab::Appearance);

        view.active_tab = SettingsTab::Clock;
        assert_eq!(view.active_tab, SettingsTab::Clock);
    }

    #[test]
    fn settings_view_delete_confirm_state() {
        let path = PathBuf::from("config.toml");
        let config = Config::default();
        let mut view = SettingsView::new(path, config, false);
        assert!(view.delete_confirm_index.is_none());

        view.delete_confirm_index = Some(2);
        assert_eq!(view.delete_confirm_index, Some(2));

        view.delete_confirm_index = None;
        assert!(view.delete_confirm_index.is_none());
    }

    #[test]
    fn settings_view_pinned_reorder_and_delete() {
        let path = PathBuf::from("config.toml");
        let config = Config {
            pinned: vec![
                PinnedApp {
                    name: "App A".to_string(),
                    path: "C:\\a.exe".to_string(),
                    args: Vec::new(),
                },
                PinnedApp {
                    name: "App B".to_string(),
                    path: "C:\\b.exe".to_string(),
                    args: Vec::new(),
                },
            ],
            ..Default::default()
        };
        let mut view = SettingsView::new(path, config, false);
        assert_eq!(view.draft.pinned.len(), 2);

        view.draft.pinned.swap(0, 1);
        assert_eq!(view.draft.pinned[0].name, "App B");
        assert_eq!(view.draft.pinned[1].name, "App A");

        view.draft.pinned.remove(0);
        assert_eq!(view.draft.pinned.len(), 1);
        assert_eq!(view.draft.pinned[0].name, "App A");
    }

    #[test]
    fn settings_view_taskbar_confirm_state() {
        let path = PathBuf::from("config.toml");
        let config = Config::default();
        let mut view = SettingsView::new(path, config, false);
        assert!(!view.showing_taskbar_confirm);
        assert!(!view.draft.behavior.hide_native_taskbar);

        view.showing_taskbar_confirm = true;
        assert!(view.showing_taskbar_confirm);

        view.draft.behavior.hide_native_taskbar = true;
        view.showing_taskbar_confirm = false;
        assert!(!view.showing_taskbar_confirm);
        assert!(view.draft.behavior.hide_native_taskbar);
    }

    #[test]
    fn settings_view_active_input_state() {
        let path = PathBuf::from("config.toml");
        let config = Config::default();
        let mut view = SettingsView::new(path, config, false);
        assert!(view.active_input.is_none());
        assert!(view.input_buffer.is_empty());

        view.active_input = Some(ActiveInput::BackgroundHex);
        view.input_buffer = "#FFFFFF".to_string();
        assert_eq!(view.active_input, Some(ActiveInput::BackgroundHex));
        assert_eq!(view.input_buffer, "#FFFFFF");
    }
}
