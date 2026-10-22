//! The window frame. The app draws its own chrome, so the only thing left for
//! the platform is the corner shape.

use tauri::WebviewWindow;

/// Ask the desktop compositor for rounded corners. Windows 11 honours it;
/// older Windows leaves the window square, which still looks right because
/// the content has its own radius.
#[cfg(windows)]
pub fn round_corners(window: &WebviewWindow) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };

    let Ok(handle) = window.hwnd() else {
        return;
    };
    let preference = DWMWCP_ROUND;
    unsafe {
        DwmSetWindowAttribute(
            handle.0 as _,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &preference as *const _ as _,
            std::mem::size_of_val(&preference) as u32,
        );
    }
}

#[cfg(not(windows))]
pub fn round_corners(_window: &WebviewWindow) {}
