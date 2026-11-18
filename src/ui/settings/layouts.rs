use gpui::{Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgba};

use super::{ActiveInput, SettingsView, controls::render_text_input};

impl SettingsView {
    pub(super) fn capture_pinned_layout(&mut self) {
        let name = self.layout_name.trim();
        if name.is_empty() {
            self.status_message = Some(("Enter a layout name.".into(), true));
            return;
        }
        if self
            .draft
            .pinned_layouts
            .keys()
            .any(|existing| existing.eq_ignore_ascii_case(name))
        {
            self.status_message = Some(("A layout already uses that name.".into(), true));
            return;
        }

        self.draft
            .pinned_layouts
            .insert(name.to_owned(), self.draft.pinned.clone());
        self.layout_name.clear();
        self.active_input = None;
        self.status_message = None;
    }

    fn load_pinned_layout(&mut self, name: &str) {
        if let Some(apps) = self.draft.pinned_layouts.get(name) {
            self.draft.pinned.clone_from(apps);
            self.selected_pinned_index = None;
            self.delete_confirm_index = None;
            self.status_message = None;
        }
    }

    pub(super) fn render_pinned_layouts(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_xl()
            .border_1()
            .border_color(rgba(0x313244FF))
            .bg(rgba(0x181825FF))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba(0xCDD6F4FF))
                    .child("Pinned app layouts"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgba(0xA6ADC8FF))
                    .child("Save the current app list as a layout. Load a layout, then Apply & Save to update the dock."),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(render_text_input(
                        "pinned-layout-name",
                        ActiveInput::LayoutName,
                        &self.layout_name,
                        "Layout name",
                        self.active_input,
                        &self.input_buffer,
                        cx,
                    ))
                    .child(
                        layout_button("save-pinned-layout", "Save layout").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.capture_pinned_layout();
                                cx.notify();
                            },
                        )),
                    ),
            )
            .children(self.draft.pinned_layouts.iter().map(|(name, apps)| {
                let load_name = name.clone();
                let update_name = name.clone();
                let delete_name = name.clone();
                div()
                    .id(SharedString::from(name.clone()))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(110.0))
                            .truncate()
                            .text_sm()
                            .text_color(rgba(0xCDD6F4FF))
                            .child(format!("{name} · {} apps", apps.len())),
                    )
                    .child(layout_button("load", "Load").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.load_pinned_layout(&load_name);
                            cx.notify();
                        },
                    )))
                    .child(layout_button("update", "Update").on_click(cx.listener(
                        move |this, _, _, cx| {
                            if let Some(apps) = this.draft.pinned_layouts.get_mut(&update_name) {
                                apps.clone_from(&this.draft.pinned);
                                this.status_message = None;
                                cx.notify();
                            }
                        },
                    )))
                    .child(layout_button("delete", "Delete").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.draft.pinned_layouts.remove(&delete_name);
                            this.status_message = None;
                            cx.notify();
                        },
                    )))
            }))
    }
}

fn layout_button(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_3()
        .py_1p5()
        .rounded_md()
        .bg(rgba(0x313244FF))
        .hover(|style| style.bg(rgba(0x45475AFF)))
        .text_xs()
        .text_color(rgba(0xCDD6F4FF))
        .cursor_pointer()
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, PinnedApp};

    #[test]
    fn saved_layout_is_independent_of_the_current_pinned_draft() {
        let mut view = SettingsView::new(
            "config.toml".into(),
            Config::default(),
            String::new(),
            false,
        );
        view.draft.pinned.push(PinnedApp {
            name: "Editor".into(),
            path: "editor.exe".into(),
            args: vec!["--work".into()],
        });
        let original = view.draft.pinned.clone();
        view.layout_name = "  Work  ".into();
        view.capture_pinned_layout();
        view.draft.pinned.clear();
        view.layout_name = "work".into();
        view.capture_pinned_layout();
        assert_eq!(view.draft.pinned_layouts.len(), 1);

        view.load_pinned_layout("Work");
        assert_eq!(view.draft.pinned, original);
        assert!(view.saved.pinned.is_empty());
    }
}
