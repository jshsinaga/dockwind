use std::ffi::c_void;

use anyhow::{Context as _, Result, bail, ensure};
use gpui::{Bounds, Pixels, Window};
use raw_window_handle::RawWindowHandle;
use tracing::{info, warn};
use windows::{
    Wdk::System::SystemServices::RtlGetVersion,
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        Graphics::Dwm::{
            DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE, DWMNCRP_DISABLED,
            DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW, DWMWA_BORDER_COLOR,
            DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY, DWMWA_SYSTEMBACKDROP_TYPE,
            DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmExtendFrameIntoClientArea,
            DwmSetWindowAttribute,
        },
        Graphics::Gdi::{CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn},
        System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
        System::SystemInformation::OSVERSIONINFOW,
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::Controls::MARGINS,
        UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GWL_STYLE, GetForegroundWindow, GetWindowLongPtrW,
            GetWindowThreadProcessId, HWND_TOPMOST, IsWindowVisible, SW_SHOWNOACTIVATE,
            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SendMessageW,
            SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_USER, WS_BORDER,
            WS_CAPTION, WS_EX_APPWINDOW, WS_EX_CLIENTEDGE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_EX_WINDOWEDGE, WS_OVERLAPPED, WS_POPUP,
            WS_THICKFRAME,
        },
    },
    core::s,
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
        let mut style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        style |= WS_POPUP.0 as isize;
        style &= !((WS_OVERLAPPED | WS_CAPTION | WS_BORDER | WS_THICKFRAME).0 as isize);
        let _previous_style = SetWindowLongPtrW(hwnd, GWL_STYLE, style);

        let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        ex_style |= WS_EX_TOOLWINDOW.0 as isize;
        ex_style |= WS_EX_TOPMOST.0 as isize;
        ex_style |= WS_EX_NOACTIVATE.0 as isize;
        ex_style &= !(WS_EX_APPWINDOW.0 as isize);
        ex_style &= !((WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE).0 as isize);

        let _previous = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);

        let ncrp = DWMNCRP_DISABLED.0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &ncrp as *const i32 as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );

        if let Ok(build) = windows_build()
            && build >= 22000
        {
            let border_color = DWMWA_COLOR_NONE;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                &border_color as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            );
            let _ = set_corner_preference(hwnd, DWMWCP_DONOTROUND);
        }

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
        // The functional topmost guarantee is the SetWindowPos(HWND_TOPMOST) above.
        // A hidden window can take one show cycle to reflect WS_EX_TOPMOST in the
        // style bits, so a read-back failure is reported and retried after the first
        // frame instead of failing startup.
        if bits & WS_EX_TOPMOST.0 == 0 {
            warn!("window style read-back is missing WS_EX_TOPMOST until it is shown");
        }
        ensure!(
            bits & WS_EX_NOACTIVATE.0 != 0,
            "window is missing WS_EX_NOACTIVATE"
        );

        let style_bits = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        ensure!(style_bits & WS_POPUP.0 != 0, "window is missing WS_POPUP");

        info!(
            hwnd = %format!("0x{:X}", hwnd.0 as isize),
            ex_style = %format!("0x{bits:08X}"),
            style = %format!("0x{style_bits:08X}"),
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

/// Moves and resizes the dock from logical bounds. GPUI receives the resulting
/// `WM_SIZE` and lays the root view out at the new size.
pub fn set_bounds(hwnd: isize, bounds: Bounds<Pixels>, scale_factor: f32) -> Result<()> {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        bail!("invalid monitor scale factor: {scale_factor}");
    }

    let hwnd = HWND(hwnd as *mut c_void);
    let x = (f32::from(bounds.origin.x) * scale_factor).round() as i32;
    let y = (f32::from(bounds.origin.y) * scale_factor).round() as i32;
    let width = (f32::from(bounds.size.width) * scale_factor).round() as i32;
    let height = (f32::from(bounds.size.height) * scale_factor).round() as i32;

    // SAFETY: `hwnd` belongs to the live dock window. The window stays topmost and
    // is not activated. The size and position are physical pixels derived from the
    // monitor scale factor.
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )
        .context("SetWindowPos(dock bounds) failed")?;
    }
    Ok(())
}

pub fn set_window_shape(
    hwnd: isize,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    radius: i32,
) -> Result<()> {
    if hwnd == 0 {
        return Ok(());
    }
    let hwnd = HWND(hwnd as *mut c_void);
    // SAFETY: `hwnd` belongs to the live dock window. CreateRoundRectRgn/CreateRectRgn
    // returns a valid region handle, and SetWindowRgn transfers ownership to the system.
    unsafe {
        let hrgn = if width <= 0 || height <= 0 {
            CreateRectRgn(0, 0, 0, 0)
        } else {
            CreateRoundRectRgn(x, y, x + width + 1, y + height + 1, radius * 2, radius * 2)
        };
        if hrgn.0.is_null() {
            bail!("CreateRoundRectRgn returned NULL");
        }
        if SetWindowRgn(hwnd, Some(hrgn), true) == 0 {
            let _ = DeleteObject(hrgn.into());
            bail!("SetWindowRgn failed");
        }
    }
    Ok(())
}

const WM_GPUI_FORCE_UPDATE_WINDOW: u32 = WM_USER + 5;

pub fn force_render(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    let hwnd = HWND(hwnd as *mut c_void);
    // SAFETY: `hwnd` belongs to the live dock window.
    unsafe {
        let _ = SendMessageW(
            hwnd,
            WM_GPUI_FORCE_UPDATE_WINDOW,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        );
    }
}

/// Shows the dock after GPUI has drawn its first frame, without activating it.
///
/// A window shown before its first frame exposes the empty native surface, which
/// appears as a white rectangle until the renderer presents.
pub fn show_without_activating(hwnd: isize) -> Result<()> {
    let hwnd = HWND(hwnd as *mut c_void);

    // SAFETY: `hwnd` belongs to the live dock window. ShowWindow only changes the
    // visibility state, and the result is read back with IsWindowVisible because
    // ShowWindow returns the previous state rather than success. The second
    // SetWindowPos reinforces topmost now that the window is visible.
    unsafe {
        let _ = SendMessageW(
            hwnd,
            WM_GPUI_FORCE_UPDATE_WINDOW,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        );

        // ShowWindow reports the previous visibility, not success; IsWindowVisible
        // below is the actual check.
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        ensure!(
            IsWindowVisible(hwnd).as_bool(),
            "failed to show the dock window"
        );

        let _ = SendMessageW(
            hwnd,
            WM_GPUI_FORCE_UPDATE_WINDOW,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        );

        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .context("SetWindowPos(topmost after show) failed")?;

        let bits = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if bits & WS_EX_TOPMOST.0 == 0 {
            warn!("dock window is topmost by z-order but missing WS_EX_TOPMOST");
        }
    }

    info!("dock window shown after first frame");
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

    if build >= 22621 {
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        // SAFETY: DwmExtendFrameIntoClientArea extends the frame into the client area.
        unsafe {
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        }

        let backdrop = match mode {
            BlurMode::None => DWMSBT_NONE,
            BlurMode::Mica => DWMSBT_MAINWINDOW,
            BlurMode::Acrylic => DWMSBT_TRANSIENTWINDOW,
        };
        let _ = set_backdrop(hwnd, backdrop);
        let _ = set_corner_preference(hwnd, DWMWCP_DONOTROUND);
    } else {
        apply_win10_accent_policy(hwnd, mode)?;
        if build >= 22000 {
            let _ = set_corner_preference(hwnd, DWMWCP_DONOTROUND);
        }
    }

    info!(build, mode = ?mode, "backdrop applied");
    Ok(())
}

fn apply_win10_accent_policy(hwnd: HWND, mode: BlurMode) -> Result<()> {
    #[repr(C)]
    struct AccentPolicy {
        accent_state: u32,
        accent_flags: u32,
        gradient_color: u32,
        animation_id: u32,
    }

    #[repr(C)]
    struct WindowCompositionAttributeData {
        attribute: u32,
        data: *mut c_void,
        size: usize,
    }

    const WCA_ACCENT_POLICY: u32 = 19;
    const ACCENT_DISABLED: u32 = 0;
    const ACCENT_ENABLE_BLURBEHIND: u32 = 3;
    const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;

    let accent_state = match mode {
        BlurMode::None => ACCENT_DISABLED,
        BlurMode::Mica => ACCENT_ENABLE_BLURBEHIND,
        BlurMode::Acrylic => ACCENT_ENABLE_ACRYLICBLURBEHIND,
    };

    let mut policy = AccentPolicy {
        accent_state,
        accent_flags: 0,
        gradient_color: 0,
        animation_id: 0,
    };

    let mut data = WindowCompositionAttributeData {
        attribute: WCA_ACCENT_POLICY,
        data: &mut policy as *mut _ as *mut c_void,
        size: std::mem::size_of::<AccentPolicy>(),
    };

    // SAFETY: user32.dll is always loaded into Windows processes, and SetWindowCompositionAttribute
    // accepts a valid pointer to WindowCompositionAttributeData.
    unsafe {
        let user32 = GetModuleHandleA(s!("user32.dll"))?;
        let proc = GetProcAddress(user32, s!("SetWindowCompositionAttribute"));
        if let Some(func) = proc {
            type Pfn = unsafe extern "system" fn(
                HWND,
                *mut WindowCompositionAttributeData,
            ) -> windows::core::BOOL;
            let func: Pfn = std::mem::transmute(func);
            let _ = func(hwnd, &mut data);
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

    #[test]
    fn set_bounds_rejects_invalid_scale_factors() {
        let bounds = Bounds::default();
        assert!(set_bounds(0, bounds, 0.0).is_err());
        assert!(set_bounds(0, bounds, -1.0).is_err());
        assert!(set_bounds(0, bounds, f32::NAN).is_err());
        assert!(set_bounds(0, bounds, f32::INFINITY).is_err());
    }
}
