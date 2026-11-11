use gpui::{
    AnyElement, Context, FontWeight, IntoElement, SharedString, div, img, prelude::*, px, rgba,
};

use super::{ActiveInput, SettingsView};
use crate::config::PinnedApp;
use crate::win;

impl SettingsView {
    pub(super) fn render_taskbar_confirm_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn render_delete_confirm_dialog(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

    pub(super) fn render_app_picker_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_search_active = self.active_input == Some(ActiveInput::AppPickerSearch);
        let search_text = if is_search_active {
            &self.input_buffer
        } else {
            &self.app_picker_search
        };
        let search_lower = search_text.trim().to_lowercase();
        let filter_letter = self.app_picker_filter;

        let all_apps = self.installed_apps.as_deref().unwrap_or(&[]);
        let total_count = all_apps.len();
        let filtered_apps: Vec<_> = all_apps
            .iter()
            .filter(|app| {
                if let Some(letter) = filter_letter {
                    let first_char = app.name.chars().next().unwrap_or(' ');
                    if !first_char.eq_ignore_ascii_case(&letter) {
                        return false;
                    }
                }
                if !search_lower.is_empty() && !app.name.to_lowercase().contains(&search_lower) {
                    return false;
                }
                true
            })
            .collect();

        let filtered_count = filtered_apps.len();

        let mut alphabet_pills = Vec::new();
        let all_selected = self.app_picker_filter.is_none();
        alphabet_pills.push(
            div()
                .id("filter-letter-all")
                .px_2()
                .py_0p5()
                .rounded_sm()
                .bg(if all_selected {
                    rgba(0x89B4FAFF)
                } else {
                    rgba(0x181825FF)
                })
                .text_xs()
                .font_weight(if all_selected {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if all_selected {
                    rgba(0x11111BFF)
                } else {
                    rgba(0xA6ADC8FF)
                })
                .cursor_pointer()
                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                    this.app_picker_filter = None;
                    cx.notify();
                }))
                .child("All")
                .into_any_element(),
        );

        for letter in 'A'..='Z' {
            let is_letter_selected = self.app_picker_filter == Some(letter);
            alphabet_pills.push(
                div()
                    .id(SharedString::from(format!("filter-letter-{letter}")))
                    .px_1p5()
                    .py_0p5()
                    .rounded_sm()
                    .bg(if is_letter_selected {
                        rgba(0x89B4FAFF)
                    } else {
                        rgba(0x181825FF)
                    })
                    .text_xs()
                    .font_weight(if is_letter_selected {
                        FontWeight::BOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(if is_letter_selected {
                        rgba(0x11111BFF)
                    } else {
                        rgba(0xA6ADC8FF)
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.app_picker_filter = if this.app_picker_filter == Some(letter) {
                            None
                        } else {
                            Some(letter)
                        };
                        cx.notify();
                    }))
                    .child(letter.to_string())
                    .into_any_element(),
            );
        }

        let app_rows: Vec<AnyElement> = if filtered_apps.is_empty() {
            vec![
                div()
                    .py_8()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .text_color(rgba(0x6C7086FF))
                    .child("No applications found matching your criteria.")
                    .into_any_element(),
            ]
        } else {
            filtered_apps
                .into_iter()
                .enumerate()
                .map(|(index, app)| {
                    let app_name = app.name.clone();
                    let app_path_display = app.path.to_string_lossy().to_string();

                    let icon = {
                        let mut cache = self.icon_cache.borrow_mut();
                        cache
                            .entry(app.path.clone())
                            .or_insert_with(|| crate::win::icons::icon_from_path(&app.path))
                            .clone()
                    };

                    let icon_element = match icon {
                        Some(img_data) => {
                            img(img_data).size(px(24.0)).rounded_md().into_any_element()
                        }
                        None => div()
                            .size(px(24.0))
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

                    let target_name = app_name.clone();
                    let target_path_str = app_path_display.clone();

                    div()
                        .id(SharedString::from(format!("app-picker-row-{index}")))
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .bg(rgba(0x18182580))
                        .hover(|s| s.bg(rgba(0x31324480)))
                        .flex()
                        .items_center()
                        .justify_between()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.draft.pinned.push(PinnedApp {
                                name: target_name.clone(),
                                path: target_path_str.clone(),
                                args: Vec::new(),
                            });
                            this.selected_pinned_index = Some(this.draft.pinned.len() - 1);
                            this.showing_app_picker = false;
                            this.active_input = None;
                            this.status_message = None;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .overflow_hidden()
                                .child(icon_element)
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_0p5()
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgba(0xCDD6F4FF))
                                                .child(app_name),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(rgba(0x6C7086FF))
                                                .child(app_path_display),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .px_2p5()
                                .py_1()
                                .rounded_md()
                                .bg(rgba(0x89B4FA26))
                                .hover(|s| s.bg(rgba(0x89B4FA4D)))
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgba(0x89B4FAFF))
                                .child("+ Pin"),
                        )
                        .into_any_element()
                })
                .collect()
        };

        div()
            .id("app-picker-backdrop")
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
                    .id("app-picker-modal")
                    .w(px(640.0))
                    .max_h(px(580.0))
                    .p_5()
                    .rounded_2xl()
                    .bg(rgba(0x181825FF))
                    .border_1()
                    .border_color(rgba(0x313244FF))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2p5()
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgba(0xFFFFFFFF))
                                            .child("Select Installed Application"),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_full()
                                            .bg(rgba(0x313244FF))
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgba(0x89B4FAFF))
                                            .child(format!(
                                                "{filtered_count} / {total_count} apps"
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .id("btn-close-app-picker")
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgba(0x31324480))
                                    .hover(|s| s.bg(rgba(0x45475AFF)))
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgba(0xCDD6F4FF))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.showing_app_picker = false;
                                        this.active_input = None;
                                        cx.notify();
                                    }))
                                    .child("X"),
                            ),
                    )
                    .child(
                        div()
                            .id("app-picker-search-bar")
                            .px_3()
                            .py_2()
                            .rounded_lg()
                            .bg(rgba(0x11111BFF))
                            .border_1()
                            .border_color(if is_search_active {
                                rgba(0x89B4FAFF)
                            } else {
                                rgba(0x313244FF)
                            })
                            .flex()
                            .items_center()
                            .justify_between()
                            .cursor_text()
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.active_input = Some(ActiveInput::AppPickerSearch);
                                this.input_buffer = this.app_picker_search.clone();
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if search_text.is_empty() {
                                        rgba(0x6C7086FF)
                                    } else {
                                        rgba(0xCDD6F4FF)
                                    })
                                    .child(if search_text.is_empty() {
                                        SharedString::from(
                                            "Search installed applications (type to filter)...",
                                        )
                                    } else {
                                        SharedString::from(search_text.to_string())
                                    }),
                            )
                            .children(if !search_text.is_empty() {
                                Some(
                                    div()
                                        .id("btn-clear-search")
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(rgba(0x313244FF))
                                        .hover(|s| s.bg(rgba(0x45475AFF)))
                                        .text_xs()
                                        .text_color(rgba(0xCDD6F4FF))
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                            this.app_picker_search.clear();
                                            this.input_buffer.clear();
                                            this.active_input = Some(ActiveInput::AppPickerSearch);
                                            cx.notify();
                                        }))
                                        .child("Clear"),
                                )
                            } else {
                                None
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .bg(rgba(0x11111BFF))
                            .children(alphabet_pills),
                    )
                    .child(
                        div()
                            .id("app-picker-list-scroll")
                            .h(px(300.0))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_1()
                            .rounded_xl()
                            .bg(rgba(0x11111B80))
                            .border_1()
                            .border_color(rgba(0x31324480))
                            .children(app_rows),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .pt_2()
                            .border_t_1()
                            .border_color(rgba(0x31324480))
                            .child(
                                div()
                                    .id("btn-modal-browse-file")
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
                                        this.showing_app_picker = false;
                                        this.active_input = None;
                                        let receiver = win::dialogs::pick_file_async();
                                        cx.spawn(async move |this, cx| {
                                            if let Ok(Ok(Some(path))) = receiver.await {
                                                let final_path =
                                                    win::dialogs::resolve_shortcut(&path)
                                                        .unwrap_or(path);
                                                let stem = final_path
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
                                                        path: final_path
                                                            .to_string_lossy()
                                                            .to_string(),
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
                                        cx.notify();
                                    }))
                                    .child("Browse for (.exe) file instead..."),
                            )
                            .child(
                                div()
                                    .id("btn-modal-cancel-picker")
                                    .px_4()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(rgba(0x313244FF))
                                    .hover(|s| s.bg(rgba(0x45475AFF)))
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgba(0xCDD6F4FF))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.showing_app_picker = false;
                                        this.active_input = None;
                                        cx.notify();
                                    }))
                                    .child("Cancel"),
                            ),
                    ),
            )
            .into_any_element()
    }
}
