use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use idm_core::log;

use crate::state::AppState;

use super::{dest, error_body, ok, DOWNLOAD_EVENT};

/// Start a download, or open the add dialog when the user asked to confirm.
///
/// A direct hand-over that finds its destination already taken — the file is
/// on disk, or another row is writing it — becomes an ask as well: the window
/// shows the file, and the engine only starts after the answer. The extension
/// has already been told the app took the link, so the browser copy is stopped
/// and the download goes through the dialog instead of the browser.
pub(super) async fn add_download(app: &AppHandle, body: &[u8]) -> (u16, Value) {
    let request: AddRequest = match serde_json::from_slice(body) {
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

    let url = request.url.trim().to_string();
    if !is_download_url(&url) {
        return (
            400,
            error_body(
                "bad_request",
                "the URL must start with http://, https:// or ftp://",
            ),
        );
    }

    let referer = if is_web_url(request.referer.trim()) {
        request.referer.trim().to_string()
    } else {
        String::new()
    };

    let name = dest::sanitize(&request.name)
        .or_else(|| dest::name_from_url(&url))
        .unwrap_or_else(|| "download".to_string());

    match request.mode {
        Mode::Ask => {
            ask_window(app, &url, &name, &referer, "the browser asked");
            (200, ok(json!({ "mode": "ask" })))
        }
        Mode::Direct => {
            let state = app.state::<AppState>();
            let folder = state
                .browser_folder()
                .or_else(|| app.path().download_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));

            // The destination is looked at before anything is started: a file
            // that is already there is a question for the window, not a name
            // to talk around with a silent rename. The plan resolves the
            // category folder for the question only; the start is given the
            // unresolved name and applies the category once itself.
            let target = match plan_direct(&state, &folder, &name) {
                DirectPlan::Taken { dest } => {
                    ask_window(
                        app,
                        &url,
                        &name,
                        &referer,
                        &format!("{dest} is already there", dest = dest.display()),
                    );
                    return (200, ok(json!({ "mode": "ask" })));
                }
                DirectPlan::Free => folder.join(&name),
            };
            let referer = if referer.is_empty() {
                None
            } else {
                Some(referer)
            };

            match crate::actions::start(
                app,
                &state,
                url.clone(),
                target.clone(),
                crate::actions::StartOptions {
                    referer,
                    // The hand-over has no dialog, so it follows the default
                    // connection count the options window pushed.
                    connections: Some(state.browser_connections()),
                    // A hand-over has no dialog to ask, so it never replaces
                    // a file: it takes the next free name beside it.
                    conflict: crate::conflict::Choice::Rename,
                    // Nor does it have a folder field: the folder the file's
                    // category saves to decides where it lands.
                    use_category_folder: true,
                    ..crate::actions::StartOptions::default()
                },
            ) {
                Ok(started) => {
                    crate::show_progress_if_requested(app, &state, &started);
                    let id = started.id;
                    log::write("info", &format!("the browser started {url} as {id:?}"));
                    (
                        200,
                        ok(json!({
                            "mode": "direct",
                            "id": id,
                            "name": Path::new(&started.dest).file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or(name),
                            "dest": started.dest,
                        })),
                    )
                }
                Err(refusal) => (400, error_body("bad_request", refusal.message)),
            }
        }
    }
}

/// Show the window and hand it the link to ask about.
///
/// The answer comes back through the window's own start command, so the File
/// Exists question — rename, replace or cancel — is settled from there.
pub(super) fn ask_window(app: &AppHandle, url: &str, name: &str, referer: &str, why: &str) {
    crate::tray::show_window(app);
    let _ = app.emit(
        DOWNLOAD_EVENT,
        json!({ "url": url, "name": name, "referer": referer }),
    );
    log::write(
        "info",
        &format!("the bridge asked the window for {url}: {why}"),
    );
}

/// What a direct hand-over may do with its destination.
pub(super) enum DirectPlan {
    /// Nothing is in the way: the engine can start at once.
    Free,
    /// A file is already there, or another row is writing that name: the
    /// window has to ask before anything starts.
    Taken { dest: PathBuf },
}

/// Where a direct hand-over would write, and whether that name is free.
///
/// The category's folder is applied first, exactly as [`crate::actions::start`]
/// applies it, so the question is about the file that would really be written
/// rather than one beside the wrong folder. An invalid destination is left for
/// the start to refuse, as before; `conflict::inspect` only answers for
/// destinations the engine can use.
pub(super) fn plan_direct(state: &AppState, folder: &Path, name: &str) -> DirectPlan {
    let (target, _) = crate::actions::category_destination(state, folder.join(name), None, true);
    let Ok(status) = crate::conflict::inspect(state, &target) else {
        return DirectPlan::Free;
    };
    if status.file_exists || status.active {
        DirectPlan::Taken { dest: target }
    } else {
        DirectPlan::Free
    }
}

/// A URL the engine may fetch. It has to name a scheme the engine speaks and a
/// non-empty host, so `http://` alone is not a download.
pub(super) fn is_download_url(url: &str) -> bool {
    is_url(url, &["http", "https", "ftp"])
}

/// A URL that is a web page, which is what a referrer may be.
pub(super) fn is_web_url(url: &str) -> bool {
    is_url(url, &["http", "https"])
}

/// The largest address the bridge accepts. A real download link is far
/// shorter; the limit stops a hand-over from carrying an unbounded string.
const MAX_URL: usize = 8 * 1024;

/// Whether `url` uses one of `schemes` and has a host to fetch.
pub(super) fn is_url(url: &str, schemes: &[&str]) -> bool {
    if url.is_empty() || url.len() > MAX_URL {
        return false;
    }
    if url
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return false;
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    if !schemes
        .iter()
        .any(|known| scheme.eq_ignore_ascii_case(known))
    {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !authority.is_empty() && !authority.starts_with(':')
}

/// What the extension asks for when it starts a download.
#[derive(Debug, Deserialize)]
pub(super) struct AddRequest {
    pub(super) url: String,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) referer: String,
    #[serde(default)]
    pub(super) mode: Mode,
}

/// How a hand-over is started: at once, or after the user confirms it in the
/// application's add dialog.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Mode {
    #[default]
    Direct,
    Ask,
}
