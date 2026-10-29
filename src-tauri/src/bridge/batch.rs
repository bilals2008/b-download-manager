//! Many links handed over in one request.
//!
//! The extension's link window collects the downloads of a page and sends the
//! ones the user ticked. They arrive in chunks, because the bridge reads one
//! small body per request; this module owns the rules for one chunk: only the
//! folders the application offered may be written to, the file name is cleaned
//! and made unique, a link that is already in the list is reported instead of
//! added twice, and every item gets its own answer.
//!
//! The batch does not open a progress window per file: twenty windows is not
//! what a hand-over should cost. The rows appear in the list as they always do.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::actions::{self, StartOptions};
use crate::bridge::dest;
use crate::bridge::{error_body, is_download_url, is_web_url, ok};
use crate::conflict::Choice;
use crate::state::AppState;

/// The most links one request carries. The extension sends the selection in
/// chunks of this size, so the answer stays small and a long list does not
/// have to fit in one body.
pub const MAX_ITEMS: usize = 25;

/// The categories the window may name. These are the application's built-in
/// categories; a batch cannot invent one, because the tree would then show a
/// category nobody configured.
const CATEGORIES: [&str; 8] = [
    "General",
    "Video",
    "Music",
    "Documents",
    "Programs",
    "Games",
    "Compressed",
    "Other",
];

/// What the link window may choose from: the folders, the queue and the
/// default connection count.
///
/// The folders are a closed list on purpose. The extension cannot browse the
/// disk, so it picks one of the places the application already uses; a path
/// that is not on this list is refused rather than trusted.
pub fn options(app: &AppHandle, state: &AppState) -> Value {
    let folders = offered_folders(
        state.browser_folder(),
        app.path().download_dir().ok(),
        last_folder(state),
    );

    json!({
        "folders": folders
            .iter()
            .map(|(label, path)| json!({ "label": label, "path": path.to_string_lossy() }))
            .collect::<Vec<Value>>(),
        "queues": [
            { "id": "now", "label": "Start now" },
            { "id": "main", "label": "Main queue" }
        ],
        "connections": state.browser_connections(),
        "maxItems": MAX_ITEMS,
    })
}

/// The one queue the application has, and the two ways a batch can use it.
///
/// There are no named queues yet (`PROGRESS.md`), so the window offers "start
/// now" and the main queue instead of a dropdown of imaginary queues.
fn last_folder(state: &AppState) -> Option<PathBuf> {
    state
        .list()
        .last()
        .and_then(|download| download.dest.parent().map(Path::to_path_buf))
}

/// The places a batch may be saved to, in the order the window shows them.
///
/// One entry per path: the configured folder and the system Downloads folder
/// are often the same place, and two identical choices would be a puzzle
/// rather than a choice.
fn offered_folders(
    configured: Option<PathBuf>,
    downloads: Option<PathBuf>,
    last: Option<PathBuf>,
) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();

    let mut offer = |label: &str, path: Option<PathBuf>| {
        let Some(path) = path else { return };
        if path.as_os_str().is_empty() {
            return;
        }
        if seen
            .iter()
            .any(|other| crate::conflict::same_path(other, &path))
        {
            return;
        }
        seen.push(path.clone());
        out.push((label.to_string(), path));
    };

    offer("App default", configured);
    offer("Downloads folder", downloads);
    offer("Last used", last);
    out
}

/// The folder a request asked for, when it is one of the offered places.
///
/// An empty answer is the first choice, which is where a hand-over has always
/// gone. Anything else has to name an offered path exactly.
fn pick_folder(offered: &[(String, PathBuf)], requested: &str) -> Option<PathBuf> {
    let requested = requested.trim();
    if requested.is_empty() {
        return offered.first().map(|(_, path)| path.clone());
    }
    let wanted = PathBuf::from(requested);
    offered
        .iter()
        .find(|(_, path)| crate::conflict::same_path(path, &wanted))
        .map(|(_, path)| path.clone())
}

/// What the window sends: the ticked links and the choices under them.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequest {
    #[serde(default)]
    pub items: Vec<BatchItem>,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub connections: Option<u32>,
    /// `now` or `main`. Anything else is refused rather than guessed.
    #[serde(default)]
    pub queue: String,
    #[serde(default)]
    pub referer: String,
}

/// One link of a batch.
#[derive(Debug, Deserialize)]
pub struct BatchItem {
    pub url: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub category: String,
}

/// What became of one link.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ItemResult {
    url: String,
    name: String,
    /// `started`, `queued`, `skipped`, `duplicate`, `invalid` or `refused`.
    status: &'static str,
    id: Option<u64>,
    dest: String,
    /// One sentence for a link that was not started.
    message: String,
}

/// Start a batch, and answer what happened to every link.
///
/// The answer is a list rather than one failure: a page of links has some that
/// work and some that do not, and the window reports each kind instead of
/// refusing the lot over one bad link.
pub fn add(app: &AppHandle, state: &AppState, body: &[u8]) -> (u16, Value) {
    let request: BatchRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return (
                400,
                error_body(
                    "bad_request",
                    format!("the body could not be read: {error}"),
                ),
            )
        }
    };

    if request.items.is_empty() {
        return (400, error_body("bad_request", "the batch holds no links"));
    }
    if request.items.len() > MAX_ITEMS {
        return (
            400,
            error_body(
                "bad_request",
                format!("a batch holds at most {MAX_ITEMS} links"),
            ),
        );
    }
    // The batch is a real download workflow, so it honours the same
    // concurrency cap a normal start does. The bridge keeps that cap current
    // before it admits the batch.
    let _ = state.browser_connections();

    let queue = match request.queue.trim() {
        "" | "now" => false,
        "main" => true,
        other => {
            return (
                400,
                error_body(
                    "bad_request",
                    format!("{other} is not a queue this build has"),
                ),
            )
        }
    };

    let offered = offered_folders(
        state.browser_folder(),
        app.path().download_dir().ok(),
        last_folder(state),
    );
    let Some(folder) = pick_folder(&offered, &request.folder) else {
        return (
            400,
            error_body(
                "bad_request",
                "that folder is not one the application offered",
            ),
        );
    };

    let referer = if is_web_url(request.referer.trim()) {
        Some(request.referer.trim().to_string())
    } else {
        None
    };
    let connections = request
        .connections
        .unwrap_or(state.browser_connections())
        .clamp(1, 32);

    let mut known: HashSet<String> = state
        .list()
        .iter()
        .map(|download| canonical(&download.url))
        .collect();

    let mut results: Vec<ItemResult> = Vec::with_capacity(request.items.len());
    for item in &request.items {
        let url = item.url.trim().to_string();
        let name = dest::sanitize(&item.name)
            .or_else(|| dest::name_from_url(&url))
            .unwrap_or_else(|| "download".to_string());

        if !is_download_url(&url) {
            results.push(ItemResult {
                url,
                name,
                status: "invalid",
                id: None,
                dest: String::new(),
                message: "Only http://, https:// and ftp:// links can be downloaded.".to_string(),
            });
            continue;
        }

        // A link the list already holds is reported, not added a second time:
        // running the window twice on one page should not double every row.
        // A second copy is still possible from the add dialog, one link at a
        // time, where the user can see what they are asking for.
        let key = canonical(&url);
        if !known.insert(key) {
            results.push(ItemResult {
                url,
                name,
                status: "duplicate",
                id: None,
                dest: String::new(),
                message: "This link is already in the download list.".to_string(),
            });
            continue;
        }

        let target = dest::unique(&folder, &name);
        let started = actions::start(
            app,
            state,
            url.clone(),
            target,
            StartOptions {
                connections: Some(connections),
                referer: referer.clone(),
                queue,
                category: clean_category(&item.category),
                // A batch has no dialog to choose a folder, so the folder its
                // category saves to decides where the file lands.
                use_category_folder: true,
                description: None,
                // A batch has no dialog to ask, so it never replaces a file:
                // it takes the next free name beside it.
                conflict: Choice::Rename,
            },
        );

        match started {
            Ok(started) => {
                let dest_name = Path::new(&started.dest)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| name.clone());
                results.push(ItemResult {
                    url,
                    name: dest_name,
                    status: if queue {
                        "queued"
                    } else if started.skipped {
                        "skipped"
                    } else {
                        "started"
                    },
                    id: started.id,
                    dest: started.dest,
                    message: String::new(),
                });
            }
            Err(refusal) => results.push(ItemResult {
                url,
                name,
                status: "refused",
                id: None,
                dest: refusal.path,
                message: refusal.message,
            }),
        }
    }

    let count = |status: &str| {
        results
            .iter()
            .filter(|result| result.status == status)
            .count()
    };
    let counts = json!({
        "started": count("started"),
        "queued": count("queued"),
        "skipped": count("skipped"),
        "duplicates": count("duplicate"),
        "invalid": count("invalid"),
        "refused": count("refused"),
    });

    if count("started") + count("queued") > 0 {
        crate::tray::show_window(app);
    }
    idm_core::log::write(
        "info",
        &format!(
            "the browser started a batch of {} link(s) in {}",
            results.len(),
            folder.display()
        ),
    );

    (
        200,
        ok(json!({
            "folder": folder.to_string_lossy(),
            "queue": if queue { "main" } else { "now" },
            "connections": connections,
            "counts": counts,
            "items": results,
        })),
    )
}

/// A category the tree knows, in its stored spelling.
///
/// Shared with the site grabber, so a batch and a grabber cannot disagree
/// about which names the tree has.
pub(crate) fn clean_category(value: &str) -> Option<String> {
    let wanted = value.trim();
    if wanted.is_empty() {
        return None;
    }
    CATEGORIES
        .iter()
        .find(|name| name.eq_ignore_ascii_case(wanted))
        .map(|name| (*name).to_string())
}

/// The same address, however it was written. The browser's parser folds the
/// parts that do not matter, so a duplicate is recognised across spellings.
fn canonical(url: &str) -> String {
    reqwest::Url::parse(url.trim())
        .map(|parsed| parsed.to_string())
        .unwrap_or_else(|_| url.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> Option<PathBuf> {
        Some(PathBuf::from(text))
    }

    #[test]
    fn the_offered_folders_are_distinct_and_ordered() {
        let offered = offered_folders(
            path("D:/Downloads"),
            path("D:/Downloads"),
            path("D:/Downloads/Films"),
        );
        assert_eq!(offered.len(), 2, "the same folder is offered once");
        assert_eq!(offered[0].0, "App default");
        assert_eq!(offered[1].0, "Last used");
        assert_eq!(offered[1].1, Path::new("D:/Downloads/Films"));
    }

    #[test]
    fn the_downloads_folder_stands_in_when_nothing_is_configured() {
        let offered = offered_folders(None, path("C:/Users/me/Downloads"), None);
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].0, "Downloads folder");
    }

    #[test]
    fn a_folder_that_was_not_offered_is_refused() {
        let offered = offered_folders(None, path("C:/Users/me/Downloads"), None);
        assert_eq!(
            pick_folder(&offered, ""),
            Some(PathBuf::from("C:/Users/me/Downloads"))
        );
        assert_eq!(
            pick_folder(&offered, " C:/Users/me/Downloads "),
            Some(PathBuf::from("C:/Users/me/Downloads"))
        );
        assert_eq!(pick_folder(&offered, "C:/Windows/System32"), None);
    }

    #[test]
    fn a_category_is_a_name_the_tree_knows() {
        assert_eq!(clean_category("video").as_deref(), Some("Video"));
        assert_eq!(
            clean_category(" Compressed ").as_deref(),
            Some("Compressed")
        );
        assert_eq!(clean_category("Podcasts"), None);
        assert_eq!(clean_category(""), None);
    }

    #[test]
    fn two_spellings_of_one_address_are_the_same_link() {
        assert_eq!(
            canonical("HTTPS://Example.ORG:443/a.bin?x=1"),
            canonical("https://example.org/a.bin?x=1")
        );
        assert_ne!(
            canonical("https://example.org/a.bin"),
            canonical("https://example.org/b.bin")
        );
        assert_eq!(canonical("not a url"), "not a url");
    }

    #[test]
    fn a_batch_body_names_its_items_and_choices() {
        let request: BatchRequest = serde_json::from_str(
            r#"{
                "items": [
                    { "url": "https://example.org/a.zip", "name": "a.zip", "category": "Compressed" },
                    { "url": "https://example.org/b.mp4", "category": "Video" }
                ],
                "folder": "C:/Users/me/Downloads",
                "connections": 4,
                "queue": "main",
                "referer": "https://example.org/page"
            }"#,
        )
        .expect("a batch body");

        assert_eq!(request.items.len(), 2);
        assert_eq!(request.items[1].name, "");
        assert_eq!(request.connections, Some(4));
        assert_eq!(request.queue, "main");
        assert!(is_web_url(&request.referer));
    }

    #[test]
    fn a_batch_body_without_choices_gets_the_defaults() {
        let request: BatchRequest =
            serde_json::from_str(r#"{"items":[{"url":"https://example.org/a"}]}"#)
                .expect("a minimal batch");
        assert_eq!(request.folder, "");
        assert_eq!(request.connections, None);
        assert_eq!(request.queue, "");
    }
}
