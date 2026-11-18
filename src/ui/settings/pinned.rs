use std::path::{Path, PathBuf};

use gpui::{Context, FontWeight, IntoElement, SharedString, div, img, prelude::*, px, rgba};

use super::{ActiveInput, DraggedPinnedApp, SettingsView, controls::render_text_input};
use crate::config::PinnedApp;
use crate::win;

impl SettingsView {
    pub(super) fn browse_for_app(&mut self, cx: &mut Context<Self>) {
        if self.native_picker_open {
            return;
        }
        self.native_picker_open = true;
        self.showing_app_picker = false;
        self.active_input = None;
        let receiver = win::dialogs::pick_file_async();

        cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| {
                this.native_picker_open = false;
                match result {
                    Ok(Ok(Some(path))) => this.add_pinned_app(pinned_app_from_path(&path)),
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        this.status_message = Some((
                            format!("Could not open the app picker: {error:#}").into(),
                            true,
                        ));
                    }
                    Err(_) => {
                        this.status_message =
                            Some(("The file picker closed unexpectedly.".into(), true));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn add_pinned_app(&mut self, app: PinnedApp) {
        self.draft.pinned.push(app);
        self.selected_pinned_index = Some(self.draft.pinned.len() - 1);
        self.showing_app_picker = false;
        self.active_input = None;
        self.status_message = None;
    }

    pub(super) fn finish_pinned_name_edit(&mut self) {
        if matches!(self.active_input, Some(ActiveInput::PinnedAppName(_))) {
            self.active_input = None;
            self.input_selected = false;
        }
    }

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
                        if this.selected_pinned_index != Some(index) {
                            this.finish_pinned_name_edit();
                            this.selected_pinned_index = Some(index);
                        }
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
                                this.finish_pinned_name_edit();
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
                                    .min_w(px(0.0))
                                    .child(if is_selected {
                                        render_text_input(
                                            "pinned-app-display-name",
                                            ActiveInput::PinnedAppName(index),
                                            &app.name,
                                            "Display name",
                                            self,
                                            cx,
                                        )
                                    } else {
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgba(0xCDD6F4FF))
                                            .child(app.name.clone())
                                            .into_any_element()
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgba(0x6C7086FF))
                                            .truncate()
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
                                            cx.stop_propagation();
                                            this.finish_pinned_name_edit();
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
                                            cx.stop_propagation();
                                            this.finish_pinned_name_edit();
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
                                        this.open_app_picker(cx);
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
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.browse_for_app(cx);
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
                    .child("Select a row to edit its display name. Drag the handle or use the arrows to reorder it."),
            )
            .child(self.render_pinned_layouts(cx))
            .into_any_element()
    }
}

fn pinned_app_from_path(path: &Path) -> PinnedApp {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("App");
    let mut chars = stem.chars();
    let name = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "App".to_owned(),
    };
    PinnedApp {
        name,
        path: path.to_string_lossy().into_owned(),
        args: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_a_pin_names_only_that_entry_and_keeps_its_arguments() {
        let mut view = SettingsView::new(
            "config.toml".into(),
            crate::config::Config::default(),
            String::new(),
            false,
        );
        let original = PinnedApp {
            name: "Editor".into(),
            path: "editor.exe".into(),
            args: vec!["--work".into()],
        };
        view.draft.pinned = vec![original.clone(), original.clone()];
        view.input_buffer = "  Work editor  ".into();
        view.live_update_input(ActiveInput::PinnedAppName(1));

        assert_eq!(view.draft.pinned[0], original);
        assert_eq!(view.draft.pinned[1].name, "Work editor");
        assert_eq!(view.draft.pinned[1].path, original.path);
        assert_eq!(view.draft.pinned[1].args, original.args);
    }

    #[test]
    fn browsing_uses_the_executable_stem_and_keeps_the_full_path() {
        let path = Path::new("Tools/my.editor.exe");
        let app = pinned_app_from_path(path);
        assert_eq!(app.name, "My.editor");
        assert_eq!(app.path, "Tools/my.editor.exe");
        assert!(app.args.is_empty());
    }
}
