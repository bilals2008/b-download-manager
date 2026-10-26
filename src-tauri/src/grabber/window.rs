//! The Site Grabber window.
//!
//! It is a window of its own, the way the progress and completion windows are:
//! a crawl takes a while, and the download list stays usable beside it. The
//! page learns which window it is from the initialization script, before the
//! first render, so it does not have to ask.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The label of the window, and the address the events are sent to.
pub const WINDOW: &str = "grabber";

const WIDTH: f64 = 1_000.0;
const HEIGHT: f64 = 700.0;
const MIN_WIDTH: f64 = 760.0;
const MIN_HEIGHT: f64 = 540.0;

/// Show the grabber, or bring it back to the front.
///
/// Called on the main thread, where the window toolkit lives. The window is
/// hidden until its page has painted, and the page calls `show_ready` once it
/// has, so it never opens on an empty white frame.
pub fn show(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("index.html".into()))
        .title("Site Grabber")
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        .decorations(false)
        .center()
        .visible(false)
        .initialization_script("window.__IDM_GRABBER__ = true;")
        .build()
        .map(|window| crate::window_style::round_corners(&window))
        .map_err(|error| error.to_string())
}
