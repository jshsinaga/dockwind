use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Controls::Dialogs::{
    CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW, CommDlgExtendedError, GetOpenFileNameW,
    OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::core::{HSTRING, Interface, PCWSTR, w};

use crate::config::parse_rgba_hex;

#[derive(Debug)]
struct ComApartmentGuard(bool);

impl ComApartmentGuard {
    fn new() -> Self {
        // SAFETY: CoInitializeEx initializes the COM apartment on the calling thread.
        let ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() };
        Self(ok)
    }
}

impl Drop for ComApartmentGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: CoUninitialize balances a successful CoInitializeEx on this thread.
            unsafe { CoUninitialize() };
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApp {
    pub name: String,
    pub path: PathBuf,
}

pub fn scan_installed_apps() -> Vec<InstalledApp> {
    let _com = ComApartmentGuard::new();
    let mut search_directories = Vec::new();
    if let Some(app_data) = dirs::data_dir() {
        search_directories.push(app_data.join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    let program_data = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    search_directories.push(program_data.join(r"Microsoft\Windows\Start Menu\Programs"));

    if let Some(desktop) = dirs::desktop_dir() {
        search_directories.push(desktop);
    }
    let public_dir = std::env::var_os("PUBLIC")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\Public"));
    search_directories.push(public_dir.join("Desktop"));

    let mut apps = Vec::new();
    for directory in search_directories {
        collect_shortcuts_recursive(&directory, &mut apps);
    }

    let mut seen_paths = HashSet::new();
    let mut seen_names = HashSet::new();
    apps.retain(|app| {
        let lower_name = app.name.to_lowercase();
        let lower_path = app.path.to_string_lossy().to_lowercase();
        if lower_name.contains("uninstall")
            || lower_name.contains("help")
            || lower_name.contains("readme")
            || lower_name.contains("documentation")
            || lower_name.contains("website")
            || lower_name.contains("web site")
            || lower_name.contains("manual")
            || lower_name.contains("release notes")
            || lower_name.contains("license")
        {
            return false;
        }
        if !seen_paths.insert(lower_path) {
            return false;
        }
        seen_names.insert(lower_name)
    });

    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

pub fn resolve_shortcut(lnk_path: &Path) -> Option<PathBuf> {
    let _com = ComApartmentGuard::new();
    // SAFETY: COM calls to resolve shell shortcut using IShellLinkW and IPersistFile.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist: IPersistFile = link.cast().ok()?;
        let path_hstring = HSTRING::from(lnk_path.as_os_str());
        persist
            .Load(PCWSTR(path_hstring.as_ptr()), STGM_READ)
            .ok()?;

        let mut buffer = [0u16; 1024];
        link.GetPath(&mut buffer, std::ptr::null_mut(), 0).ok()?;
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        if len == 0 {
            return None;
        }
        let target = String::from_utf16_lossy(&buffer[..len]);
        let target_path = PathBuf::from(target);
        if target_path.exists() {
            Some(target_path)
        } else {
            None
        }
    }
}

fn collect_shortcuts_recursive(directory: &Path, apps: &mut Vec<InstalledApp>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_shortcuts_recursive(&path, apps);
        } else if let Some(extension) = path.extension() {
            if extension.eq_ignore_ascii_case("lnk")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && let name = stem.trim().to_string()
                && !name.is_empty()
                && let Some(target) = resolve_shortcut(&path)
                && let Some(ext) = target.extension()
                && ext.eq_ignore_ascii_case("exe")
            {
                apps.push(InstalledApp { name, path: target });
            } else if extension.eq_ignore_ascii_case("exe")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && let name = stem.trim().to_string()
                && !name.is_empty()
            {
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
            let _com = ComApartmentGuard::new();
            let result = pick_app_dialog();
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
    fn test_pick_color_initial_hex_parsing() {
        let parsed = parse_rgba_hex("#1E1E2ECC");
        assert_eq!(parsed, Some([0x1E, 0x1E, 0x2E, 0xCC]));
    }

    #[test]
    fn test_scan_installed_apps_returns_list() {
        let apps = scan_installed_apps();
        assert!(!apps.is_empty());
        for app in &apps {
            assert!(!app.name.is_empty());
            assert!(app.path.exists());
            assert_eq!(
                app.path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_lowercase()),
                Some("exe".to_string())
            );
        }
    }
}
