use std::ffi::c_void;

use anyhow::{Context as _, Result, ensure};
use gpui::Window;
use raw_window_handle::RawWindowHandle;
use tracing::info;
use windows::Win32::{
    Foundation::HWND,
    Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, HWND_TOPMOST, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetWindowLongPtrW,
        SetWindowPos, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    },
};

pub fn foreground_window() -> HWND {
    unsafe { GetForegroundWindow() }
}

pub fn configure_dock_window(window: &Window, previous_foreground: HWND) -> Result<()> {
    let handle = raw_window_handle::HasWindowHandle::window_handle(window).map_err(|error| {
        anyhow::anyhow!("GPUI window did not expose a raw window handle: {error}")
    })?;
    let raw = handle.as_raw();
    let RawWindowHandle::Win32(win32) = raw else {
        anyhow::bail!("expected a Win32 window handle, got {raw:?}");
    };
    let hwnd = HWND(win32.hwnd.get() as *mut c_void);

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

    probe_backdrop(hwnd);
    Ok(())
}

fn probe_backdrop(hwnd: HWND) {
    const DWMWA_SYSTEMBACKDROP_TYPE: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(38);
    const DWMSBT_NONE: i32 = 1;

    let value = DWMSBT_NONE;
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &value as *const i32 as *const c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };

    match result {
        Ok(()) => info!("DWM backdrop attribute 38 accepted"),
        Err(error) => info!(%error, "DWM backdrop attribute 38 rejected"),
    }
}
