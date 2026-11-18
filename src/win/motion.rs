use anyhow::{Context as _, Result};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};
use windows::core::BOOL;

pub fn animations_enabled() -> Result<bool> {
    let mut enabled = BOOL::default();
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes a BOOL to the supplied pointer.
    // The BOOL stays live for the call and this action does not use uiParam.
    unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS::default(),
        )
    }
    .context("failed to read Windows animation preference")?;
    Ok(enabled.as_bool())
}
