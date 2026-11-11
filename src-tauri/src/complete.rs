//! The "Download complete" window.
//!
//! The reference download manager opens a window of its own when a file
//! finishes, rather than a box inside the list. The strings of the reference
//! binary call it "Download complete" and give its body as "File %s download
//! complete.\n\nDownloaded %s (%I64d
//! Bytes)\nFile saved as %s"; the option that turns it on is "Show download
//! complete dialog". Those quoted strings are the record — the report they came
//! from (`ANALYSIS.md`) was removed in `433e99e`. This window keeps the first
//! two lines and leaves the path out: Open folder and Open file reach it.
//!
//! The window is a second webview with its own title bar, so the main window
//! stays usable while it is open. The download it reports rides in as an
//! initialization script: the page has it before the first render, so there
//! is no request to wait for and no address to encode.

use idm_core::downloader::DownloadInfo;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// The label of the one completion window.
pub const WINDOW: &str = "complete";

/// The download the one window is reporting.
///
/// Several downloads can finish at the same time. A window each would mean a
/// webview each, and a webview costs about a hundred milliseconds to build on
/// the main thread: the application froze for a second and a half whenever a
/// few files finished together. The window reports the newest one instead,
/// the way the reference manager shows one completion at a time.
static SHOWING: OnceLock<Mutex<Option<u64>>> = OnceLock::new();

/// The download the window is reporting.
fn showing() -> Option<u64> {
    *SHOWING
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// The download the window reports from now on.
fn set_showing(id: u64) {
    *SHOWING
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(id);
}

/// The script that hands the finished download to the page.
fn payload(info: &DownloadInfo) -> String {
    let json = serde_json::to_string(info).unwrap_or_else(|_| "null".to_string());
    format!("window.__BDM_COMPLETE__ = {json};")
}

/// Show the window for a download that just finished.
///
/// Called on the main thread, where the window toolkit lives.
pub fn show(app: &AppHandle, info: &DownloadInfo) {
    set_showing(info.id);
    if let Some(window) = app.get_webview_window(WINDOW) {
        // The window is already open and reports another download: it takes
        // the newest one instead of a second window opening beside it.
        let _ = window.emit("complete://job", info);
        // A window that is already on screen takes the newest download
        // quietly. Bringing it to the front again for every file that
        // finishes reads as a window that will not stay closed.
        if !window.is_visible().unwrap_or(false) {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        return;
    }

    let _ = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("index.html".into()))
        .title("Download complete")
        .inner_size(500.0, 160.0)
        .resizable(false)
        .maximizable(false)
        .decorations(false)
        .center()
        // Hidden until the page has painted, so the window never opens on an
        // empty white webview. The page calls `show_ready` when it is ready.
        .visible(false)
        .initialization_script(payload(info))
        .build()
        .map(|window| crate::window_style::round_corners(&window));
}

/// Close the completion window of a download that is leaving the list.
///
/// The window would otherwise keep announcing a file the list no longer has.
/// A window that already reports another download stays open.
pub fn close(app: &AppHandle, id: u64) {
    if showing() != Some(id) {
        return;
    }
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_window_reports_the_newest_finished_download() {
        assert_eq!(WINDOW, "complete");
    }

    #[test]
    fn the_payload_is_the_state_of_the_download() {
        let info = DownloadInfo {
            id: 7,
            url: "https://example.org/a.bin".to_string(),
            dest: "C:\\Downloads\\a.bin".to_string(),
            total: 2048,
            downloaded: 2048,
            connections: 8,
            status: idm_core::downloader::Status::Completed,
            error: None,
            referer: None,
            segments: vec![],
            resumable: Some(true),
            speed_limit: 0,
            category: None,
            description: None,
        };
        let script = payload(&info);
        assert!(script.starts_with("window.__BDM_COMPLETE__ = {"));
        assert!(script.contains("\"dest\":\"C:\\\\Downloads\\\\a.bin\""));
    }
}
