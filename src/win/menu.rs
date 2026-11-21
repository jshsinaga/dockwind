use std::ffi::c_void;

use anyhow::{Context as _, Result};
use tracing::warn;
use windows::{
    Win32::{
        Foundation::{HWND, POINT},
        UI::WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetForegroundWindow, HMENU,
            MF_CHECKED, MF_POPUP, MF_SEPARATOR, MF_STRING, SetForegroundWindow, TPM_LEFTALIGN,
            TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN, TrackPopupMenu,
        },
    },
    core::{HSTRING, PCWSTR},
};

use crate::win::autostart;
use crate::win::system_tray::{AudioAction, TrayStatus};

const CMD_RELOAD: usize = 1;
const CMD_AUTOSTART: usize = 2;
const CMD_RESTORE_QUIT: usize = 3;
const CMD_SETTINGS: usize = 4;
const CMD_NEXT_LAYOUT: usize = 5;
const FIRST_LAYOUT_COMMAND: usize = 100;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    OpenSettings,
    UsePinnedLayout(String),
    ReloadConfig,
    ToggleAutostart,
    RestoreAndQuit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrayAction {
    ToggleNativeTaskbar,
    OpenSettings(&'static str),
    Audio(AudioAction),
}

pub struct PopupAnchor {
    hwnd: isize,
    cursor: POINT,
}

impl PopupAnchor {
    pub fn at_cursor(hwnd: isize) -> Result<Self> {
        let mut cursor = POINT::default();
        // SAFETY: cursor is a valid POINT written by the API.
        unsafe { GetCursorPos(&mut cursor) }.context("GetCursorPos failed")?;
        Ok(Self { hwnd, cursor })
    }
}

pub fn show(
    anchor: PopupAnchor,
    layouts: &[(String, bool)],
    next_layout: Option<&str>,
) -> Result<Option<MenuAction>> {
    let autostart_enabled = autostart::is_enabled().unwrap_or_else(|error| {
        warn!(%error, "failed to query autostart state");
        false
    });

    with_popup_menu(|menu| {
        // SAFETY: `menu` is a live popup menu. Each appended string is a temporary
        // HSTRING that lives until AppendMenuW has copied it.
        unsafe {
            AppendMenuW(menu, MF_STRING, CMD_SETTINGS, &HSTRING::from("Settings..."))
                .context("AppendMenuW(Settings) failed")?;
            AppendMenuW(menu, MF_STRING, CMD_RELOAD, &HSTRING::from("Reload config"))
                .context("AppendMenuW(Reload config) failed")?;
            AppendMenuW(
                menu,
                MF_STRING,
                CMD_AUTOSTART,
                &HSTRING::from(if autostart_enabled {
                    "Disable autostart"
                } else {
                    "Enable autostart"
                }),
            )
            .context("AppendMenuW(autostart) failed")?;
            AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null())
                .context("AppendMenuW(separator) failed")?;
            AppendMenuW(
                menu,
                MF_STRING,
                CMD_RESTORE_QUIT,
                &HSTRING::from("Restore taskbar & Quit"),
            )
            .context("AppendMenuW(Restore taskbar & Quit) failed")?;
        }

        if !layouts.is_empty() {
            append_layouts(menu, layouts, next_layout.is_some())?;
        }

        Ok(match popup_command(anchor, menu) {
            CMD_NEXT_LAYOUT => next_layout.map(|name| MenuAction::UsePinnedLayout(name.to_owned())),
            CMD_SETTINGS => Some(MenuAction::OpenSettings),
            CMD_RELOAD => Some(MenuAction::ReloadConfig),
            CMD_AUTOSTART => Some(MenuAction::ToggleAutostart),
            CMD_RESTORE_QUIT => Some(MenuAction::RestoreAndQuit),
            id => id
                .checked_sub(FIRST_LAYOUT_COMMAND)
                .and_then(|index| layouts.get(index))
                .map(|(name, _)| MenuAction::UsePinnedLayout(name.clone())),
        })
    })
}

pub fn show_windows(
    anchor: PopupAnchor,
    windows: &[(isize, String, bool)],
) -> Result<Option<isize>> {
    with_popup_menu(|menu| {
        for (index, (_, title, active)) in windows.iter().enumerate() {
            let flags = if *active {
                MF_STRING | MF_CHECKED
            } else {
                MF_STRING
            };
            let label = HSTRING::from(title.replace('&', "&&"));
            // SAFETY: menu is live and the label is copied during the call.
            unsafe { AppendMenuW(menu, flags, index + 1, &label) }
                .context("AppendMenuW(window title) failed")?;
        }
        Ok(popup_command(anchor, menu)
            .checked_sub(1)
            .and_then(|index| windows.get(index))
            .map(|(hwnd, _, _)| *hwnd))
    })
}

pub fn show_tray(
    anchor: PopupAnchor,
    native_taskbar_hidden: Option<bool>,
    status: TrayStatus,
) -> Result<Option<TrayAction>> {
    let entries = tray_entries(status);
    with_popup_menu(|menu| {
        // SAFETY: the menu is live and AppendMenuW copies labels during each call.
        unsafe {
            if let Some(hidden) = native_taskbar_hidden {
                let label = if hidden {
                    "Show Windows taskbar and tray"
                } else {
                    "Hide Windows taskbar and tray"
                };
                AppendMenuW(menu, MF_STRING, 1, &HSTRING::from(label))
                    .context("AppendMenuW(taskbar toggle) failed")?;
                AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null())
                    .context("AppendMenuW(tray separator) failed")?;
            }
            for (index, (label, _)) in entries.iter().enumerate() {
                AppendMenuW(menu, MF_STRING, index + 2, &HSTRING::from(label.as_str()))
                    .with_context(|| format!("AppendMenuW({label}) failed"))?;
            }
        }

        let command = popup_command(anchor, menu);
        Ok(match command {
            1 if native_taskbar_hidden.is_some() => Some(TrayAction::ToggleNativeTaskbar),
            id => id
                .checked_sub(2)
                .and_then(|index| entries.get(index))
                .map(|(_, action)| *action),
        })
    })
}

fn tray_entries(status: TrayStatus) -> Vec<(String, TrayAction)> {
    let setting = TrayAction::OpenSettings;
    let audio = TrayAction::Audio;
    let mut entries = vec![
        (
            status.network.map_or_else(
                || "Network and internet".into(),
                |network| format!("Network ({network})"),
            ),
            setting("ms-settings:network-status"),
        ),
        (
            status.sound.as_ref().map_or_else(
                || "Sound".into(),
                |sound| {
                    let mute = if sound.muted { ", muted" } else { "" };
                    format!("Sound ({}%{mute})", sound.percentage)
                },
            ),
            setting("ms-settings:sound"),
        ),
    ];
    if let Some(sound) = status.sound {
        entries.push((
            if sound.muted {
                "Unmute sound"
            } else {
                "Mute sound"
            }
            .into(),
            audio(AudioAction::SetOutputMute(!sound.muted)),
        ));
        entries.push((
            "Volume up 5%".into(),
            audio(AudioAction::ChangeVolume(0.05)),
        ));
        entries.push((
            "Volume down 5%".into(),
            audio(AudioAction::ChangeVolume(-0.05)),
        ));
    }
    if let Some(muted) = status.microphone_muted {
        entries.push((
            if muted {
                "Unmute microphone"
            } else {
                "Mute microphone"
            }
            .into(),
            audio(AudioAction::SetMicrophoneMute(!muted)),
        ));
    }
    entries.extend([
        (
            "Microphone privacy".into(),
            setting("ms-settings:privacy-microphone"),
        ),
        (
            status.power.map_or_else(
                || "Power and battery".into(),
                |power| format!("Battery ({power})"),
            ),
            setting("ms-settings:powersleep"),
        ),
        (
            status.input_language.map_or_else(
                || "Language and region".into(),
                |language| format!("Language and region ({language})"),
            ),
            setting("ms-settings:regionlanguage"),
        ),
        (
            "Notification settings".into(),
            setting("ms-settings:notifications"),
        ),
        ("Taskbar settings".into(), setting("ms-settings:taskbar")),
    ]);
    entries
}

fn with_popup_menu<T>(show_menu: impl FnOnce(HMENU) -> Result<T>) -> Result<T> {
    // SAFETY: CreatePopupMenu has no preconditions; a null handle means failure.
    let menu = unsafe { CreatePopupMenu() }.context("CreatePopupMenu failed")?;
    let result = show_menu(menu);

    // SAFETY: `menu` was created by this function and is destroyed exactly once.
    if let Err(error) = unsafe { DestroyMenu(menu) } {
        warn!(%error, "failed to destroy the dock popup menu");
    }
    result
}

fn popup_command(anchor: PopupAnchor, menu: HMENU) -> usize {
    // SAFETY: read-only foreground query; may return null.
    let previous_foreground = unsafe { GetForegroundWindow() };
    let dock = HWND(anchor.hwnd as *mut c_void);
    // SAFETY: setting the dock as foreground owner lets the popup dismiss when
    // the user clicks elsewhere. TrackPopupMenu still runs if activation is denied.
    let _ = unsafe { SetForegroundWindow(dock) };

    // SAFETY: the menu is live, `dock` is the owner, and the coordinates came from
    // GetCursorPos. TPM_RETURNCMD returns the selection without posting WM_COMMAND.
    let command = unsafe {
        TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            anchor.cursor.x,
            anchor.cursor.y,
            None,
            dock,
            None,
        )
    };

    // SAFETY: the calls only query foreground and try to restore the previous window.
    let restored = unsafe {
        GetForegroundWindow().0 != dock.0 || SetForegroundWindow(previous_foreground).as_bool()
    };
    if !restored {
        warn!("failed to restore the foreground window after the dock menu");
    }
    command.0 as usize
}

fn append_layouts(menu: HMENU, layouts: &[(String, bool)], can_cycle: bool) -> Result<()> {
    // SAFETY: the APIs take live menu handles and copy each HSTRING label.
    // On success the parent owns the submenu. On failure it is destroyed below.
    unsafe {
        let submenu = CreatePopupMenu().context("CreatePopupMenu(layouts) failed")?;
        let result = (|| -> Result<()> {
            if can_cycle {
                AppendMenuW(
                    submenu,
                    MF_STRING,
                    CMD_NEXT_LAYOUT,
                    &HSTRING::from("Next layout"),
                )
                .context("AppendMenuW(Next layout) failed")?;
                AppendMenuW(submenu, MF_SEPARATOR, 0, PCWSTR::null())
                    .context("AppendMenuW(layout separator) failed")?;
            }
            for (index, (name, current)) in layouts.iter().enumerate() {
                let mut flags = MF_STRING;
                if *current {
                    flags |= MF_CHECKED;
                }
                let label = HSTRING::from(name.replace('&', "&&"));
                AppendMenuW(submenu, flags, FIRST_LAYOUT_COMMAND + index, &label)
                    .context("AppendMenuW(layout) failed")?;
            }
            AppendMenuW(
                menu,
                MF_POPUP,
                submenu.0 as usize,
                &HSTRING::from("Pinned app layouts"),
            )
            .context("AppendMenuW(layout submenu) failed")?;
            Ok(())
        })();
        if result.is_err()
            && let Err(error) = DestroyMenu(submenu)
        {
            warn!(%error, "failed to destroy the layouts submenu");
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win::system_tray::SoundStatus;

    #[test]
    fn audio_actions_reflect_the_reported_device_state() {
        let entries = tray_entries(TrayStatus {
            sound: Some(SoundStatus {
                percentage: 42,
                muted: true,
            }),
            microphone_muted: Some(false),
            ..TrayStatus::default()
        });
        assert!(
            entries
                .iter()
                .any(|(label, _)| label == "Sound (42%, muted)")
        );
        assert!(
            entries
                .iter()
                .any(|(_, action)| *action == TrayAction::Audio(AudioAction::SetOutputMute(false)))
        );
        assert!(
            entries
                .iter()
                .any(|(_, action)| *action
                    == TrayAction::Audio(AudioAction::SetMicrophoneMute(true)))
        );
    }

    #[test]
    fn tray_keeps_settings_shortcuts_without_audio_devices() {
        let entries = tray_entries(TrayStatus::default());
        assert!(
            entries
                .iter()
                .all(|(_, action)| matches!(action, TrayAction::OpenSettings(_)))
        );
        assert!(entries.iter().any(|(label, _)| label == "Sound"));
    }
}
