use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};
use tracing::debug;
use windows::Win32::{
    Foundation::{CloseHandle, HWND, LPARAM, PROPERTYKEY, SetLastError, WIN32_ERROR},
    Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
    System::Com::{
        CoTaskMemFree,
        StructuredStorage::{PropVariantClear, PropVariantToStringAlloc},
    },
    System::Threading::{
        AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId, OpenProcess,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    },
    UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
    UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetForegroundWindow,
        GetLastActivePopup, GetWindow, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_MINIMIZE, SW_RESTORE,
        SetForegroundWindow, ShowWindow, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    },
};
use windows::core::PWSTR;
use windows::core::{BOOL, GUID};

use crate::dock::WindowInfo;
use crate::win::dialogs::ComApartmentGuard;

const APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
    pid: 5,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct WindowFlags {
    pub is_visible: bool,
    pub is_cloaked: bool,
    pub has_visible_owner: bool,
    pub is_noactivate: bool,
    pub is_toolwindow: bool,
    pub is_appwindow: bool,
    pub has_title: bool,
    pub is_own_process: bool,
}

pub fn list_app_windows() -> Result<Vec<WindowInfo>> {
    let mut handles = Vec::new();
    // SAFETY: the callback only appends to the vector behind `lparam`, which stays
    // alive for the duration of EnumWindows. A false result means an error because
    // the callback always returns TRUE. SetLastError(0) clears any stale Win32 error
    // before enumeration begins.
    unsafe {
        SetLastError(WIN32_ERROR(0));
        EnumWindows(
            Some(enum_windows_proc),
            LPARAM(&mut handles as *mut Vec<HWND> as isize),
        )
    }
    .context("EnumWindows failed")?;

    // SAFETY: both calls have no preconditions. GetCurrentProcessId never fails.
    let current_pid = unsafe { GetCurrentProcessId() };
    // SAFETY: GetForegroundWindow is read-only and may return null.
    let foreground = unsafe { GetForegroundWindow() };
    let mut result = Vec::new();
    let mut process_paths = HashMap::new();
    let _com = ComApartmentGuard::new();

    for hwnd in handles {
        let pid = window_process_id(hwnd);
        if pid == 0 || pid == current_pid {
            continue;
        }
        let title = window_title(hwnd);
        let flags = window_flags(hwnd, !title.trim().is_empty());
        if !is_taskbar_worthy(&flags) {
            continue;
        }

        let exe_path = process_paths
            .entry(pid)
            .or_insert_with(|| match process_path(pid) {
                Ok(path) => Some(path),
                Err(error) => {
                    debug!(%error, pid, "tracking window without an executable path");
                    None
                }
            });
        let app_id = window_app_id(hwnd);
        let (exe_path, display_name, shared_host) = match exe_path.as_deref() {
            Some(path) if is_application_frame_host(path) => {
                match frame_host_app(hwnd, pid, &title) {
                    Some((path, name)) => (Some(path), Some(name), false),
                    None => (None, Some(title.clone()), true),
                }
            }
            path => (path.map(Path::to_path_buf), None, false),
        };

        result.push(WindowInfo {
            hwnd: hwnd.0 as isize,
            title,
            exe_path,
            process_id: pid,
            app_id,
            shared_host,
            display_name,
            // SAFETY: `hwnd` came from EnumWindows in this call and may have closed
            // since; IsIconic reports false for an invalid handle, which is harmless.
            is_minimized: unsafe { IsIconic(hwnd) }.as_bool(),
            is_foreground: false,
        });
    }

    // A modal dialog may be foreground even though its owner is the taskbar entry.
    let mut candidate = foreground;
    while !candidate.0.is_null() {
        if let Some(window) = result
            .iter_mut()
            .find(|window| window.hwnd == candidate.0 as isize)
        {
            window.is_foreground = true;
            break;
        }
        candidate = window_owner(candidate).unwrap_or_default();
    }

    Ok(result)
}

fn window_app_id(hwnd: HWND) -> Option<String> {
    // SAFETY: hwnd was enumerated by Windows and COM is initialized on this
    // thread. The store and property are owned for these calls only.
    let mut property = unsafe {
        SHGetPropertyStoreForWindow::<IPropertyStore>(hwnd)
            .and_then(|store| store.GetValue(&APP_USER_MODEL_ID))
    }
    .ok()?;
    // SAFETY: the conversion accepts an initialized PROPVARIANT. Its output,
    // when present, is an independent null-terminated COM allocation.
    let text = unsafe { PropVariantToStringAlloc(&property) };
    // SAFETY: GetValue initialized the variant; no references into it remain.
    if let Err(error) = unsafe { PropVariantClear(&mut property) } {
        debug!(%error, "failed to clear a window property");
    }
    let text = text.ok()?;
    if text.is_null() {
        return None;
    }
    // SAFETY: text is the null-terminated allocation returned by propsys.
    let value = unsafe { text.to_string() }.ok();
    // SAFETY: the string was allocated with the COM allocator and the Rust
    // conversion copied it before this free.
    unsafe { CoTaskMemFree(Some(text.0.cast())) };
    value.filter(|id| !id.trim().is_empty())
}

fn is_application_frame_host(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("ApplicationFrameHost.exe"))
}

/// Maps a UWP `ApplicationFrameHost` window to the process that owns its hosted content.
/// In Windows 10, the hosted content is in a child window.
/// In Windows 11, the hosted content is in a top-level `Windows.UI.Core.CoreWindow`.
/// If both fail, falls back to the frame process itself with the window title.
fn frame_host_app(frame: HWND, frame_pid: u32, title: &str) -> Option<(PathBuf, String)> {
    let mut children = Vec::new();
    // SAFETY: `children` lives for the call and the callback only appends to it.
    let enumerated = unsafe {
        EnumChildWindows(
            Some(frame),
            Some(enum_child_proc),
            LPARAM(&mut children as *mut Vec<HWND> as isize),
        )
    }
    .as_bool();
    if enumerated {
        for child in children {
            let pid = window_process_id(child);
            if pid == 0 || pid == frame_pid {
                continue;
            }
            let Ok(path) = process_path(pid) else {
                continue;
            };
            if is_application_frame_host(&path) {
                continue;
            }
            let name = title.to_owned();
            debug!(
                frame_pid,
                app_pid = pid,
                path = %path.display(),
                name = %name,
                "mapped UWP frame host via child window"
            );
            return Some((path, name));
        }
    }

    // Windows 11: CoreWindow is a top-level window matching the frame title.
    if let Some((path, name)) = find_core_window_app(title) {
        debug!(
            frame_pid,
            path = %path.display(),
            name = %name,
            "mapped UWP frame host via CoreWindow"
        );
        return Some((path, name));
    }

    None
}

fn find_core_window_app(frame_title: &str) -> Option<(PathBuf, String)> {
    let trimmed = frame_title.trim();
    if trimmed.is_empty() {
        return None;
    }

    struct CoreSearch {
        target: String,
        result: Option<(PathBuf, String)>,
    }

    unsafe extern "system" fn enum_core_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: the caller guarantees `lparam` is the live CoreSearch pointer.
        let state = unsafe { &mut *(lparam.0 as *mut CoreSearch) };
        let class = window_class_name(hwnd);
        if class == "Windows.UI.Core.CoreWindow" {
            let title = window_title(hwnd);
            if title.trim().eq_ignore_ascii_case(&state.target) {
                let pid = window_process_id(hwnd);
                if pid != 0
                    && let Ok(path) = process_path(pid)
                    && !is_application_frame_host(&path)
                {
                    let name = path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().to_string())
                        .unwrap_or_else(|| state.target.clone());
                    state.result = Some((path, name));
                    return BOOL(0);
                }
            }
        }
        BOOL(1)
    }

    let mut state = CoreSearch {
        target: trimmed.to_string(),
        result: None,
    };

    // SAFETY: the callback only interacts with state via lparam.
    unsafe {
        let _ = EnumWindows(
            Some(enum_core_proc),
            LPARAM(&mut state as *mut CoreSearch as isize),
        );
    }

    state.result
}

fn window_class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    // SAFETY: buffer has 256 elements.
    let written = unsafe { GetClassNameW(hwnd, &mut buffer) };
    if written <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..written as usize])
}

pub fn is_taskbar_worthy(flags: &WindowFlags) -> bool {
    flags.is_visible
        && !flags.is_cloaked
        && !flags.is_own_process
        && (flags.is_appwindow
            || (flags.has_title
                && !flags.has_visible_owner
                && !flags.is_toolwindow
                && !flags.is_noactivate))
}

pub fn activate_or_minimize(hwnd: isize) -> Result<()> {
    focus_window(hwnd, true)
}

pub fn activate(hwnd: isize) -> Result<()> {
    focus_window(hwnd, false)
}

fn focus_window(hwnd: isize, minimize_if_active: bool) -> Result<()> {
    let hwnd = HWND(hwnd as *mut c_void);
    if hwnd.0.is_null() {
        bail!("cannot activate an empty window handle");
    }

    // SAFETY: every call accepts the handle made from a previously enumerated window.
    // ShowWindow returns the previous visibility, not success, so the minimize result
    // is verified with IsIconic. The foreground retry attaches input only for live
    // thread ids and detaches them again.
    unsafe {
        let foreground = GetForegroundWindow();
        let popup = GetLastActivePopup(hwnd);
        let foreground_popup = foreground.0 == popup.0
            && !popup.0.is_null()
            && GetWindowLongPtrW(popup, GWL_EXSTYLE) as u32 & WS_EX_APPWINDOW.0 == 0;
        if minimize_if_active && (foreground.0 == hwnd.0 || foreground_popup) {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
            ensure!(
                IsIconic(hwnd).as_bool(),
                "Windows did not minimize the foreground app"
            );
            return Ok(());
        }

        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            ensure!(
                !IsIconic(hwnd).as_bool(),
                "Windows did not restore the minimized app"
            );
        }

        let focus_target = if !popup.0.is_null()
            && IsWindowVisible(popup).as_bool()
            && GetWindowLongPtrW(popup, GWL_EXSTYLE) as u32 & WS_EX_APPWINDOW.0 == 0
        {
            popup
        } else {
            hwnd
        };
        if SetForegroundWindow(focus_target).as_bool() {
            return Ok(());
        }

        let current_thread = GetCurrentThreadId();
        let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let attached = foreground_thread != 0
            && foreground_thread != current_thread
            && AttachThreadInput(current_thread, foreground_thread, true).as_bool();

        let restored = SetForegroundWindow(focus_target).as_bool();
        if attached && !AttachThreadInput(current_thread, foreground_thread, false).as_bool() {
            debug!("failed to detach the foreground input queue");
        }

        if restored {
            Ok(())
        } else {
            bail!("Windows denied SetForegroundWindow for the selected app")
        }
    }
}

fn window_flags(hwnd: HWND, has_title: bool) -> WindowFlags {
    // SAFETY: hwnd and its owner come from read-only window queries.
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
    let has_visible_owner = window_owner(hwnd).is_some_and(|owner| {
        // SAFETY: read-only visibility query for the owner of an enumerated window.
        unsafe { IsWindowVisible(owner) }.as_bool()
    });

    WindowFlags {
        // SAFETY: read-only visibility query for an enumerated window.
        is_visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
        is_cloaked: is_cloaked(hwnd),
        has_visible_owner,
        is_noactivate: ex_style & WS_EX_NOACTIVATE.0 != 0,
        is_toolwindow: ex_style & WS_EX_TOOLWINDOW.0 != 0,
        is_appwindow: ex_style & WS_EX_APPWINDOW.0 != 0,
        has_title,
        is_own_process: false,
    }
}

fn window_owner(hwnd: HWND) -> Option<HWND> {
    // SAFETY: this read-only query accepts an HWND from Windows and reports
    // failure when the window has closed or it has no owner.
    unsafe { GetWindow(hwnd, GW_OWNER) }
        .ok()
        .filter(|owner| !owner.0.is_null())
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    // SAFETY: `cloaked` is a valid u32 of the size passed. DWM copies the attribute
    // into it. A failure means the attribute is unavailable, which is treated as not
    // cloaked; the window filters still apply afterwards.
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    result.is_ok() && cloaked != 0
}

fn window_title(hwnd: HWND) -> String {
    // SAFETY: read-only query for an enumerated window.
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return String::new();
    }

    let mut buffer = vec![0u16; length as usize + 1];
    // SAFETY: the buffer holds `length + 1` UTF-16 code units, as required.
    let written = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if written <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..written as usize])
}

fn window_process_id(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid u32. The returned thread id is not needed.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn process_path(pid: u32) -> Result<PathBuf> {
    if pid == 0 {
        bail!("window has no process id");
    }

    // SAFETY: OpenProcess only reads the target process identity. The returned handle
    // is closed below on every path after the call.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .context("OpenProcess failed")?;

    let mut buffer = vec![0u16; 32768];
    let mut size = buffer.len() as u32;
    // SAFETY: the buffer and `size` are valid; the API writes at most `size` UTF-16
    // code units and updates `size` to the written length.
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    // SAFETY: `handle` was opened above and is closed exactly once here.
    if let Err(error) = unsafe { CloseHandle(handle) } {
        debug!(%error, "failed to close a process handle");
    }

    result.context("QueryFullProcessImageNameW failed")?;
    buffer.truncate(size as usize);
    Ok(PathBuf::from(String::from_utf16_lossy(&buffer)))
}

/// SAFETY: Windows calls this with `lparam` set to the pointer passed to EnumWindows,
/// which points to a live `Vec<HWND>` for the duration of the enumeration.
unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: the caller guarantees `lparam` is the live vector pointer from EnumWindows.
    let windows = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    windows.push(hwnd);
    BOOL(1)
}

/// SAFETY: Windows calls this with `lparam` set to the live vector pointer passed
/// to EnumChildWindows. The callback only appends and always continues.
unsafe extern "system" fn enum_child_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: documented by the caller.
    let children = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    children.push(hwnd);
    BOOL(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worthy_flags() -> WindowFlags {
        WindowFlags {
            is_visible: true,
            has_title: true,
            ..Default::default()
        }
    }

    #[test]
    fn application_frame_host_detection_is_case_insensitive() {
        assert!(is_application_frame_host(Path::new(
            r"C:\Windows\System32\ApplicationFrameHost.exe"
        )));
        assert!(is_application_frame_host(Path::new(
            r"C:\WINDOWS\SYSTEM32\APPLICATIONFRAMEHOST.EXE"
        )));
        assert!(!is_application_frame_host(Path::new(
            r"C:\Windows\System32\notepad.exe"
        )));
    }

    #[test]
    fn normal_window_is_worthy() {
        assert!(is_taskbar_worthy(&worthy_flags()));
    }

    #[test]
    fn hidden_cloaked_untitled_own_windows_are_not_worthy() {
        for flags in [
            WindowFlags {
                is_visible: false,
                ..worthy_flags()
            },
            WindowFlags {
                is_cloaked: true,
                ..worthy_flags()
            },
            WindowFlags {
                has_title: false,
                ..worthy_flags()
            },
            WindowFlags {
                is_own_process: true,
                ..worthy_flags()
            },
        ] {
            assert!(!is_taskbar_worthy(&flags));
        }
    }

    #[test]
    fn tool_windows_and_owned_windows_are_filtered() {
        let tool = WindowFlags {
            is_toolwindow: true,
            ..worthy_flags()
        };
        assert!(!is_taskbar_worthy(&tool));

        let owned = WindowFlags {
            has_visible_owner: true,
            ..worthy_flags()
        };
        assert!(!is_taskbar_worthy(&owned));

        let appwindow = WindowFlags {
            is_toolwindow: true,
            has_visible_owner: true,
            is_appwindow: true,
            ..worthy_flags()
        };
        assert!(is_taskbar_worthy(&appwindow));
    }

    #[test]
    fn explicit_taskbar_windows_are_tracked_even_without_a_caption() {
        assert!(is_taskbar_worthy(&WindowFlags {
            has_title: false,
            is_appwindow: true,
            is_noactivate: true,
            ..worthy_flags()
        }));
        assert!(!is_taskbar_worthy(&WindowFlags {
            is_noactivate: true,
            ..worthy_flags()
        }));
    }

    #[test]
    fn core_window_search_handles_empty_title() {
        assert!(find_core_window_app("").is_none());
        assert!(find_core_window_app("   ").is_none());
    }
}
