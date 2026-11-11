use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use chrono::Local;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, FontWeight, IntoElement, Render, ScrollHandle,
    SharedString, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions, div, point, prelude::*, px, rgba, size,
};
use tracing::error;

use crate::config::{BlurMode, Config, DockEdge, PinnedApp, parse_rgba_hex};
use crate::win;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Behavior,
    Clock,
    Pinned,
}

pub struct SettingsView {
    config_path: PathBuf,
    draft: Config,
    saved: Config,
    autostart_draft: bool,
    autostart_saved: bool,
    active_tab: SettingsTab,
    selected_pinned_index: Option<usize>,
    status_message: Option<(SharedString, bool)>,
    scroll_handle: ScrollHandle,
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
            status_message: None,
            scroll_handle: ScrollHandle::default(),
        }
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.draft != self.saved || self.autostart_draft != self.autostart_saved
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
            ("Behavior", SettingsTab::Behavior),
            ("Clock & Date", SettingsTab::Clock),
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
                                cx.notify();
                            }))
                            .child(label)
                    })),
            )
    }

    fn render_appearance_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current_orientation = self.draft.appearance.orientation;
        let current_blur = self.draft.appearance.blur;
        let background_hex = self.draft.appearance.background.clone();
        let parsed_color = parse_rgba_hex(&background_hex)
            .map(|[r, g, b, a]| rgba(u32::from_be_bytes([r, g, b, a])))
            .unwrap_or_else(|| rgba(0x1E1E2ECC));

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(render_card(
                "Dock Dimensions",
                "Configure width, height, and overall physical layout of the dock.",
                vec![
                    render_setting_row(
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
                                this.draft.appearance.width =
                                    (this.draft.appearance.width + 20).min(3840);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
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
                                this.draft.appearance.height =
                                    (this.draft.appearance.height + 4).min(200);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
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
                        "Inner Padding",
                        "Padding between dock edges and icon elements (0 - 48 px).",
                        render_stepper(
                            "dock-padding",
                            format!("{} px", self.draft.appearance.padding),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.padding =
                                    this.draft.appearance.padding.saturating_sub(2);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.padding =
                                    (this.draft.appearance.padding + 2).min(48);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                    render_setting_row(
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
                    ),
                    render_setting_row(
                        "Screen Offset",
                        "Distance between the dock and monitor edge (0 - 120 px).",
                        render_stepper(
                            "dock-offset",
                            format!("{} px", self.draft.appearance.offset),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.offset =
                                    this.draft.appearance.offset.saturating_sub(2);
                                this.status_message = None;
                                cx.notify();
                            }),
                            cx.listener(|this: &mut Self, _, _, cx| {
                                this.draft.appearance.offset =
                                    (this.draft.appearance.offset + 2).min(120);
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
                ],
            ))
            .child(render_card(
                "Position & Orientation",
                "Select dock placement on your primary display.",
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
                "Backdrop & Blur",
                "Windows DWM material backdrop styling behind the dock.",
                vec![render_setting_row(
                    "Blur Mode",
                    "Windows transparency backdrop effect (None, Mica, Acrylic).",
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
                )],
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
                        "Choose from color presets or open the native color dialog.",
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
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgba(0xCDD6F4FF))
                                    .child(background_hex),
                            )
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
                                        if let Ok(Some(color)) = win::dialogs::pick_color_dialog(
                                            &this.draft.appearance.background,
                                        ) {
                                            this.draft.appearance.background = color;
                                            this.status_message = None;
                                            cx.notify();
                                        }
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
                                this.draft.behavior.hide_native_taskbar =
                                    !this.draft.behavior.hide_native_taskbar;
                                this.status_message = None;
                                cx.notify();
                            }),
                        ),
                    ),
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
                            .child("Clock Widget Preview"),
                    )
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgba(0xFFFFFFFF))
                            .child(time_display),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgba(0xA6ADC8FF))
                            .child(date_display),
                    ),
            )
            .child(render_card(
                "Time Format",
                "Choose the time presentation style for the dock.",
                TIME_FORMAT_PRESETS
                    .iter()
                    .map(|(label, format)| {
                        let is_active = self.draft.clock.time_format == *format;
                        let fmt = (*format).to_string();
                        div()
                            .id(SharedString::from(format!("time-fmt-{format}")))
                            .px_3()
                            .py_2()
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
                            .flex()
                            .items_center()
                            .justify_between()
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.draft.clock.time_format = fmt.clone();
                                this.status_message = None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if is_active {
                                        rgba(0xFFFFFFFF)
                                    } else {
                                        rgba(0xCDD6F4FF)
                                    })
                                    .child(*label),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgba(0x6C7086FF))
                                    .child(format!("Pattern: {format}")),
                            )
                            .into_any_element()
                    })
                    .collect(),
            ))
            .child(render_card(
                "Date Format",
                "Choose the date presentation style for the dock.",
                DATE_FORMAT_PRESETS
                    .iter()
                    .map(|(label, format)| {
                        let is_active = self.draft.clock.date_format == *format;
                        let fmt = (*format).to_string();
                        div()
                            .id(SharedString::from(format!("date-fmt-{label}")))
                            .px_3()
                            .py_2()
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
                            .flex()
                            .items_center()
                            .justify_between()
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.draft.clock.date_format = fmt.clone();
                                this.status_message = None;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(if is_active {
                                        rgba(0xFFFFFFFF)
                                    } else {
                                        rgba(0xCDD6F4FF)
                                    })
                                    .child(*label),
                            )
                            .child(div().text_xs().text_color(rgba(0x6C7086FF)).child(
                                if format.is_empty() {
                                    "Hidden".to_string()
                                } else {
                                    format!("Pattern: {format}")
                                },
                            ))
                            .into_any_element()
                    })
                    .collect(),
            ))
    }

    fn render_pinned_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_selection = self.selected_pinned_index.is_some();
        let selected_index = self.selected_pinned_index.unwrap_or(0);
        let can_move_up = has_selection && selected_index > 0;
        let can_move_down = has_selection && selected_index + 1 < self.draft.pinned.len();

        let app_items: Vec<_> = self
            .draft
            .pinned
            .iter()
            .enumerate()
            .map(|(index, app)| {
                let is_selected = self.selected_pinned_index == Some(index);
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
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.selected_pinned_index = Some(index);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(0x6C7086FF))
                                    .child(format!("{}.", index + 1)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
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
                                    .child("Applications that stay on the dock for quick access."),
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
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                if let Ok(Some(path)) = win::dialogs::pick_app_dialog() {
                                    let stem =
                                        path.file_stem().and_then(|s| s.to_str()).unwrap_or("App");
                                    let mut chars = stem.chars();
                                    let name = match chars.next() {
                                        None => "App".to_string(),
                                        Some(first) => {
                                            first.to_uppercase().collect::<String>()
                                                + chars.as_str()
                                        }
                                    };
                                    this.draft.pinned.push(PinnedApp {
                                        name,
                                        path: path.to_string_lossy().to_string(),
                                        args: Vec::new(),
                                    });
                                    this.selected_pinned_index = Some(this.draft.pinned.len() - 1);
                                    this.status_message = None;
                                    cx.notify();
                                }
                            }))
                            .child("+ Add Application (.exe)..."),
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
                                .child("No pinned applications yet.")
                                .into_any_element(),
                        ]
                    } else {
                        app_items
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(render_action_button(
                        "btn-move-up",
                        "Move Up",
                        can_move_up,
                        rgba(0x313244FF),
                        rgba(0xCDD6F4FF),
                        cx.listener(move |this: &mut Self, _, _, cx| {
                            if let Some(index) = this.selected_pinned_index
                                && index > 0
                            {
                                this.draft.pinned.swap(index, index - 1);
                                this.selected_pinned_index = Some(index - 1);
                                this.status_message = None;
                                cx.notify();
                            }
                        }),
                    ))
                    .child(render_action_button(
                        "btn-move-down",
                        "Move Down",
                        can_move_down,
                        rgba(0x313244FF),
                        rgba(0xCDD6F4FF),
                        cx.listener(move |this: &mut Self, _, _, cx| {
                            if let Some(index) = this.selected_pinned_index
                                && index + 1 < this.draft.pinned.len()
                            {
                                this.draft.pinned.swap(index, index + 1);
                                this.selected_pinned_index = Some(index + 1);
                                this.status_message = None;
                                cx.notify();
                            }
                        }),
                    ))
                    .child(render_action_button(
                        "btn-delete-app",
                        "Remove",
                        has_selection,
                        rgba(0xF38BA833),
                        rgba(0xF38BA8FF),
                        cx.listener(move |this: &mut Self, _, _, cx| {
                            if let Some(index) = this.selected_pinned_index
                                && index < this.draft.pinned.len()
                            {
                                this.draft.pinned.remove(index);
                                if this.draft.pinned.is_empty() {
                                    this.selected_pinned_index = None;
                                } else if index >= this.draft.pinned.len() {
                                    this.selected_pinned_index = Some(this.draft.pinned.len() - 1);
                                }
                                this.status_message = None;
                                cx.notify();
                            }
                        }),
                    )),
            )
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
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgba(0x11111BFF))
            .flex()
            .flex_col()
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
                        SettingsTab::Behavior => self.render_behavior_tab(cx).into_any_element(),
                        SettingsTab::Clock => self.render_clock_tab(cx).into_any_element(),
                        SettingsTab::Pinned => self.render_pinned_tab(cx).into_any_element(),
                    }),
            )
            .child(self.render_footer(cx))
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

fn render_action_button(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    bg: gpui::Rgba,
    text_color: gpui::Rgba,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let base = div()
        .id(id)
        .px_3()
        .py_1p5()
        .rounded_md()
        .text_xs()
        .font_weight(FontWeight::MEDIUM);

    if enabled {
        base.bg(bg)
            .text_color(text_color)
            .cursor_pointer()
            .hover(|s| s.bg(rgba(0x45475AFF)))
            .on_click(on_click)
            .child(label)
            .into_any_element()
    } else {
        base.bg(rgba(0x18182580))
            .text_color(rgba(0x6C7086FF))
            .child(label)
            .into_any_element()
    }
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
}
