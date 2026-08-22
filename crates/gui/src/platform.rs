//! Windows-specific native chrome tweaks.
//!
//! The native caption is tinted to match the app's sidebar color and its
//! icon/title are hidden, so the bar reads as a seamless extension of the
//! layout while keeping native behavior (resize, snap, shadows, buttons).
//!
//! Caption/border/text colors use `DwmSetWindowAttribute`, which requires
//! Windows 11; on Windows 10 the calls are silently ignored by the OS and the
//! bar simply follows the light/dark theme set via `set_theme`.

#![cfg(windows)]

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIcon, SendMessageW, ICON_SMALL, WM_SETICON,
};

/// Sidebar background color per resolved theme, mirroring the CSS tokens:
/// light = `neutral-50` (#FAFAFA); dark = 98% `neutral-950` mixed with white
/// (#0F0F0F).
pub fn sidebar_rgb(dark: bool) -> (u8, u8, u8) {
    if dark {
        (0x0F, 0x0F, 0x0F)
    } else {
        (0xFA, 0xFA, 0xFA)
    }
}

/// Paint the caption background, text and border in `rgb`. Text matching the
/// background effectively hides the window title.
pub fn apply_caption_colors(hwnd: HWND, rgb: (u8, u8, u8)) {
    let (r, g, b) = rgb;
    // COLORREF is 0x00BBGGRR.
    let colorref = (b as u32) << 16 | (g as u32) << 8 | r as u32;

    unsafe {
        for attribute in [DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_BORDER_COLOR] {
            DwmSetWindowAttribute(
                hwnd,
                attribute as u32,
                &colorref as *const u32 as *const core::ffi::c_void,
                size_of::<u32>() as u32,
            );
        }
    }
}

/// Replace the caption's small icon with a fully transparent one so the
/// native titlebar shows neither icon nor title. Safe to call repeatedly;
/// the transparent icon handle is created once and reused.
pub fn hide_caption_icon(hwnd: HWND) {
    static TRANSPARENT_ICON: std::sync::OnceLock<isize> = std::sync::OnceLock::new();
    let handle = *TRANSPARENT_ICON.get_or_init(|| {
        // A monochrome icon whose AND mask is all opaque-zero pixels is fully
        // transparent: 16x16 rows padded to 2 bytes each -> 32 bytes/plane.
        let and_mask = [0xFFu8; 32];
        let xor_bits = [0x00u8; 32];
        unsafe {
            let icon = CreateIcon(
                std::ptr::null_mut(),
                16,
                16,
                1,
                1,
                and_mask.as_ptr(),
                xor_bits.as_ptr(),
            );
            icon as isize
        }
    });

    unsafe {
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, handle);
    }
}
