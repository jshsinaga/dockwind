use std::ffi::c_void;

use anyhow::{Context as _, Result, ensure};
use gpui::Window;
use raw_window_handle::RawWindowHandle;
use tracing::{info, warn};
use windows::{
    Wdk::System::SystemServices::RtlGetVersion,
    Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE, DWMSBT_MAINWINDOW,
            DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_WINDOW_CORNER_PREFERENCE,
            DWMWCP_DONOTROUND, DWMWCP_ROUND, DwmSetWindowAttribute,
        },
        System::SystemInformation::OSVERSIONINFOW,
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
            HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, WS_EX_APPWINDOW,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
        },
    },
};

use crate::config::BlurMode;

pub fn foreground_window() -> HWND {
    // SAFETY: GetForegroundWindow has no preconditions and returns null when no window
    // holds the foreground.
    unsafe { GetForegroundWindow() }
}

pub fn window_hwnd(window: &Window) -> Result<isize> {
    raw_hwnd(window).map(|hwnd| hwnd.0 as isize)
}

pub fn set_click_through(hwnd: isize, enabled: bool) -> Result<()> {
    let hwnd = HWND(hwnd as *mut c_void);

    // SAFETY: `hwnd` belongs to the live dock window. The extended style is updated
    // and read back on the same handle; a zero return from SetWindowLongPtrW is a
    // valid previous value, so success is checked by comparing the style bits.
    unsafe {
        let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if enabled {
            ex_style |= WS_EX_TRANSPARENT.0 as isize;
        } else {
            ex_style &= !(WS_EX_TRANSPARENT.0 as isize);
        }

        let _previous = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
        .context("SetWindowPos(click-through) failed")?;

        let bits = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        ensure!(
            (bits & WS_EX_TRANSPARENT.0 != 0) == enabled,
            "failed to set WS_EX_TRANSPARENT to {enabled}"
        );
    }

    Ok(())
}

pub fn apply_dock_styles(window: &Window, previous_foreground: HWND) -> Result<()> {
    let hwnd = raw_hwnd(window)?;

    // SAFETY: `hwnd` is the live dock window. SetWindowLongPtrW returns the previous
    // value, not success, so the required styles are read back afterwards. The
    // SetWindowPos call uses only size/position flags and no activation.
    unsafe {
        let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        ex_style |= WS_EX_TOOLWINDOW.0 as isize;
        ex_style |= WS_EX_TOPMOST.0 as isize;
        ex_style |= WS_EX_NOACTIVATE.0 as isize;
        ex_style &= !(WS_EX_APPWINDOW.0 as isize);

        let _previous = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
        .context("SetWindowPos(topmost) failed")?;

        let bits = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        ensure!(
            bits & WS_EX_TOOLWINDOW.0 != 0,
            "window is missing WS_EX_TOOLWINDOW"
        );
        ensure!(
            bits & WS_EX_APPWINDOW.0 == 0,
            "window still has WS_EX_APPWINDOW"
        );
        ensure!(
            bits & WS_EX_TOPMOST.0 != 0,
            "window is missing WS_EX_TOPMOST"
        );
        ensure!(
            bits & WS_EX_NOACTIVATE.0 != 0,
            "window is missing WS_EX_NOACTIVATE"
        );

        info!(
            hwnd = %format!("0x{:X}", hwnd.0 as isize),
            ex_style = %format!("0x{bits:08X}"),
            toolwindow = true,
            appwindow = false,
            topmost = true,
            noactivate = true,
            "dock window styles applied"
        );

        if !restore_foreground(hwnd, previous_foreground) {
            warn!("could not restore the previous foreground window");
        }
    }

    Ok(())
}

fn restore_foreground(dock: HWND, previous: HWND) -> bool {
    if previous.0.is_null() {
        return true;
    }

    // SAFETY: both handles were returned by user32. AttachThreadInput is only called
    // with thread ids obtained from live windows, and the attachment is released
    // again before returning.
    unsafe {
        if GetForegroundWindow().0 != dock.0 {
            return true;
        }

        let current_thread = GetCurrentThreadId();
        let foreground = GetForegroundWindow();
        let foreground_thread = if foreground.0.is_null() {
            0
        } else {
            GetWindowThreadProcessId(foreground, None)
        };
        let attached = foreground_thread != 0
            && foreground_thread != current_thread
            && AttachThreadInput(current_thread, foreground_thread, true).as_bool();

        let restored = SetForegroundWindow(previous).as_bool();

        if attached && !AttachThreadInput(current_thread, foreground_thread, false).as_bool() {
            warn!("failed to detach the foreground input queue");
        }

        restored
    }
}

pub fn apply_backdrop(window: &Window, mode: BlurMode) -> Result<()> {
    let hwnd = raw_hwnd(window)?;
    apply_backdrop_to_hwnd(hwnd.0 as isize, mode)
}

pub fn apply_backdrop_to_hwnd(hwnd: isize, mode: BlurMode) -> Result<()> {
    let hwnd = HWND(hwnd as *mut c_void);
    let build = windows_build()?;

    match mode {
        BlurMode::None => {
            if build >= 22000 {
                set_corner_preference(hwnd, DWMWCP_DONOTROUND)?;
            }
            info!(build, "using custom-drawn transparency");
        }
        BlurMode::Mica | BlurMode::Acrylic => {
            if build < 22621 {
                info!(
                    build,
                    mode = ?mode,
                    "DWM backdrop requires Windows 11 22H2; keeping transparent"
                );
                return Ok(());
            }

            let backdrop = match mode {
                BlurMode::Mica => DWMSBT_MAINWINDOW,
                BlurMode::Acrylic => DWMSBT_TRANSIENTWINDOW,
                BlurMode::None => unreachable!("handled above"),
            };

            if let Err(error) = set_backdrop(hwnd, backdrop) {
                warn!(%error, "DWM backdrop failed; keeping transparent");
                return Ok(());
            }
            set_corner_preference(hwnd, DWMWCP_ROUND)?;
            info!(build, mode = ?mode, "DWM backdrop applied");
        }
    }

    Ok(())
}

pub fn windows_build() -> Result<u32> {
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: with a valid dwOSVersionInfoSize, RtlGetVersion writes into `version`
    // and returns an NTSTATUS. Unlike GetVersionExW it is not manifest-shimmed.
    let status = unsafe { RtlGetVersion(&mut version) };
    ensure!(
        status.0 >= 0,
        "RtlGetVersion failed with NTSTATUS 0x{:08X}",
        status.0
    );
    Ok(version.dwBuildNumber)
}

fn set_backdrop(hwnd: HWND, backdrop: DWM_SYSTEMBACKDROP_TYPE) -> Result<()> {
    let value = backdrop.0;
    set_dwm_attribute(
        hwnd,
        DWMWA_SYSTEMBACKDROP_TYPE,
        &value as *const i32 as *const c_void,
        std::mem::size_of::<i32>() as u32,
    )
}

fn set_corner_preference(hwnd: HWND, preference: DWM_WINDOW_CORNER_PREFERENCE) -> Result<()> {
    let value = preference.0;
    set_dwm_attribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE,
        &value as *const i32 as *const c_void,
        std::mem::size_of::<i32>() as u32,
    )
}

fn set_dwm_attribute(
    hwnd: HWND,
    attribute: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE,
    value: *const c_void,
    size: u32,
) -> Result<()> {
    // SAFETY: `value` points to a stack value of `size` bytes in every caller, and
    // `hwnd` is the live dock window. DWM copies the value during the call.
    unsafe { DwmSetWindowAttribute(hwnd, attribute, value, size) }
        .context("DwmSetWindowAttribute failed")
}

fn raw_hwnd(window: &Window) -> Result<HWND> {
    let handle = raw_window_handle::HasWindowHandle::window_handle(window).map_err(|error| {
        anyhow::anyhow!("GPUI window did not expose a raw window handle: {error}")
    })?;
    let raw = handle.as_raw();
    let RawWindowHandle::Win32(win32) = raw else {
        anyhow::bail!("expected a Win32 window handle, got {raw:?}");
    };
    Ok(HWND(win32.hwnd.get() as *mut c_void))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backdrop_value(mode: BlurMode) -> Option<i32> {
        match mode {
            BlurMode::None => None,
            BlurMode::Mica => Some(DWMSBT_MAINWINDOW.0),
            BlurMode::Acrylic => Some(DWMSBT_TRANSIENTWINDOW.0),
        }
    }

    #[test]
    fn blur_modes_map_to_dwm_backdrops() {
        assert_eq!(backdrop_value(BlurMode::None), None);
        assert_eq!(backdrop_value(BlurMode::Mica), Some(2));
        assert_eq!(backdrop_value(BlurMode::Acrylic), Some(3));
    }
}
