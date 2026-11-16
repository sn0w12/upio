//! The native window caption, colored from the app's palette.
//!
//! Windows colors the DWM caption, macOS makes the native titlebar transparent over the
//! window's background color. Both keep the platform's own title text and buttons. Linux keeps
//! the window manager's caption, which no application can recolor.

use rune::Color;

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn apply(caption: Color, text: Color, dark: bool) {
    imp::apply(caption, text, dark);
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn apply(_caption: Color, _text: Color, _dark: bool) {}

#[cfg(target_os = "windows")]
mod imp {
    use std::mem::size_of;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::Mutex;

    use rune::Color;
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowLongPtrW, GetWindowThreadProcessId, IsWindow, GWL_STYLE, WS_CAPTION,
    };

    static WINDOW: AtomicIsize = AtomicIsize::new(0);
    static APPLIED: Mutex<Option<(isize, u32, u32, bool)>> = Mutex::new(None);

    pub(super) fn apply(caption: Color, text: Color, dark: bool) {
        let Some(hwnd) = window() else { return };
        let (caption, text) = (colorref(caption), colorref(text));
        let mut applied = APPLIED.lock().unwrap_or_else(|error| error.into_inner());
        if *applied == Some((hwnd as isize, caption, text, dark)) {
            return;
        }
        let attribute = |id: i32, value: u32| {
            // SAFETY: `hwnd` is this process's live window; each attribute takes a COLORREF or BOOL.
            unsafe {
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    id as u32,
                    &value as *const u32 as *const _,
                    size_of::<u32>() as u32,
                );
            }
        };
        // Windows 11 colors the caption; earlier builds only switch it dark or light.
        attribute(DWMWA_USE_IMMERSIVE_DARK_MODE, dark as u32);
        attribute(DWMWA_CAPTION_COLOR, caption);
        attribute(DWMWA_TEXT_COLOR, text);
        *applied = Some((hwnd as isize, caption, text, dark));
    }

    /// `COLORREF`, whose bytes are `0x00BBGGRR`.
    fn colorref(color: Color) -> u32 {
        let byte =
            |value: f32| (rune::color::srgb_encode(value.clamp(0.0, 1.0)) * 255.0).round() as u32;
        byte(color.b) << 16 | byte(color.g) << 8 | byte(color.r)
    }

    /// This process's top-level window, the one with a caption.
    fn window() -> Option<HWND> {
        let cached = WINDOW.load(Ordering::Relaxed);
        if cached != 0 {
            let hwnd = cached as HWND;
            // SAFETY: a handle found for this process before; `IsWindow` only reads it.
            if !hwnd.is_null() && unsafe { IsWindow(hwnd) } != 0 {
                return Some(hwnd);
            }
        }
        let mut found: HWND = std::ptr::null_mut();
        // SAFETY: `EnumWindows` visits this desktop's top-level windows on this thread.
        unsafe { EnumWindows(Some(find), &mut found as *mut HWND as LPARAM) };
        if found.is_null() {
            None
        } else {
            WINDOW.store(found as isize, Ordering::Relaxed);
            Some(found)
        }
    }

    unsafe extern "system" fn find(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is `window`'s out pointer, and both calls read this window's ids.
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            if pid == std::process::id() && style & WS_CAPTION == WS_CAPTION {
                *(lparam as *mut HWND) = hwnd;
                return 0;
            }
        }
        1
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2_app_kit::{
        NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication, NSColor,
    };
    use objc2_foundation::MainThreadMarker;
    use rune::Color;

    pub(super) fn apply(caption: Color, _text: Color, dark: bool) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        let name = unsafe {
            if dark {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            }
        };
        app.setAppearance(NSAppearance::appearanceNamed(name).as_deref());
        let color = unsafe {
            NSColor::colorWithSRGBRed_green_blue_alpha(
                rune::color::srgb_encode(caption.r) as f64,
                rune::color::srgb_encode(caption.g) as f64,
                rune::color::srgb_encode(caption.b) as f64,
                caption.a as f64,
            )
        };
        let windows = app.windows();
        for index in 0..windows.len() {
            let window = &windows[index];
            window.setTitlebarAppearsTransparent(true);
            window.setBackgroundColor(Some(&color));
        }
    }
}
