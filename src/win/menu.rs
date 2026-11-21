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

const CMD_RELOAD: usize = 1;
const CMD_AUTOSTART: usize = 2;
const CMD_RESTORE_QUIT: usize = 3;
const CMD_SETTINGS: usize = 4;
const CMD_NEXT_LAYOUT: usize = 5;
const FIRST_LAYOUT_COMMAND: usize = 100;
const CMD_TOGGLE_TASKBAR: usize = 1;
const FIRST_TRAY_SETTING_COMMAND: usize = 2;

const TRAY_SETTINGS: &[(&str, &str)] = &[
    ("Network and internet", "ms-settings:network-status"),
    ("Sound", "ms-settings:sound"),
    ("Microphone privacy", "ms-settings:privacy-microphone"),
    ("Power and battery", "ms-settings:powersleep"),
    ("Language and region", "ms-settings:regionlanguage"),
    ("Notifications", "ms-settings:notifications"),
    ("Taskbar settings", "ms-settings:taskbar"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    OpenSettings,
    UsePinnedLayout(String),
    ReloadConfig,
    ToggleAutostart,
    RestoreAndQuit,
}

pub enum TrayAction {
    ToggleNativeTaskbar,
    OpenSettings(&'static str),
}

pub fn show(
    hwnd: isize,
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

        Ok(match popup_command(hwnd, menu)? {
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

pub fn show_tray(hwnd: isize, native_taskbar_hidden: Option<bool>) -> Result<Option<TrayAction>> {
    with_popup_menu(|menu| {
        // SAFETY: the menu is live and AppendMenuW copies the labels during each call.
        unsafe {
            if let Some(hidden) = native_taskbar_hidden {
                let label = if hidden {
                    "Show Windows taskbar and tray"
                } else {
                    "Hide Windows taskbar and tray"
                };
                AppendMenuW(menu, MF_STRING, CMD_TOGGLE_TASKBAR, &HSTRING::from(label))
                    .context("AppendMenuW(taskbar toggle) failed")?;
                AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null())
                    .context("AppendMenuW(tray separator) failed")?;
            }
            for (index, (label, _)) in TRAY_SETTINGS.iter().enumerate() {
                AppendMenuW(
                    menu,
                    MF_STRING,
                    FIRST_TRAY_SETTING_COMMAND + index,
                    &HSTRING::from(*label),
                )
                .with_context(|| format!("AppendMenuW({label}) failed"))?;
            }
        }

        Ok(match popup_command(hwnd, menu)? {
            CMD_TOGGLE_TASKBAR if native_taskbar_hidden.is_some() => {
                Some(TrayAction::ToggleNativeTaskbar)
            }
            id => id
                .checked_sub(FIRST_TRAY_SETTING_COMMAND)
                .and_then(|index| TRAY_SETTINGS.get(index))
                .map(|(_, uri)| TrayAction::OpenSettings(uri)),
        })
    })
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

fn popup_command(hwnd: isize, menu: HMENU) -> Result<usize> {
    let mut cursor = POINT::default();
    // SAFETY: `cursor` is a valid POINT written by the API.
    unsafe { GetCursorPos(&mut cursor) }.context("GetCursorPos failed")?;
    // SAFETY: read-only foreground query; may return null.
    let previous_foreground = unsafe { GetForegroundWindow() };
    let dock = HWND(hwnd as *mut c_void);
    // SAFETY: setting the dock as foreground owner lets the popup dismiss when
    // the user clicks elsewhere. TrackPopupMenu still runs if activation is denied.
    let _ = unsafe { SetForegroundWindow(dock) };

    // SAFETY: the menu is live, `dock` is the owner, and the coordinates came from
    // GetCursorPos. TPM_RETURNCMD returns the selection without posting WM_COMMAND.
    let command = unsafe {
        TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            cursor.x,
            cursor.y,
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
    Ok(command.0 as usize)
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
