//! The quality panel window.
//!
//! It is a window of its own, like the progress and completion windows: a
//! manifest is parsed while the download list stays usable, and choosing a
//! quality is a small decision that does not belong in a modal over the list.
//!
//! One window serves every job. A second manifest reuses it and sends the new
//! job number through `video://job`, so opening a video twice does not stack
//! panels. The page calls `show_ready` once it has painted, so the window
//! never opens on an empty white frame.

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// The label of the window, and the address its events are sent to.
pub const WINDOW: &str = "video";

const WIDTH: f64 = 480.0;
const HEIGHT: f64 = 620.0;
const MIN_WIDTH: f64 = 360.0;
const MIN_HEIGHT: f64 = 420.0;

/// Show the panel for a parsed job, or bring it forward for a newer one.
pub fn show(app: &AppHandle, job: u64) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || show_now(&app, job));
}

fn show_now(app: &AppHandle, job: u64) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("video://job", json!({ "job": job }));
        return;
    }

    match WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("index.html".into()))
        .title("Video quality")
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        .decorations(false)
        .center()
        .visible(false)
        .initialization_script(format!("window.__BDM_VIDEO__ = {{ job: {job} }};"))
        .build()
    {
        Ok(window) => crate::window_style::round_corners(&window),
        Err(error) => idm_core::log::write(
            "warn",
            &format!("the video quality window did not open: {error}"),
        ),
    }
}
