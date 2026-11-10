use anyhow::{Context as _, Result, bail};
use gpui::{Point, point, px};
use tracing::warn;
use windows::Win32::{
    Foundation::POINT,
    UI::{
        Shell::{
            QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
            SHQueryUserNotificationState,
        },
        WindowsAndMessaging::GetCursorPos,
    },
};

pub fn cursor_pos(scale_factor: f32) -> Result<Point<gpui::Pixels>> {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        bail!("invalid monitor scale factor: {scale_factor}");
    }

    let mut cursor = POINT::default();
    // SAFETY: `cursor` is a valid POINT written by the API.
    unsafe { GetCursorPos(&mut cursor) }.context("GetCursorPos failed")?;
    Ok(point(
        px(cursor.x as f32 / scale_factor),
        px(cursor.y as f32 / scale_factor),
    ))
}

pub fn fullscreen_active() -> bool {
    // SAFETY: SHQueryUserNotificationState has no preconditions and returns a state
    // value. Query failures are logged and treated as "not fullscreen" so the dock
    // keeps working rather than staying hidden forever.
    match unsafe { SHQueryUserNotificationState() } {
        Ok(state) => matches!(
            state,
            QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE
        ),
        Err(error) => {
            warn!(%error, "SHQueryUserNotificationState failed");
            false
        }
    }
}
