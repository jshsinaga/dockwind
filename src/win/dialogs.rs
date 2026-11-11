use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Controls::Dialogs::{
    CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW, CommDlgExtendedError, GetOpenFileNameW,
    OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::core::{PCWSTR, w};

use crate::config::parse_rgba_hex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApp {
    pub name: String,
    pub path: PathBuf,
}

pub fn scan_installed_apps() -> Vec<InstalledApp> {
    let mut search_directories = Vec::new();
    if let Some(app_data) = dirs::data_dir() {
        search_directories.push(app_data.join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    let program_data = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    search_directories.push(program_data.join(r"Microsoft\Windows\Start Menu\Programs"));

    let mut apps = Vec::new();
    for directory in search_directories {
        collect_shortcuts_recursive(&directory, &mut apps);
    }

    let mut seen_names = HashSet::new();
    apps.retain(|app| {
        let lower = app.name.to_lowercase();
        if lower.contains("uninstall")
            || lower.contains("help")
            || lower.contains("readme")
            || lower.contains("documentation")
            || lower.contains("website")
            || lower.contains("web site")
            || lower.contains("manual")
        {
            return false;
        }
        seen_names.insert(lower)
    });

    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

fn collect_shortcuts_recursive(directory: &Path, apps: &mut Vec<InstalledApp>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_shortcuts_recursive(&path, apps);
        } else if let Some(extension) = path.extension()
            && extension.eq_ignore_ascii_case("lnk")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            let name = stem.trim().to_string();
            if !name.is_empty() {
                apps.push(InstalledApp { name, path });
            }
        }
    }
}

pub fn pick_color_async(
    initial_hex: String,
) -> futures::channel::oneshot::Receiver<Result<Option<String>>> {
    let (sender, receiver) = futures::channel::oneshot::channel();
    let spawn_result = std::thread::Builder::new()
        .name("dockwind-pick-color".to_string())
        .spawn(move || {
            let result = pick_color_dialog(&initial_hex);
            let _ = sender.send(result);
        });
    if let Err(error) = spawn_result {
        tracing::error!(%error, "failed to spawn color dialog thread");
    }
    receiver
}

pub fn pick_file_async() -> futures::channel::oneshot::Receiver<Result<Option<PathBuf>>> {
    let (sender, receiver) = futures::channel::oneshot::channel();
    let spawn_result = std::thread::Builder::new()
        .name("dockwind-pick-file".to_string())
        .spawn(move || {
            // SAFETY: CoInitializeEx initializes COM on this dedicated thread for the open file dialog.
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }
            let result = pick_app_dialog();
            // SAFETY: CoUninitialize balances CoInitializeEx on this dedicated thread.
            unsafe {
                CoUninitialize();
            }
            let _ = sender.send(result);
        });
    if let Err(error) = spawn_result {
        tracing::error!(%error, "failed to spawn file dialog thread");
    }
    receiver
}

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
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_installed_apps_returns_list() {
        let apps = scan_installed_apps();
        assert!(!apps.is_empty());
        for app in &apps {
            assert!(!app.name.is_empty());
            assert!(app.path.exists());
        }
    }
}
