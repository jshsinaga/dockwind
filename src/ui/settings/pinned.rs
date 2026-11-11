use std::path::PathBuf;

use gpui::{Context, FontWeight, IntoElement, SharedString, div, img, prelude::*, px, rgba};

use super::{ActiveInput, DraggedPinnedApp, SettingsView};
use crate::config::PinnedApp;
use crate::win;

impl SettingsView {
    pub(super) fn render_pinned_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .flex()
                            .items_center()
                            .gap_2()
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
                                        if this.installed_apps.is_none() {
                                            this.installed_apps =
                                                Some(win::dialogs::scan_installed_apps());
                                        }
                                        this.showing_app_picker = true;
                                        this.app_picker_search.clear();
                                        this.app_picker_filter = None;
                                        this.active_input = Some(ActiveInput::AppPickerSearch);
                                        this.input_buffer.clear();
                                        this.status_message = None;
                                        cx.notify();
                                    }))
                                    .child("+ Add Application..."),
                            )
                            .child(
                                div()
                                    .id("btn-browse-pinned")
                                    .px_3()
                                    .py_1p5()
                                    .rounded_md()
                                    .bg(rgba(0x313244FF))
                                    .hover(|s| s.bg(rgba(0x45475AFF)))
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgba(0xCDD6F4FF))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|_this: &mut Self, _, _, cx| {
                                        let receiver = win::dialogs::pick_file_async();
                                        cx.spawn(async move |this, cx| {
                                            if let Ok(Ok(Some(path))) = receiver.await {
                                                let final_path = win::dialogs::resolve_shortcut(&path)
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
                                                        path: final_path.to_string_lossy().to_string(),
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
                                    .child("Browse (.exe)..."),
                            ),
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
}
