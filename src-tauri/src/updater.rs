//! Checking for a new version of the application, from the release the
//! configuration names.
//!
//! The updater plugin does the trusting: it fetches `latest.json` over TLS,
//! and it verifies the minisign signature of every package against the public
//! key baked into the bundle before a byte is run. This module only carries
//! that answer to the window, and never acts on its own: a check happens when
//! the reader asks, and the install happens only after the window has shown
//! what is coming and the reader said yes.
//!
//! The download reports what it has actually received, not a timer dressed as
//! a download: the bytes come from the response stream, the total comes from
//! the server's `Content-Length` when it sent one, and nothing is called
//! installed until the plugin has verified and started the package.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

use crate::state::AppState;

/// How often a running download sends its byte count to the window. Chunks
/// arrive faster than a 60 Hz repaint, so reporting each one would only queue
/// work the meter cannot show.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

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

/// The step a running update is on.
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStage {
    /// Bytes are arriving from the release.
    Downloading,
    /// The package is complete and its signature is being checked.
    Verifying,
    /// The verified package has been handed to its installer.
    Installing,
}

/// One report from a running update, sent over the command's channel.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    /// The release actually being installed. A release that moved between the
    /// check and this download is named here, so the window never keeps
    /// showing a version it is not installing.
    pub version: String,
    pub stage: UpdateStage,
    /// Bytes received so far. During verification and install this is the
    /// size of the complete, verified package.
    pub downloaded: u64,
    /// The package size the server named, when it named one.
    pub total: Option<u64>,
}

/// What the window has to do once the installer has been started.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    /// Windows hands the process to the installer and restarts it from there.
    /// The other platforms replace the bundle in place, so the window offers
    /// the restart itself.
    pub restart_required: bool,
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

/// Download the update the check found, verify it, and start its installer.
///
/// The plugin verifies the package signature against the baked-in public key
/// while downloading and refuses an answer that does not match; nothing is
/// written to disk or handed to an installer that the key did not sign. The
/// caller is expected to have shown the notes and asked the reader first.
///
/// On Windows the installer takes the process over without a normal exit, so
/// the state of this run is saved the same deliberate way the close button
/// saves it before that hand-over.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, AppState>,
    on_progress: Channel<UpdateProgress>,
) -> Result<Installed, String> {
    let updater = app.updater().map_err(|error| error.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or("there is no update to install")?;

    let version = update.version.clone();
    let received = Arc::new(AtomicU64::new(0));
    let total = Arc::new(AtomicU64::new(0));
    let chunk_received = Arc::clone(&received);
    let chunk_total = Arc::clone(&total);
    let chunk_progress = on_progress.clone();
    let chunk_version = version.clone();
    let mut last_report: Option<Instant> = None;

    let download = update.download(
        move |length, announced| {
            let received =
                chunk_received.fetch_add(length as u64, Ordering::Relaxed) + length as u64;
            if let Some(announced) = announced {
                chunk_total.store(announced, Ordering::Relaxed);
            }
            let due = last_report.map_or(true, |at| at.elapsed() >= PROGRESS_INTERVAL);
            if !due {
                return;
            }
            last_report = Some(Instant::now());
            let _ = chunk_progress.send(UpdateProgress {
                version: chunk_version.clone(),
                stage: UpdateStage::Downloading,
                downloaded: received,
                total: size_of(&chunk_total),
            });
        },
        {
            let finish_received = Arc::clone(&received);
            let finish_total = Arc::clone(&total);
            let finish_progress = on_progress.clone();
            let finish_version = version.clone();
            move || {
                let _ = finish_progress.send(UpdateProgress {
                    version: finish_version,
                    stage: UpdateStage::Verifying,
                    downloaded: finish_received.load(Ordering::Relaxed),
                    total: size_of(&finish_total),
                });
            }
        },
    );

    let bytes = download.await.map_err(|error| error.to_string())?;
    let size = size_of(&total).unwrap_or(bytes.len() as u64);

    // The installer needs the saved list and preferences to be complete before
    // it closes the application; a download that was running is paused and
    // saved the same way the close button saves it.
    for download in state.list() {
        if download.status() == idm_core::downloader::Status::Running {
            let _ = crate::actions::pause(&app, &state, download.id);
        }
    }
    state.save_manifests();
    state.save_index();
    state.save_preferences();

    let _ = on_progress.send(UpdateProgress {
        version,
        stage: UpdateStage::Installing,
        downloaded: size,
        total: Some(size),
    });
    update.install(&bytes).map_err(|error| error.to_string())?;

    Ok(Installed {
        restart_required: !cfg!(windows),
    })
}

/// Restart the application into the version the installer placed.
///
/// Windows restarts from the installer itself; this command is what the other
/// platforms call once the window has said the new version is ready.
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

/// The server's total, with the zero atomics use for "it did not say".
fn size_of(total: &AtomicU64) -> Option<u64> {
    match total.load(Ordering::Relaxed) {
        0 => None,
        value => Some(value),
    }
}
