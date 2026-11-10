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

    let menu = unsafe { CreatePopupMenu() }.context("CreatePopupMenu failed")?;
    let result = (|| -> Result<Option<MenuAction>> {
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
        unsafe { GetCursorPos(&mut cursor) }.context("GetCursorPos failed")?;
        let previous_foreground = unsafe { GetForegroundWindow() };
        let dock = HWND(hwnd as *mut c_void);
        let _ = unsafe { SetForegroundWindow(dock) };

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

        if unsafe { GetForegroundWindow().0 == dock.0 }
            && !unsafe { SetForegroundWindow(previous_foreground) }.as_bool()
        {
            warn!("failed to restore the foreground window after the dock menu");
        }

        Ok(match command.0 as usize {
            CMD_RELOAD => Some(MenuAction::ReloadConfig),
            CMD_AUTOSTART => Some(MenuAction::ToggleAutostart),
            CMD_RESTORE_QUIT => Some(MenuAction::RestoreAndQuit),
            _ => None,
        })
    })();

    let _ = unsafe { DestroyMenu(menu) };
    result
}
