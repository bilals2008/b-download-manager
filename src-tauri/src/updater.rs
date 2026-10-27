//! Checking for a new version of the application, from the release the
//! configuration names.
//!
//! The updater plugin does the trusting: it fetches `latest.json` over TLS,
//! and it verifies the minisign signature of every package against the public
//! key baked into the bundle before a byte is run. This module only carries
//! that answer to the window, and never acts on its own: a check happens when
//! the reader asks, and the install happens only after the window has shown
//! what is coming and the reader said yes.

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

use crate::state::AppState;

/// What a version check found, in the words the window can show.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// Whether there is anything newer than the running version.
    pub available: bool,
    /// The version being run now, so the window can say what it was.
    pub current_version: String,
    /// The new version, when there is one.
    pub version: Option<String>,
    /// The date the release was published, as the updater prints it.
    pub date: Option<String>,
    /// The release notes written for this version, when the release carried
    /// them.
    pub notes: Option<String>,
    /// One sentence for the window when nothing could be said.
    pub error: Option<String>,
}

/// Ask the release endpoint what the newest version is.
///
/// Reaching the endpoint is network I/O, so the window awaits this command;
/// an unreachable endpoint or a reply that fails verification is a readable
/// `error`, never a panic and never a silent success.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Found {
    let current_version = app.package_info().version.to_string();
    let fail = |error: String| Found {
        available: false,
        current_version: current_version.clone(),
        version: None,
        date: None,
        notes: None,
        error: Some(error),
    };

    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(error) => return fail(format!("the updater is not set up: {error}")),
    };
    match updater.check().await {
        Ok(Some(update)) => Found {
            available: true,
            current_version,
            version: Some(update.version),
            // The plugin's own date type, shown in the form it prints; the
            // window only ever displays it, never re-parses meaning into it.
            date: update.date.map(|date| date.to_string()),
            notes: update.body,
            error: None,
        },
        Ok(None) => Found {
            available: false,
            current_version,
            version: None,
            date: None,
            notes: None,
            error: None,
        },
        Err(error) => fail(format!("the server could not be asked: {error}")),
    }
}

/// Download the update the check found and start its installer.
///
/// The plugin verifies the package signature against the baked-in public key
/// while downloading and refuses an answer that does not match; nothing is
/// written to disk that the key did not sign. The caller is expected to have
/// shown the notes and asked the reader first.
///
/// On Windows the installer takes the process over without a normal exit, so
/// the state of this run is saved the same deliberate way the close button
/// saves it before that hand-over.
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let updater = app.updater().map_err(|error| error.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or("there is no update to install")?;

    for download in state.list() {
        if download.status() == idm_core::downloader::Status::Running {
            let _ = crate::actions::pause(&app, &state, download.id);
        }
    }
    state.save_manifests();
    state.save_index();
    state.save_preferences();

    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|error| error.to_string())
}
