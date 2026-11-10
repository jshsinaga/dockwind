use std::ffi::c_void;

use anyhow::{Context as _, Result};
use tracing::warn;
use windows::{
    Win32::{
        Foundation::{HWND, POINT},
        UI::WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetForegroundWindow,
            MF_SEPARATOR, MF_STRING, SetForegroundWindow, TPM_LEFTALIGN, TPM_RETURNCMD,
            TPM_RIGHTBUTTON, TPM_TOPALIGN, TrackPopupMenu,
        },
    },
    core::{HSTRING, PCWSTR},
};

use crate::win::autostart;

const CMD_RELOAD: usize = 1;
const CMD_AUTOSTART: usize = 2;
const CMD_RESTORE_QUIT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    ReloadConfig,
    ToggleAutostart,
    RestoreAndQuit,
}

pub fn show(hwnd: isize) -> Result<Option<MenuAction>> {
    let autostart_enabled = autostart::is_enabled().unwrap_or_else(|error| {
        warn!(%error, "failed to query autostart state");
        false
    });

    // SAFETY: CreatePopupMenu has no preconditions; a null handle means failure.
    let menu = unsafe { CreatePopupMenu() }.context("CreatePopupMenu failed")?;
    let result = (|| -> Result<Option<MenuAction>> {
        // SAFETY: `menu` is a live popup menu. Each appended string is a temporary
        // HSTRING that lives until AppendMenuW has copied it.
        unsafe {
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

        let mut cursor = POINT::default();
        // SAFETY: `cursor` is a valid POINT written by the API.
        unsafe { GetCursorPos(&mut cursor) }.context("GetCursorPos failed")?;
        // SAFETY: read-only foreground query; may return null.
        let previous_foreground = unsafe { GetForegroundWindow() };
        let dock = HWND(hwnd as *mut c_void);
        // SAFETY: setting the dock as foreground owner is what lets the popup dismiss
        // when the user clicks elsewhere. Failure is tolerated because TrackPopupMenu
        // still runs and the foreground window is restored below when possible.
        let _ = unsafe { SetForegroundWindow(dock) };

        // SAFETY: the menu is live, `dock` is the owner window, and the coordinates
        // came from GetCursorPos. TPM_RETURNCMD makes this call synchronous without a
        // posted WM_COMMAND.
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

        // SAFETY: both calls only read the foreground window and try to restore the
        // previously active window. A denied restore is reported, not fatal.
        let restored = unsafe {
            GetForegroundWindow().0 != dock.0 || SetForegroundWindow(previous_foreground).as_bool()
        };
        if !restored {
            warn!("failed to restore the foreground window after the dock menu");
        }

        Ok(match command.0 as usize {
            CMD_RELOAD => Some(MenuAction::ReloadConfig),
            CMD_AUTOSTART => Some(MenuAction::ToggleAutostart),
            CMD_RESTORE_QUIT => Some(MenuAction::RestoreAndQuit),
            _ => None,
        })
    })();

    // SAFETY: `menu` was created by this function and is destroyed exactly once.
    if let Err(error) = unsafe { DestroyMenu(menu) } {
        warn!(%error, "failed to destroy the dock popup menu");
    }
    result
}
