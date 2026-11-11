use std::ffi::c_void;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};
use tracing::debug;
use windows::Win32::{
    Foundation::{CloseHandle, HWND, LPARAM},
    Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
    System::Threading::{
        AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId, OpenProcess,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    },
    UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetForegroundWindow, GetWindow,
        GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsIconic, IsWindowVisible, SW_MINIMIZE, SW_RESTORE, SetForegroundWindow, ShowWindow,
        WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    },
};
use windows::core::BOOL;
use windows::core::PWSTR;

use crate::dock::WindowInfo;

#[derive(Debug, Clone, Copy, Default)]
pub struct WindowFlags {
    pub is_visible: bool,
    pub is_cloaked: bool,
    pub has_owner: bool,
    pub is_toolwindow: bool,
    pub is_appwindow: bool,
    pub has_title: bool,
    pub is_own_process: bool,
}

pub fn list_app_windows() -> Result<Vec<WindowInfo>> {
    let mut handles = Vec::new();
    // SAFETY: the callback only appends to the vector behind `lparam`, which stays
    // alive for the duration of EnumWindows. A false result means an error because
    // the callback always returns TRUE.
    unsafe {
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

    for hwnd in handles {
        let flags = window_flags(hwnd, current_pid);
        if !is_taskbar_worthy(&flags) {
            continue;
        }

        let title = window_title(hwnd);
        let pid = window_process_id(hwnd);
        let exe_path = match process_path(pid) {
            Ok(path) => path,
            Err(error) => {
                debug!(%error, pid, "skipping window without an executable path");
                continue;
            }
        };

        let (exe_path, display_name) = if is_application_frame_host(&exe_path) {
            match frame_host_app(hwnd, pid) {
                Some((path, name)) => (path, Some(name)),
                None => {
                    debug!(pid, "skipping an unmapped UWP ApplicationFrameHost window");
                    continue;
                }
            }
        } else {
            (exe_path, None)
        };

        result.push(WindowInfo {
            hwnd: hwnd.0 as isize,
            title,
            exe_path,
            display_name,
            // SAFETY: `hwnd` came from EnumWindows in this call and may have closed
            // since; IsIconic reports false for an invalid handle, which is harmless.
            is_minimized: unsafe { IsIconic(hwnd) }.as_bool(),
            is_foreground: hwnd.0 == foreground.0,
        });
    }

    Ok(result)
}

fn is_application_frame_host(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("ApplicationFrameHost.exe"))
}

/// Maps a UWP `ApplicationFrameHost` window to the process that owns its hosted
/// `Windows.UI.Core.CoreWindow`. The frame handle stays the activation target.
fn frame_host_app(frame: HWND, frame_pid: u32) -> Option<(PathBuf, String)> {
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
    if !enumerated {
        return None;
    }

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
        let title = window_title(frame);
        let name = if title.trim().is_empty() {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string())
        } else {
            title
        };
        debug!(
            frame_pid,
            app_pid = pid,
            path = %path.display(),
            name = %name,
            "mapped UWP frame host"
        );
        return Some((path, name));
    }
    None
}

pub fn is_taskbar_worthy(flags: &WindowFlags) -> bool {
    flags.is_visible
        && !flags.is_cloaked
        && flags.has_title
        && !flags.is_own_process
        && (flags.is_appwindow || (!flags.has_owner && !flags.is_toolwindow))
}

pub fn activate_or_minimize(hwnd: isize) -> Result<()> {
    let hwnd = HWND(hwnd as *mut c_void);
    if hwnd.0.is_null() {
        bail!("cannot activate an empty window handle");
    }

    // SAFETY: every call accepts the handle made from a previously enumerated window.
    // ShowWindow returns the previous visibility, not success, so the minimize result
    // is verified with IsIconic. The foreground retry attaches input only for live
    // thread ids and detaches them again.
    unsafe {
        if GetForegroundWindow().0 == hwnd.0 {
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

        if SetForegroundWindow(hwnd).as_bool() {
            return Ok(());
        }

        let current_thread = GetCurrentThreadId();
        let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let attached = foreground_thread != 0
            && foreground_thread != current_thread
            && AttachThreadInput(current_thread, foreground_thread, true).as_bool();

        let restored = SetForegroundWindow(hwnd).as_bool();
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

fn window_flags(hwnd: HWND, current_pid: u32) -> WindowFlags {
    // SAFETY: all calls take a handle from EnumWindows. Style and owner queries are
    // read-only; GetWindow(GW_OWNER) returns an error when the window has no owner.
    let (ex_style, owner) = unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let owner = GetWindow(hwnd, GW_OWNER)
            .ok()
            .is_some_and(|owner| !owner.0.is_null());
        (ex_style, owner)
    };

    WindowFlags {
        // SAFETY: read-only visibility query for an enumerated window.
        is_visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
        is_cloaked: is_cloaked(hwnd),
        has_owner: owner,
        is_toolwindow: ex_style & WS_EX_TOOLWINDOW.0 != 0,
        is_appwindow: ex_style & WS_EX_APPWINDOW.0 != 0,
        has_title: !window_title(hwnd).trim().is_empty(),
        is_own_process: window_process_id(hwnd) == current_pid,
    }
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
            has_owner: true,
            ..worthy_flags()
        };
        assert!(!is_taskbar_worthy(&owned));

        let appwindow = WindowFlags {
            is_toolwindow: true,
            has_owner: true,
            is_appwindow: true,
            ..worthy_flags()
        };
        assert!(is_taskbar_worthy(&appwindow));
    }
}
