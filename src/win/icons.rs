use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::RenderImage;
use image::{Frame, RgbaImage};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        Graphics::Gdi::{
            BI_RGB, BITMAP, BITMAPINFO, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject,
            GetDIBits, GetObjectW, HGDIOBJ,
        },
        Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
        UI::{
            Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW},
            WindowsAndMessaging::{
                DestroyIcon, GCLP_HICON, GetClassLongPtrW, GetIconInfo, HICON, ICON_BIG, ICONINFO,
                SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_GETICON,
            },
        },
    },
    core::{HSTRING, PCWSTR},
};

#[derive(Default)]
pub struct IconCache {
    cache: HashMap<PathBuf, Option<Arc<RenderImage>>>,
}

impl IconCache {
    pub fn get(&mut self, path: &Path, hwnd: Option<isize>) -> Option<Arc<RenderImage>> {
        if let Some(cached) = self.cache.get(path) {
            return cached.clone();
        }

        let icon = hwnd
            .and_then(|hwnd| icon_from_window(HWND(hwnd as *mut c_void)))
            .or_else(|| icon_from_path(path));
        self.cache.insert(path.to_path_buf(), icon.clone());
        icon
    }
}

fn icon_from_window(hwnd: HWND) -> Option<Arc<RenderImage>> {
    let mut result = 0usize;
    // SAFETY: `result` receives an HICON-sized value; the message is sent with a
    // timeout so a hung target cannot block the dock. Zero means timeout or refusal,
    // in which case the documented fallback chain continues.
    let sent = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_GETICON,
            WPARAM(ICON_BIG as usize),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            100,
            Some(&mut result),
        )
    };
    if sent.0 != 0
        && result != 0
        && let Some(image) = icon_to_render_image(HICON(result as *mut c_void), false)
    {
        return Some(image);
    }

    // SAFETY: read-only query for the window's class icon. The returned handle is
    // borrowed from the class and is never destroyed by this module.
    let class_icon = unsafe { GetClassLongPtrW(hwnd, GCLP_HICON) };
    if class_icon != 0
        && let Some(image) = icon_to_render_image(HICON(class_icon as *mut c_void), false)
    {
        return Some(image);
    }

    None
}

fn icon_from_path(path: &Path) -> Option<Arc<RenderImage>> {
    let path = HSTRING::from(path.as_os_str());
    let mut info = SHFILEINFOW::default();
    // SAFETY: the path is a live null-terminated HSTRING and `info` has the size
    // passed to the API. The returned icon is owned by the caller and destroyed in
    // `icon_to_render_image`.
    let result = unsafe {
        SHGetFileInfoW(
            PCWSTR(path.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    if result == 0 || info.hIcon.0.is_null() {
        return None;
    }

    icon_to_render_image(info.hIcon, true)
}

fn icon_to_render_image(icon: HICON, destroy: bool) -> Option<Arc<RenderImage>> {
    let image = convert_icon(icon);
    if destroy {
        // SAFETY: only icons obtained from SHGetFileInfoW set `destroy`, and each is
        // an owned copy that is destroyed exactly once here.
        if let Err(error) = unsafe { DestroyIcon(icon) } {
            tracing::debug!(%error, "failed to destroy an extracted shell icon");
        }
    }
    image
}

fn convert_icon(icon: HICON) -> Option<Arc<RenderImage>> {
    if icon.0.is_null() {
        return None;
    }

    let mut info = ICONINFO::default();
    // SAFETY: `info` is a valid ICONINFO and the icon is a live icon handle. The
    // colour and mask bitmaps returned in `info` are owned by the caller and released
    // below.
    if unsafe { GetIconInfo(icon, &mut info) }.is_err() {
        return None;
    }

    let result = convert_icon_info(&info);
    // SAFETY: GetIconInfo succeeded, so both bitmap handles are owned by this call.
    // Each non-null handle is deleted exactly once.
    unsafe {
        if !info.hbmColor.0.is_null() && !DeleteObject(HGDIOBJ(info.hbmColor.0)).as_bool() {
            tracing::debug!("failed to delete an icon colour bitmap");
        }
        if !info.hbmMask.0.is_null() && !DeleteObject(HGDIOBJ(info.hbmMask.0)).as_bool() {
            tracing::debug!("failed to delete an icon mask bitmap");
        }
    }
    result
}

fn convert_icon_info(info: &ICONINFO) -> Option<Arc<RenderImage>> {
    let bitmap_handle = if !info.hbmColor.0.is_null() {
        info.hbmColor
    } else {
        info.hbmMask
    };
    if bitmap_handle.0.is_null() {
        return None;
    }

    let mut bitmap = BITMAP::default();
    // SAFETY: `bitmap` is a valid BITMAP of the size passed; the handle is one of the
    // icon bitmaps owned by the caller.
    let copied = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap_handle.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut BITMAP as *mut c_void),
        )
    };
    if copied == 0 {
        return None;
    }

    let width = bitmap.bmWidth;
    let height = bitmap.bmHeight;
    if width <= 0 || height <= 0 {
        return None;
    }

    // SAFETY: a memory DC with no bitmap selected is created and released below.
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.0.is_null() {
        return None;
    }

    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    if !info.hbmColor.0.is_null() {
        let mut bitmap_info = bitmap_info(width, height, 32);
        // SAFETY: the DIB buffer is large enough for 32 bits per pixel, the bitmap is
        // live, and the DC is compatible with the display.
        let lines = unsafe {
            GetDIBits(
                dc,
                info.hbmColor,
                0,
                height as u32,
                Some(pixels.as_mut_ptr() as *mut c_void),
                &mut bitmap_info,
                DIB_RGB_COLORS,
            )
        };
        if lines == 0 {
            // SAFETY: `dc` was created above and has no selected objects.
            if !unsafe { DeleteDC(dc) }.as_bool() {
                tracing::debug!("failed to delete an icon memory DC");
            }
            return None;
        }
    }

    let mask_stride = (width as usize).div_ceil(32) * 4;
    let mut mask = vec![0u8; mask_stride * height as usize];
    let mut mask_info = bitmap_info(width, height, 1);
    let mask_lines = if !info.hbmMask.0.is_null() {
        // SAFETY: the mask buffer is sized for a 1-bit DIB of this width and height.
        unsafe {
            GetDIBits(
                dc,
                info.hbmMask,
                0,
                height as u32,
                Some(mask.as_mut_ptr() as *mut c_void),
                &mut mask_info,
                DIB_RGB_COLORS,
            )
        }
    } else {
        0
    };
    // SAFETY: `dc` was created above and is released exactly once here.
    if !unsafe { DeleteDC(dc) }.as_bool() {
        tracing::debug!("failed to delete an icon memory DC");
    }

    let has_alpha = pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0);
    if !has_alpha {
        for y in 0..height as usize {
            for x in 0..width as usize {
                let pixel = (y * width as usize + x) * 4;
                let alpha = if mask_lines > 0 {
                    let byte = mask[y * mask_stride + x / 8];
                    let bit = (byte >> (7 - (x % 8))) & 1;
                    if bit == 1 { 0 } else { 255 }
                } else {
                    255
                };
                pixels[pixel + 3] = alpha;
            }
        }
    }

    let buffer = RgbaImage::from_raw(width as u32, height as u32, pixels)?;
    Some(Arc::new(RenderImage::new([Frame::new(buffer)])))
}

fn bitmap_info(width: i32, height: i32, bit_count: u16) -> BITMAPINFO {
    let mut info = BITMAPINFO::default();
    info.bmiHeader.biSize = std::mem::size_of_val(&info.bmiHeader) as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = bit_count;
    info.bmiHeader.biCompression = BI_RGB.0;
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_icon_is_rejected() {
        assert!(convert_icon(HICON(std::ptr::null_mut())).is_none());
    }
}
