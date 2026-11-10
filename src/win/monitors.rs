use anyhow::{Context as _, Result, bail};
use gpui::{Bounds, Pixels, Size, point, px, size};
use windows::Win32::{
    Foundation::RECT,
    Graphics::Gdi::{
        GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow,
    },
    UI::{
        HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        WindowsAndMessaging::GetDesktopWindow,
    },
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorInfo {
    pub bounds: Bounds<Pixels>,
    pub work_area: Bounds<Pixels>,
    pub scale_factor: f32,
}

impl MonitorInfo {
    pub fn primary() -> Result<Self> {
        let monitor = unsafe { MonitorFromWindow(GetDesktopWindow(), MONITOR_DEFAULTTOPRIMARY) };
        if !is_valid(monitor) {
            bail!("no primary monitor is available");
        }

        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return Err(anyhow::anyhow!(
                "GetMonitorInfoW failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let mut dpi_x = 0u32;
        let mut dpi_y = 0u32;
        unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
            .context("GetDpiForMonitor failed")?;

        if dpi_x == 0 || dpi_y == 0 {
            bail!("GetDpiForMonitor returned a zero DPI");
        }

        Ok(Self::from_physical(info.rcMonitor, info.rcWork, dpi_x))
    }

    fn from_physical(monitor: RECT, work_area: RECT, dpi: u32) -> Self {
        let scale_factor = dpi as f32 / 96.0;
        Self {
            bounds: rect_to_bounds(monitor, scale_factor),
            work_area: rect_to_bounds(work_area, scale_factor),
            scale_factor,
        }
    }
}

pub fn dock_rect(
    monitor: &MonitorInfo,
    content_size: Size<Pixels>,
    margin_bottom: f32,
) -> Bounds<Pixels> {
    let origin = point(
        monitor.work_area.origin.x + (monitor.work_area.size.width - content_size.width) / 2.0,
        monitor.work_area.origin.y + monitor.work_area.size.height
            - content_size.height
            - px(margin_bottom),
    );
    Bounds::new(origin, content_size)
}

fn rect_to_bounds(rect: RECT, scale_factor: f32) -> Bounds<Pixels> {
    Bounds::new(
        point(
            px(rect.left as f32 / scale_factor),
            px(rect.top as f32 / scale_factor),
        ),
        size(
            px((rect.right - rect.left) as f32 / scale_factor),
            px((rect.bottom - rect.top) as f32 / scale_factor),
        ),
    )
}

fn is_valid(monitor: HMONITOR) -> bool {
    !monitor.0.is_null()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn physical_monitor_rects_convert_at_100_to_200_percent_dpi() {
        for dpi in [96u32, 120, 144, 192] {
            let width = 1920 * dpi as i32 / 96;
            let height = 1080 * dpi as i32 / 96;
            let work_height = 1040 * dpi as i32 / 96;
            let info = MonitorInfo::from_physical(
                rect(0, 0, width, height),
                rect(0, 0, width, work_height),
                dpi,
            );

            assert_eq!(info.bounds.size, size(px(1920.0), px(1080.0)));
            assert_eq!(info.work_area.size, size(px(1920.0), px(1040.0)));
            assert_eq!(info.scale_factor, dpi as f32 / 96.0);
        }
    }

    #[test]
    fn dock_is_centered_at_the_bottom_of_the_work_area() {
        let monitor = MonitorInfo {
            bounds: Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1080.0))),
            work_area: Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1040.0))),
            scale_factor: 1.0,
        };
        let bounds = dock_rect(&monitor, size(px(560.0), px(92.0)), 12.0);
        assert_eq!(bounds.origin, point(px(680.0), px(936.0)));
        assert_eq!(bounds.size, size(px(560.0), px(92.0)));
    }

    #[test]
    fn dock_respects_a_work_area_offset() {
        let monitor = MonitorInfo {
            bounds: Bounds::new(point(px(1920.0), px(0.0)), size(px(1920.0), px(1080.0))),
            work_area: Bounds::new(point(px(1920.0), px(0.0)), size(px(1920.0), px(1040.0))),
            scale_factor: 1.0,
        };
        let bounds = dock_rect(&monitor, size(px(560.0), px(92.0)), 12.0);
        assert_eq!(bounds.origin, point(px(2600.0), px(936.0)));
    }
}
