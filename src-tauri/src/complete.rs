//! The "Download complete" window.
//!
//! IDM opens a window of its own when a file finishes, rather than a box
//! inside the list. The strings of `IDMan.exe` call it "Download complete" and
//! give its body as "File %s download complete.\n\nDownloaded %s (%I64d
//! Bytes)\nFile saved as %s"; the option that turns it on is "Show download
//! complete dialog" (`ANALYSIS.md`, section 9.4).
//!
//! The window is a second webview with its own title bar, so the main window
//! stays usable while it is open. The download it reports rides in as an
//! initialization script: the page has it before the first render, so there
//! is no request to wait for and no address to encode.

use idm_core::downloader::DownloadInfo;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The label prefix of the completion windows.
const LABEL_PREFIX: &str = "complete";

/// The window label of one download.
pub fn label(id: u64) -> String {
    format!("{LABEL_PREFIX}-{id}")
}

/// The script that hands the finished download to the page.
fn payload(info: &DownloadInfo) -> String {
    let json = serde_json::to_string(info).unwrap_or_else(|_| "null".to_string());
    format!("window.__IDM_COMPLETE__ = {json};")
}

/// Show the window for a download that just finished.
///
/// Called on the main thread, where the window toolkit lives.
pub fn show(app: &AppHandle, info: &DownloadInfo) {
    let label = label(info.id);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let _ = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::App("index.html".into()),
    )
    .title("Download complete")
    .inner_size(470.0, 240.0)
    .resizable(false)
    .maximizable(false)
    .decorations(false)
    .center()
    .initialization_script(payload(info))
    .build();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_named_after_its_download() {
        assert_eq!(label(42), "complete-42");
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
        };
        let script = payload(&info);
        assert!(script.starts_with("window.__IDM_COMPLETE__ = {"));
        assert!(script.contains("\"dest\":\"C:\\\\Downloads\\\\a.bin\""));
    }
}
