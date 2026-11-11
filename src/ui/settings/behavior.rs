use gpui::{Context, FontWeight, IntoElement, SharedString, div, prelude::*, rgba};

use super::SettingsView;
use super::controls::*;
use crate::config::AnimationCurve;

impl SettingsView {
    pub(super) fn render_behavior_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
}
