//! Clipboard links.
//!
//! IDM watches the clipboard and offers a copied address as a download. The
//! application does the same, but never starts anything on its own: the link
//! opens the add dialog, where it can be checked and saved.
//!
//! The webview cannot read the clipboard while it is hidden, and the setting
//! has to work from the tray too, so the watch runs in the backend. The
//! clipboard is only read while the preference is on.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::state::AppState;

/// The event the window listens to for a copied link.
pub const CLIPBOARD_EVENT: &str = "clipboard://url";

/// How often the clipboard is checked. Long enough to stay out of the way,
/// short enough that a link appears while the user is still looking for it.
const POLL: Duration = Duration::from_millis(700);

/// The longest address a link may be. A clipboard holding a document is not a
/// download link.
const MAX_LINK: usize = 4096;

/// Start watching the clipboard. It runs until the application ends.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last = String::new();
        loop {
            tokio::time::sleep(POLL).await;

            let state = app.state::<AppState>();
            if !state.clipboard_capture() {
                // Forget the last value, so a link that is already there when
                // the watch is turned on is offered once instead of being
                // mistaken for one already seen.
                last.clear();
                continue;
            }

            let Ok(text) = app.clipboard().read_text() else {
                continue;
            };
            let Some(link) = captured_url(&text) else {
                last = text;
                continue;
            };
            if link == last {
                continue;
            }
            last = link.to_string();
            let _ = app.emit(CLIPBOARD_EVENT, link.to_string());
        }
    });
}

/// The download address in a piece of clipboard text, if it is one.
///
/// One address per copy: a note that happens to contain a link is not a
/// download, and a link with a second line is not one either.
fn captured_url(text: &str) -> Option<&str> {
    let link = text.trim();
    if link.is_empty() || link.len() > MAX_LINK || link.split_whitespace().count() != 1 {
        return None;
    }
    if link.starts_with("http://") || link.starts_with("https://") || link.starts_with("ftp://") {
        Some(link)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_on_its_own_is_a_link() {
        assert_eq!(
            captured_url("  https://example.org/film.mp4\n"),
            Some("https://example.org/film.mp4")
        );
        assert_eq!(
            captured_url("ftp://example.org/archive.zip"),
            Some("ftp://example.org/archive.zip")
        );
    }

    #[test]
    fn a_note_or_another_scheme_is_not_a_link() {
        assert_eq!(captured_url("see https://example.org/film.mp4"), None);
        assert_eq!(
            captured_url("https://example.org/a\nhttps://example.org/b"),
            None
        );
        assert_eq!(captured_url("javascript:alert(1)"), None);
        assert_eq!(captured_url("file:///C:/Windows/notepad.exe"), None);
        assert_eq!(captured_url(""), None);
    }

    #[test]
    fn a_document_is_not_a_link() {
        let long = format!("https://example.org/{}", "a".repeat(MAX_LINK));
        assert_eq!(captured_url(&long), None);
    }
}
