//! BitTorrent: the app's side of it.
//!
//! A magnet link or a `.torrent` file is a job, not a download: it holds the
//! metadata, the file list, the piece map and the peers. The files are
//! written into the folder the reader chose, with the `.part` files going
//! next to them, and the order is the plain order of the pieces — a film can
//! be played while it comes in.
//!
//! Seeding after the finish is a setting, off by default, with a note about
//! upload traffic. The files are not deleted until the reader says so; when
//! the reader removes a job that is seeding, that is the choice of stopping
//! the upload, and it is theirs.

use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::json;

use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use idm_torrent::{JobRequest, JobSource, TorrentClient, TorrentJob};

use crate::state::AppState;

/// The one client of the whole app, so the request budget of a torrent is
/// one budget and not one per window.
pub struct Torrents {
    client: TorrentClient,
}

static TORRENTS: OnceLock<Torrents> = OnceLock::new();

fn torrents() -> &'static Torrents {
    TORRENTS.get_or_init(|| Torrents {
        client: TorrentClient::new(),
    })
}

/// What a torrent holds, before anything is downloaded.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentInfo {
    /// The info hash, the id of the torrent.
    info_hash: String,
    /// The name of the torrent.
    name: String,
    /// How many bytes the whole torrent holds.
    total_length: u64,
    /// The files of the torrent, in the order they come in.
    files: Vec<idm_torrent::TorrentFile>,
}

/// Read a torrent: its name and its files, before a byte is written.
///
/// A `.torrent` file answers at once. A magnet link has no file list of its
/// own, so its peers are asked for it, which is the same exchange a running
/// job makes. The dialog shows the list, and the reader decides what to take.
#[tauri::command]
pub async fn torrent_lookup(source: String) -> Result<TorrentInfo, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("there is no link to look up".to_string());
    }
    let request = if source.to_ascii_lowercase().starts_with("magnet:") {
        JobSource::Magnet(source.to_string())
    } else {
        let bytes = std::fs::read(PathBuf::from(source))
            .map_err(|error| format!("the .torrent file could not be read: {error}"))?;
        JobSource::Bytes(bytes)
    };
    let metainfo = idm_torrent::lookup(&request)
        .await
        .map_err(|error| error.to_string())?;
    Ok(TorrentInfo {
        info_hash: metainfo.info_hash,
        name: metainfo.name,
        total_length: metainfo.total_length,
        files: metainfo.files,
    })
}

/// Start a torrent: a magnet link or a `.torrent` file.
///
/// The folder the files land in is named after the torrent, under the one
/// the reader is using, the way an image gallery is named after its page.
#[tauri::command]
pub async fn torrent_start(
    app: AppHandle,
    state: State<'_, AppState>,
    source: String,
    folder: Option<String>,
    wanted: Option<Vec<usize>>,
) -> Result<TorrentJob, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("there is no link to start".to_string());
    }
    let directory = directory_for(&app, &state, source, folder.as_deref()).await?;
    let chosen = wanted.unwrap_or_default();
    let request = if source.to_ascii_lowercase().starts_with("magnet:") {
        JobRequest {
            source: JobSource::Magnet(source.to_string()),
            directory: directory.clone(),
            wanted: chosen,
        }
    } else {
        let path = PathBuf::from(source);
        let bytes = std::fs::read(&path).map_err(|error| {
            format!("the .torrent file could not be read: {error}")
        })?;
        JobRequest {
            source: JobSource::Bytes(bytes),
            directory: directory.clone(),
            wanted: chosen,
        }
    };
    let seeding = state.engine_settings().torrent_seeding;
    let id = torrents()
        .client
        .add_job(request)
        .await
        .map_err(|error| error.to_string())?;
    if seeding {
        let _ = torrents().client.set_seeding(&id, true);
    }
    show(&app, &id);
    torrents()
        .client
        .job(&id)
        .ok_or_else(|| "the job did not start".to_string())
}

/// Every job of the client.
#[tauri::command]
pub fn torrent_jobs() -> Vec<TorrentJob> {
    torrents().client.jobs()
}

/// One job.
#[tauri::command]
pub fn torrent_job(id: String) -> Result<TorrentJob, String> {
    torrents()
        .client
        .job(&id)
        .ok_or_else(|| "unknown torrent".to_string())
}

/// Stop a job where it is. The bytes that are there stay.
#[tauri::command]
pub fn torrent_pause(id: String) -> Result<(), String> {
    torrents()
        .client
        .pause(&id)
        .map_err(|error| error.to_string())
}

/// Let a job go on.
#[tauri::command]
pub fn torrent_resume(id: String) -> Result<(), String> {
    torrents()
        .client
        .resume(&id)
        .map_err(|error| error.to_string())
}

/// Turn the upload of a job on or off.
#[tauri::command]
pub fn torrent_set_seeding(id: String, seeding: bool) -> Result<(), String> {
    torrents()
        .client
        .set_seeding(&id, seeding)
        .map_err(|error| error.to_string())
}

/// Remove a job. The files go only when the reader says they should.
#[tauri::command]
pub async fn torrent_remove(id: String, delete_files: bool) -> Result<(), String> {
    torrents()
        .client
        .remove(&id, delete_files)
        .await
        .map_err(|error| error.to_string())
}

/// The folder the torrent lands in: a child of the folder the reader is
/// using, named after the torrent. A `.torrent` file whose name cannot be
/// read is refused before anything is created.
async fn directory_for(
    app: &AppHandle,
    state: &AppState,
    source: &str,
    folder: Option<&str>,
) -> Result<PathBuf, String> {
    let base = folder
        .map(str::trim)
        .filter(|folder| !folder.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            state
                .browser_folder()
                .or_else(|| app.path().download_dir().ok())
        })
        .unwrap_or_else(|| PathBuf::from("."));
    if !base.is_absolute() {
        return Err("an absolute folder is needed".to_string());
    }
    let name = if source.to_ascii_lowercase().starts_with("magnet:") {
        idm_torrent::parse_magnet(source)
            .map(|magnet| magnet.display_name())
            .unwrap_or_else(|_| "torrent".to_string())
    } else {
        let path = PathBuf::from(source);
        std::fs::read(&path)
            .ok()
            .and_then(|bytes| idm_torrent::parse_torrent(&bytes).ok())
            .map(|metainfo| metainfo.name)
            .unwrap_or_else(|| "torrent".to_string())
    };
    let name = idm_torrent::safe_path(&name).unwrap_or("torrent".to_string());
    Ok(base.join(name))
}

/// Whether a finished torrent keeps uploading.
#[tauri::command]
pub fn torrent_seeding(state: State<'_, AppState>) -> bool {
    state.engine_settings().torrent_seeding
}

/// Turn the upload of finished torrents on or off, and keep the choice.
#[tauri::command]
pub fn set_torrent_seeding(state: State<'_, AppState>, enabled: bool) -> bool {
    state.modify_settings(|settings| {
        settings.torrent_seeding = enabled;
    });
    enabled
}

/// The torrent window. One window shows every job, the way the video window
/// shows every manifest.
pub const WINDOW: &str = "torrent";

const WIDTH: f64 = 460.0;
const HEIGHT: f64 = 560.0;
const MIN_WIDTH: f64 = 340.0;
const MIN_HEIGHT: f64 = 400.0;

/// Open the torrent window, or bring it forward for a newer job.
fn show(app: &AppHandle, id: &str) {
    let app = app.clone();
    let id = id.to_string();
    let _ = app.clone().run_on_main_thread(move || show_now(&app, &id));
}

fn show_now(app: &AppHandle, id: &str) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("torrent://job", json!({ "id": id }));
        return;
    }

    match WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("index.html".into()))
        .title("Torrent")
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        .decorations(false)
        .center()
        .visible(false)
        .initialization_script(format!("window.__BDM_TORRENT__ = {{ id: \"{id}\" }};"))
        .build()
    {
        Ok(window) => crate::window_style::round_corners(&window),
        Err(error) => idm_core::log::write(
            "warn",
            &format!("the torrent window did not open: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_magnet_is_parsed() {
        let magnet = idm_torrent::parse_magnet(
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567",
        );
        assert!(magnet.is_ok());
    }
}
