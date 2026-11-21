use gpui::{Context, FontWeight, IntoElement, SharedString, div, prelude::*, px, rgba};

use super::{ActiveInput, SettingsView, controls::render_text_input};

impl SettingsView {
    pub(super) fn save_layout_name(&mut self) {
        let name = self.layout_name.trim();
        if name.is_empty() {
            self.status_message = Some(("Enter a layout name.".into(), true));
            return;
        }
        if self.draft.pinned_layouts.keys().any(|existing| {
            Some(existing) != self.renaming_layout.as_ref() && existing.eq_ignore_ascii_case(name)
        }) {
            self.status_message = Some(("A layout already uses that name.".into(), true));
            return;
        }

        let name = name.to_owned();
        let apps = if let Some(original) = &self.renaming_layout {
            let Some(apps) = self.draft.pinned_layouts.remove(original) else {
                self.clear_layout_name();
                self.status_message = Some(("That layout is no longer available.".into(), true));
                return;
            };
            apps
        } else {
            self.draft.pinned.clone()
        };
        self.draft.pinned_layouts.insert(name, apps);
        self.clear_layout_name();
    }

    pub(super) fn clear_layout_name(&mut self) {
        self.renaming_layout = None;
        self.layout_name.clear();
        self.active_input = None;
        self.status_message = None;
    }

    fn begin_layout_rename(&mut self, name: &str) {
        self.renaming_layout = Some(name.to_owned());
        self.layout_name = name.to_owned();
        self.input_buffer.clone_from(&self.layout_name);
        self.active_input = Some(ActiveInput::LayoutName);
        self.input_selected = true;
        self.status_message = None;
    }

    fn render_layout_name_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let renaming = self.renaming_layout.is_some();
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
                self,
                cx,
            ))
            .child(
                layout_button(
                    "save-pinned-layout",
                    if renaming {
                        "Rename layout"
                    } else {
                        "Save layout"
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.save_layout_name();
                    cx.notify();
                })),
            )
            .when(renaming, |editor| {
                editor.child(
                    layout_button("cancel-rename", "Cancel").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.clear_layout_name();
                            cx.notify();
                        },
                    )),
                )
            })
    }

    fn duplicate_pinned_layout(&mut self, name: &str) {
        let Some(apps) = self.draft.pinned_layouts.get(name).cloned() else {
            return;
        };
        let base = format!("{name} copy");
        let mut copy = base.clone();
        let mut suffix = 2;
        while self
            .draft
            .pinned_layouts
            .keys()
            .any(|existing| existing.eq_ignore_ascii_case(&copy))
        {
            copy = format!("{base} {suffix}");
            suffix += 1;
        }
        self.status_message = Some((format!("Created layout {copy}.").into(), false));
        self.draft.pinned_layouts.insert(copy, apps);
    }

    fn load_pinned_layout(&mut self, name: &str) {
        if let Some(apps) = self.draft.pinned_layouts.get(name) {
            self.draft.pinned.clone_from(apps);
            self.scroll_handle.set_offset(gpui::Point::default());
            self.finish_pinned_edit();
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
            .when(self.renaming_layout.is_none(), |card| {
                card.child(self.render_layout_name_editor(cx))
            })
            .children(self.draft.pinned_layouts.iter().map(|(name, apps)| {
                if self.renaming_layout.as_ref() == Some(name) {
                    return div()
                        .id(SharedString::from(name.clone()))
                        .child(self.render_layout_name_editor(cx))
                        .into_any_element();
                }
                let load_name = name.clone();
                let rename_name = name.clone();
                let duplicate_name = name.clone();
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
                    .child(layout_button("rename", "Rename").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.begin_layout_rename(&rename_name);
                            cx.notify();
                        },
                    )))
                    .child(layout_button("duplicate", "Duplicate").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.duplicate_pinned_layout(&duplicate_name);
                            cx.notify();
                        },
                    )))
                    .child(layout_button("delete", "Delete").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.draft.pinned_layouts.remove(&delete_name);
                            this.status_message = None;
                            cx.notify();
                        },
                    )))
                    .into_any_element()
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
        view.save_layout_name();
        view.draft.pinned.clear();
        view.layout_name = "work".into();
        view.save_layout_name();
        assert_eq!(view.draft.pinned_layouts.len(), 1);

        view.scroll_handle
            .set_offset(gpui::point(px(0.0), px(-100.0)));
        view.load_pinned_layout("Work");
        assert_eq!(view.scroll_handle.offset(), gpui::Point::default());
        assert_eq!(view.draft.pinned, original);
        assert!(view.saved.pinned.is_empty());
    }

    #[test]
    fn rename_preserves_saved_apps_and_rejects_name_collisions() {
        let mut view = SettingsView::new(
            "config.toml".into(),
            Config::default(),
            String::new(),
            false,
        );
        let apps = vec![PinnedApp {
            name: "Editor".into(),
            path: "editor.exe".into(),
            args: vec!["--work".into()],
        }];
        view.draft
            .pinned_layouts
            .insert("Work".into(), apps.clone());
        view.draft.pinned_layouts.insert("Home".into(), Vec::new());

        view.begin_layout_rename("Work");
        view.layout_name = " home ".into();
        view.save_layout_name();
        assert_eq!(view.draft.pinned_layouts.get("Work"), Some(&apps));

        view.layout_name = " WORK ".into();
        view.save_layout_name();
        assert_eq!(view.draft.pinned_layouts.get("WORK"), Some(&apps));
        assert!(!view.draft.pinned_layouts.contains_key("Work"));
        assert!(view.draft.pinned.is_empty());
        assert!(view.renaming_layout.is_none());
    }

    #[test]
    fn duplicating_a_layout_preserves_arguments_and_uses_a_unique_name() {
        let mut view = SettingsView::new(
            "config.toml".into(),
            Config::default(),
            String::new(),
            false,
        );
        let apps = vec![PinnedApp {
            name: "Editor".into(),
            path: "editor.exe".into(),
            args: vec!["--work".into()],
        }];
        view.draft
            .pinned_layouts
            .insert("Work".into(), apps.clone());
        view.draft
            .pinned_layouts
            .insert("work copy".into(), Vec::new());

        view.duplicate_pinned_layout("Work");
        view.duplicate_pinned_layout("Work");
        assert_eq!(view.draft.pinned_layouts.get("Work copy 2"), Some(&apps));
        assert_eq!(view.draft.pinned_layouts.get("Work copy 3"), Some(&apps));
        assert!(view.draft.pinned.is_empty());
    }
}
