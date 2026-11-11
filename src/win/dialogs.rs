use std::path::PathBuf;

use anyhow::{Result, bail};
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::UI::Controls::Dialogs::{
    CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW, CommDlgExtendedError, GetOpenFileNameW,
    OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::core::{PCWSTR, w};

use crate::config::parse_rgba_hex;

pub fn pick_app_dialog() -> Result<Option<PathBuf>> {
    let mut buffer = [0u16; 2048];
    let filter: Vec<u16> = "Applications\0*.exe;*.lnk;*.bat;*.cmd\0All files\0*.*\0\0"
        .encode_utf16()
        .collect();
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: HWND::default(),
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: windows::core::PWSTR(buffer.as_mut_ptr()),
        nMaxFile: buffer.len() as u32,
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
        ..Default::default()
    };

    // SAFETY: every pointer in `dialog` refers to a live buffer for the duration of
    // the call. The dialog blocks until the user closes it.
    let accepted = unsafe { GetOpenFileNameW(&mut dialog) }.as_bool();
    if !accepted {
        // SAFETY: CommDlgExtendedError has no preconditions. Zero means the user
        // cancelled rather than an error.
        let error = unsafe { CommDlgExtendedError() };
        if error.0 != 0 {
            bail!("file picker failed with code {}", error.0);
        }
        return Ok(None);
    }

    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Ok(Some(PathBuf::from(String::from_utf16_lossy(
        &buffer[..length],
    ))))
}

pub fn pick_color_dialog(initial_hex: &str) -> Result<Option<String>> {
    let [red, green, blue, alpha] = parse_rgba_hex(initial_hex).unwrap_or([0x1E, 0x1E, 0x2E, 0xCC]);
    let mut custom = [COLORREF(0xFFFFFF); 16];
    let mut dialog = CHOOSECOLORW {
        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: HWND::default(),
        rgbResult: COLORREF((red as u32) | ((green as u32) << 8) | ((blue as u32) << 16)),
        lpCustColors: custom.as_mut_ptr(),
        Flags: CC_FULLOPEN | CC_RGBINIT,
        ..Default::default()
    };

    // SAFETY: every pointer in `dialog` refers to a live buffer for the duration of
    // the call. The dialog blocks until the user closes it.
    if !unsafe { ChooseColorW(&mut dialog) }.as_bool() {
        return Ok(None);
    }

    let value = dialog.rgbResult.0;
    let selected_red = (value & 0xFF) as u8;
    let selected_green = ((value >> 8) & 0xFF) as u8;
    let selected_blue = ((value >> 16) & 0xFF) as u8;
    Ok(Some(format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        selected_red, selected_green, selected_blue, alpha
    )))
}

pub fn focus_existing_settings_window() -> bool {
    let window_title = w!("Dockwind Settings");
    // SAFETY: FindWindowW is a safe read-only search by window title.
    let hwnd = unsafe { FindWindowW(PCWSTR::null(), window_title) };
    if let Ok(hwnd) = hwnd
        && !hwnd.0.is_null()
    {
        // SAFETY: hwnd is a live window found by title.
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
        true
    } else {
        false
    }
}
