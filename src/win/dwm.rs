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
        UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, HWND_TOPMOST, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetWindowLongPtrW,
            SetWindowPos, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        },
    },
};

use crate::config::BlurMode;

pub fn foreground_window() -> HWND {
    unsafe { GetForegroundWindow() }
}

pub fn apply_dock_styles(window: &Window, previous_foreground: HWND) -> Result<()> {
    let hwnd = raw_hwnd(window)?;

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

        if !previous_foreground.0.is_null() {
            let restored = SetForegroundWindow(previous_foreground).as_bool();
            ensure!(restored, "failed to restore the previous foreground window");
        }
    }

    Ok(())
}

pub fn apply_backdrop(window: &Window, mode: BlurMode) -> Result<()> {
    let hwnd = raw_hwnd(window)?;
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
