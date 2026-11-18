mod appearance;
mod behavior;
mod clock;
pub mod controls;
mod modals;
mod pinned;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use gpui::{
    App, Bounds, Context, FocusHandle, FontWeight, IntoElement, KeyDownEvent, Render, RenderImage,
    ScrollHandle, ScrollStrategy, SharedString, TitlebarOptions, UniformListScrollHandle, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, point, prelude::*,
    px, rgba, size,
};
use tracing::error;

use crate::config::{Config, ConfigError};
use crate::win;

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
    AppPickerSearch,
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
    pub(super) config_path: PathBuf,
    pub(super) draft: Config,
    pub(super) saved: Config,
    saved_source: String,
    overwrite_pending: bool,
    pub(super) autostart_draft: bool,
    pub(super) autostart_saved: bool,
    pub(super) active_tab: SettingsTab,
    pub(super) selected_pinned_index: Option<usize>,
    pub(super) delete_confirm_index: Option<usize>,
    pub(super) status_message: Option<(SharedString, bool)>,
    pub(super) scroll_handle: ScrollHandle,
    pub(super) installed_apps: Option<Vec<win::dialogs::InstalledApp>>,
    pub(super) showing_app_picker: bool,
    app_picker_loading: bool,
    app_picker_scroll: UniformListScrollHandle,
    pub(super) app_picker_search: String,
    pub(super) app_picker_filter: Option<char>,
    pub(super) showing_taskbar_confirm: bool,
    pub(super) active_input: Option<ActiveInput>,
    pub(super) input_buffer: String,
    pub(super) focus_handle: Option<FocusHandle>,
    pub(super) font_dropdown_open: bool,
    pub(super) icon_cache: RefCell<HashMap<PathBuf, Option<Arc<RenderImage>>>>,
}

impl SettingsView {
    fn new(config_path: PathBuf, config: Config, saved_source: String, autostart: bool) -> Self {
        Self {
            config_path,
            draft: config.clone(),
            saved: config,
            saved_source,
            overwrite_pending: false,
            autostart_draft: autostart,
            autostart_saved: autostart,
            active_tab: SettingsTab::Appearance,
            selected_pinned_index: None,
            delete_confirm_index: None,
            status_message: None,
            scroll_handle: ScrollHandle::default(),
            installed_apps: None,
            showing_app_picker: false,
            app_picker_loading: false,
            app_picker_scroll: UniformListScrollHandle::default(),
            app_picker_search: String::new(),
            app_picker_filter: None,
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

    fn open_app_picker(&mut self, cx: &mut Context<Self>) {
        self.showing_app_picker = true;
        self.app_picker_search.clear();
        self.app_picker_filter = None;
        self.active_input = Some(ActiveInput::AppPickerSearch);
        self.input_buffer.clear();
        self.status_message = None;
        self.reset_app_picker_scroll();

        if self.installed_apps.is_none() && !self.app_picker_loading {
            self.app_picker_loading = true;
            let scan = cx
                .background_executor()
                .spawn(async { win::dialogs::scan_installed_apps() });
            cx.spawn(async move |this, cx| {
                let apps = scan.await;
                let _ = this.update(cx, |this, cx| {
                    this.installed_apps = Some(apps);
                    this.app_picker_loading = false;
                    cx.notify();
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn reset_app_picker_scroll(&self) {
        self.app_picker_scroll
            .scroll_to_item_strict(0, ScrollStrategy::Top);
    }

    fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        let modifiers = event.keystroke.modifiers;
        if key == "escape" {
            if self.showing_taskbar_confirm {
                self.showing_taskbar_confirm = false;
                cx.notify();
                return;
            }
            if self.showing_app_picker {
                self.showing_app_picker = false;
                self.active_input = None;
                cx.notify();
                return;
            }
            if self.delete_confirm_index.take().is_some() {
                cx.notify();
                return;
            }
        }

        if modifiers.control && !modifiers.alt && key == "s" && !event.is_held {
            if !self.showing_taskbar_confirm
                && !self.showing_app_picker
                && self.delete_confirm_index.is_none()
                && (self.has_unsaved_changes() || self.active_input.is_some())
            {
                self.apply_and_save(false, cx);
            }
            return;
        }

        let Some(field) = self.active_input else {
            return;
        };
        if key == "escape" || key == "enter" {
            self.live_update_input(field);
            self.active_input = None;
            cx.notify();
            return;
        }

        let hex_field = matches!(
            field,
            ActiveInput::BackgroundHex | ActiveInput::BorderColorHex | ActiveInput::ClockFontColor
        );
        if modifiers.control && !modifiers.alt && key == "v" {
            let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
                return;
            };
            if hex_field {
                let hex = text.trim();
                if crate::config::parse_rgba_hex(hex).is_none() {
                    return;
                }
                self.input_buffer = hex.to_ascii_uppercase();
            } else {
                self.input_buffer = text
                    .chars()
                    .filter(|ch| !ch.is_control())
                    .take(64)
                    .collect();
            }
        } else if key == "backspace" {
            self.input_buffer.pop();
        } else {
            let Some(text) = &event.keystroke.key_char else {
                return;
            };
            for ch in text.chars().filter(|ch| !ch.is_control()) {
                if hex_field {
                    if self.input_buffer.len() < 9
                        && (ch.is_ascii_hexdigit() || (ch == '#' && self.input_buffer.is_empty()))
                    {
                        self.input_buffer.push(ch.to_ascii_uppercase());
                    }
                } else if self.input_buffer.chars().count() < 64 {
                    self.input_buffer.push(ch);
                }
            }
        }
        self.live_update_input(field);
        cx.notify();
    }

    fn live_update_input(&mut self, field: ActiveInput) {
        match field {
            ActiveInput::BackgroundHex => {
                let trimmed = self.input_buffer.trim();
                let clean = trimmed.strip_prefix('#').unwrap_or(trimmed);
                if (clean.len() == 6 || clean.len() == 8)
                    && clean.bytes().all(|b| b.is_ascii_hexdigit())
                {
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
            ActiveInput::AppPickerSearch => {
                if self.app_picker_search != self.input_buffer {
                    self.app_picker_search.clone_from(&self.input_buffer);
                    self.reset_app_picker_scroll();
                }
            }
        }
    }

    fn apply_and_save(&mut self, overwrite: bool, cx: &mut Context<Self>) {
        if let Some(field) = self.active_input.take() {
            self.live_update_input(field);
        }
        let expected = (!overwrite).then_some(self.saved_source.as_str());
        self.overwrite_pending = false;
        match self
            .draft
            .save_atomic_preserving(&self.config_path, expected)
        {
            Ok((loaded, saved_source)) => {
                self.saved_source = saved_source;
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

                self.status_message = Some((SharedString::from("Settings saved"), false));
            }
            Err(ConfigError::ChangedOnDisk { .. }) => {
                self.overwrite_pending = true;
                self.status_message = None;
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
            .flex_wrap()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w(px(0.0))
                    .max_w_full()
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
                            .truncate()
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
                                if let Some(field) = this.active_input.take() {
                                    this.live_update_input(field);
                                }
                                this.active_tab = tab;
                                this.showing_app_picker = false;
                                this.delete_confirm_index = None;
                                this.showing_taskbar_confirm = false;
                                cx.notify();
                            }))
                            .child(label)
                    })),
            )
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (status_text, status_color) = if self.overwrite_pending {
            (
                SharedString::from(
                    "Config changed on disk. Reopen Settings to load it, or overwrite with this draft.",
                ),
                rgba(0xF9E2AFFF),
            )
        } else if let Some((msg, is_error)) = &self.status_message {
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
            .gap_4()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(status_color)
                    .child(status_text),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
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
                                this.apply_and_save(this.overwrite_pending, cx);
                            }))
                            .child(if self.overwrite_pending {
                                "Overwrite config"
                            } else {
                                "Apply & Save"
                            }),
                    ),
            )
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
                    .min_h(px(0.0))
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
            } else if self.showing_app_picker {
                Some(self.render_app_picker_dialog(cx))
            } else {
                self.delete_confirm_index
                    .map(|idx| self.render_delete_confirm_dialog(idx, cx))
            })
    }
}

pub fn open_settings_window(cx: &mut App, config_path: PathBuf) -> Result<()> {
    if win::dialogs::focus_existing_settings_window() {
        return Ok(());
    }

    let (loaded, saved_source) = Config::load_or_create_with_source(&config_path)
        .context("failed to load configuration for settings window")?;

    let monitor = win::monitors::MonitorInfo::primary()
        .context("failed to query primary monitor for settings window")?;

    let window_width = px(780.0).min(monitor.work_area.size.width);
    let window_height = px(720.0).min(monitor.work_area.size.height);
    let origin_x = monitor.work_area.origin.x + (monitor.work_area.size.width - window_width) / 2.0;
    let origin_y =
        monitor.work_area.origin.y + (monitor.work_area.size.height - window_height) / 2.0;
    let bounds = Bounds::new(point(origin_x, origin_y), size(window_width, window_height));

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(
            window_width.min(px(480.0)),
            window_height.min(px(420.0)),
        )),
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

    cx.open_window(options, |window, cx| {
        cx.new(|cx| {
            let mut view =
                SettingsView::new(config_path, loaded.config, saved_source, autostart_enabled);
            let focus = cx.focus_handle();
            window.focus(&focus);
            view.focus_handle = Some(focus);
            view
        })
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
    use super::controls::HOTKEY_PRESETS;
    use super::*;
    use crate::config::PinnedApp;

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
        let mut view = SettingsView::new(path, config, String::new(), false);
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
        let mut view = SettingsView::new(path, config, String::new(), false);
        assert_eq!(view.active_tab, SettingsTab::Appearance);

        view.active_tab = SettingsTab::Clock;
        assert_eq!(view.active_tab, SettingsTab::Clock);
    }

    #[test]
    fn settings_view_delete_confirm_state() {
        let path = PathBuf::from("config.toml");
        let config = Config::default();
        let mut view = SettingsView::new(path, config, String::new(), false);
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
        let mut view = SettingsView::new(path, config, String::new(), false);
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
        let mut view = SettingsView::new(path, config, String::new(), false);
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
        let mut view = SettingsView::new(path, config, String::new(), false);
        assert!(view.active_input.is_none());
        assert!(view.input_buffer.is_empty());

        view.active_input = Some(ActiveInput::BackgroundHex);
        view.input_buffer = "#FFFFFF".to_string();
        assert_eq!(view.active_input, Some(ActiveInput::BackgroundHex));
        assert_eq!(view.input_buffer, "#FFFFFF");
    }

    #[test]
    fn background_hex_input_preserves_alpha() {
        let mut view = SettingsView::new(
            PathBuf::from("config.toml"),
            Config::default(),
            String::new(),
            false,
        );
        view.input_buffer = "#14263A80".to_string();
        view.live_update_input(ActiveInput::BackgroundHex);
        assert_eq!(view.draft.appearance.background, "#14263A80");
    }

    #[test]
    fn settings_view_app_picker_state() {
        let path = PathBuf::from("config.toml");
        let config = Config::default();
        let mut view = SettingsView::new(path, config, String::new(), false);
        assert!(!view.showing_app_picker);
        assert!(view.app_picker_search.is_empty());
        assert_eq!(view.app_picker_filter, None);
        assert!(view.installed_apps.is_none());

        view.showing_app_picker = true;
        view.app_picker_search = "Code".to_string();
        view.app_picker_filter = Some('C');
        view.installed_apps = Some(vec![win::dialogs::InstalledApp {
            name: "Visual Studio Code".to_string(),
            path: PathBuf::from(r"C:\Tools\Code.exe"),
        }]);

        assert!(view.showing_app_picker);
        assert_eq!(view.app_picker_search, "Code");
        assert_eq!(view.app_picker_filter, Some('C'));
        assert_eq!(view.installed_apps.as_ref().map(|a| a.len()), Some(1));
    }
}
