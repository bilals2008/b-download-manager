//! Clipboard links.
//!
//! The reference download manager watches the clipboard and offers a copied
//! address as a download. The application does the same, but never starts
//! anything on its own: the link
//! opens the add dialog, where it can be checked and saved.
//!
//! The webview cannot read the clipboard while it is hidden, and the setting
//! has to work from the tray too, so the watch runs in the backend. The
//! clipboard is only read while the preference is on.
//!
//! Which addresses count as one is the reader's list — the URL list of
//! **View > B Download Manager tray icon > Customize URL List...** — kept with
//! the other desk-side choices so a link is offered the same way before the
//! window has loaded.

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
            // A genuine exit sets the flag before the process ends; this loop
            // must not read the clipboard on the way out.
            if state.shutting_down() {
                return;
            }
            if !state.clipboard_capture() {
                // Forget the last value, so a link that is already there when
                // the watch is turned on is offered once instead of being
                // mistaken for one already seen.
                last.clear();
                continue;
            }
            let protocols = state.url_protocols();

            let Ok(text) = app.clipboard().read_text() else {
                continue;
            };
            let Some(link) = captured_url(&text, &protocols) else {
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
/// download, and a link with a second line is not one either. The protocol has
/// to be one the reader put on the list.
fn captured_url<'a>(text: &'a str, protocols: &[String]) -> Option<&'a str> {
    let link = text.trim();
    if link.is_empty() || link.len() > MAX_LINK || link.split_whitespace().count() != 1 {
        return None;
    }
    protocols
        .iter()
        .find(|protocol| link.starts_with(&format!("{protocol}://")))
        .map(|_| link)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protocols(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    const STANDARD: &[&str] = &["http", "https", "ftp"];

    #[test]
    fn an_address_on_its_own_is_a_link() {
        let list = protocols(STANDARD);
        assert_eq!(
            captured_url("  https://example.org/film.mp4\n", &list),
            Some("https://example.org/film.mp4")
        );
        assert_eq!(
            captured_url("ftp://example.org/archive.zip", &list),
            Some("ftp://example.org/archive.zip")
        );
    }

    #[test]
    fn a_note_or_another_scheme_is_not_a_link() {
        let list = protocols(STANDARD);
        assert_eq!(
            captured_url("see https://example.org/film.mp4", &list),
            None
        );
        assert_eq!(
            captured_url("https://example.org/a\nhttps://example.org/b", &list),
            None
        );
        assert_eq!(captured_url("javascript:alert(1)", &list), None);
        assert_eq!(captured_url("file:///C:/Windows/notepad.exe", &list), None);
        assert_eq!(captured_url("", &list), None);
    }

    #[test]
    fn a_protocol_the_reader_added_is_offered_and_a_dropped_one_is_not() {
        let list = protocols(&["https", "mms"]);
        assert_eq!(
            captured_url("mms://example.org/stream.asf", &list),
            Some("mms://example.org/stream.asf")
        );
        assert_eq!(captured_url("ftp://example.org/archive.zip", &list), None);
    }

    #[test]
    fn a_protocol_needs_its_slashes_before_it_counts() {
        let list = protocols(&["http"]);
        assert_eq!(captured_url("httpexample.org/a.zip", &list), None);
        assert_eq!(captured_url("https://example.org/a.zip", &list), None);
    }

    #[test]
    fn a_document_is_not_a_link() {
        let list = protocols(STANDARD);
        let long = format!("https://example.org/{}", "a".repeat(MAX_LINK));
        assert_eq!(captured_url(&long, &list), None);
    }
}
