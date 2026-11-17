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
//!
//! A `.torrent` is a small file and travels like one: the reader can hand over
//! a path, or an address the file sits at, and both arrive here as bytes.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use idm_torrent::{JobRequest, JobSource, TorrentClient, TorrentJob};

use crate::settings_store::EngineSettings;
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

/// The largest `.torrent` this application will take off the network. The
/// file lists a torrent carries are kilobytes for a whole series; ten
/// megabytes is already far past any real one, so a response bigger than
/// that is not a torrent and must not be read into memory.
const MAX_TORRENT_BYTES: usize = 10 * 1024 * 1024;

/// How long a `.torrent` server gets to answer and to send the file. A
/// torrent is small, so a host that cannot produce it in half a minute is
/// not going to.
const TORRENT_TIMEOUT: Duration = Duration::from_secs(30);

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
    /// The trackers the torrent names, so the add dialog can say where the
    /// peers will be looked for before a byte is written.
    trackers: Vec<String>,
}

/// The choices the reader makes about torrents, as the torrent window shows
/// and edits them.
///
/// A zero is "no limit" for every one of them: no upload cap, no ratio that
/// stops the sharing, no number of minutes that ends it. That is the default
/// for the sharing, because uploading after the file is complete is traffic
/// the reader pays for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentSettings {
    /// Whether a finished torrent keeps uploading.
    pub seeding: bool,
    /// The upload limit in kilobytes a second; zero is no limit.
    pub upload_limit_kbps: u64,
    /// The uploaded over downloaded ratio a finished torrent stops at; zero
    /// is never.
    pub seed_ratio: f64,
    /// The minutes a finished torrent keeps uploading; zero is never.
    pub seed_minutes: u64,
    /// The port the trackers are told about. Zero is the engine's own while
    /// the engine takes no connection in, which is what it does today.
    pub port: u16,
    /// Extra nodes the distributed table starts from, as `host:port` texts.
    /// The standard ones are always used as well.
    #[serde(default)]
    pub dht_nodes: Vec<String>,
    /// Whether torrents are announced on the local network and the peers
    /// that answer are taken.
    #[serde(default = "yes")]
    pub lpd: bool,
    /// Whether the router is asked to open the torrent port.
    #[serde(default)]
    pub port_mapping: bool,
}

/// The default of a switch that ships on.
fn yes() -> bool {
    true
}

/// Read a torrent: its name and its files, before a byte is written.
///
/// A `.torrent` file answers at once. A magnet link has no file list of its
/// own, so its peers are asked for it, which is the same exchange a running
/// job makes. An address is fetched first and read like a file. The dialog
/// shows the list, and the reader decides what to take.
#[tauri::command]
pub async fn torrent_lookup(
    state: State<'_, AppState>,
    source: String,
) -> Result<TorrentInfo, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("there is no link to look up".to_string());
    }
    let settings = state.engine_settings();
    let request = if is_magnet(source) {
        JobSource::Magnet(source.to_string())
    } else {
        JobSource::Bytes(torrent_file(source, settings.proxy_url.as_str()).await?)
    };
    let metainfo = idm_torrent::lookup_with(&request, &options_of(&settings))
        .await
        .map_err(|error| error.to_string())?;
    Ok(TorrentInfo {
        info_hash: metainfo.info_hash,
        name: metainfo.name,
        total_length: metainfo.total_length,
        files: metainfo.files,
        trackers: metainfo.trackers,
    })
}

/// Start a torrent: a magnet link, a `.torrent` file path, or the address a
/// `.torrent` file sits at.
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
    start_paused: Option<bool>,
    sequential: Option<bool>,
) -> Result<TorrentJob, String> {
    let source = source.trim();
    if source.is_empty() {
        return Err("there is no link to start".to_string());
    }
    // The bytes are read once: the folder is named after the torrent and the
    // job is the same bytes, so an address is not fetched twice.
    let bytes = if is_magnet(source) {
        None
    } else {
        Some(torrent_file(source, state.engine_settings().proxy_url.as_str()).await?)
    };
    let directory =
        directory_for(&app, &state, source, folder.as_deref(), bytes.as_deref()).await?;
    let chosen = wanted.unwrap_or_default();
    let start_paused = start_paused.unwrap_or(false);
    let sequential = sequential.unwrap_or(false);
    let request = match bytes {
        Some(bytes) => JobRequest {
            source: JobSource::Bytes(bytes),
            directory: directory.clone(),
            wanted: chosen,
            start_paused,
            sequential,
        },
        None => JobRequest {
            source: JobSource::Magnet(source.to_string()),
            directory: directory.clone(),
            wanted: chosen,
            start_paused,
            sequential,
        },
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
    show(&app, Some(id.as_str()));
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

/// Stop every job where it is. One call for the whole window, so a pause that
/// means all of them is one click and not one per row.
#[tauri::command]
pub fn torrent_pause_all() {
    torrents().client.pause_all();
}

/// Let every job go on.
#[tauri::command]
pub fn torrent_resume_all() {
    torrents().client.resume_all();
}

/// Turn the upload of a job on or off.
#[tauri::command]
pub fn torrent_set_seeding(id: String, seeding: bool) -> Result<(), String> {
    torrents()
        .client
        .set_seeding(&id, seeding)
        .map_err(|error| error.to_string())
}

/// Check the files on disk against the torrent again.
///
/// The pieces that are there are found by hashing them, so a file the reader
/// edited, or one that was moved in by hand, is either taken as it is or
/// downloaded again — never trusted because it has the right name.
#[tauri::command]
pub fn torrent_recheck(id: String) -> Result<(), String> {
    torrents()
        .client
        .recheck(&id)
        .map_err(|error| error.to_string())
}

/// Change the files a job takes. A file that is already whole keeps its bytes;
/// a file that is dropped is not deleted, it is only not written to any more.
#[tauri::command]
pub fn torrent_set_files(id: String, wanted: Vec<usize>) -> Result<(), String> {
    torrents()
        .client
        .set_files(&id, wanted)
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
/// using, named after the torrent. A `.torrent` whose name cannot be read is
/// refused before anything is created.
async fn directory_for(
    app: &AppHandle,
    state: &AppState,
    source: &str,
    folder: Option<&str>,
    bytes: Option<&[u8]>,
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
    let name = match bytes {
        Some(bytes) => idm_torrent::parse_torrent(bytes)
            .map(|metainfo| metainfo.name)
            .unwrap_or_else(|_| "torrent".to_string()),
        None => idm_torrent::parse_magnet(source)
            .map(|magnet| magnet.display_name())
            .unwrap_or_else(|_| "torrent".to_string()),
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

/// The torrent choices, as the window reads them.
#[tauri::command]
pub fn torrent_settings(state: State<'_, AppState>) -> TorrentSettings {
    saved_settings(&state.engine_settings())
}

/// Keep the torrent choices. The saved file is cleaned first, so the window is
/// answered with what was actually kept rather than with what it sent.
#[tauri::command]
pub fn set_torrent_settings(
    state: State<'_, AppState>,
    settings: TorrentSettings,
) -> TorrentSettings {
    state.modify_settings(|saved| {
        saved.torrent_seeding = settings.seeding;
        saved.torrent_upload_kbps = settings.upload_limit_kbps;
        saved.torrent_seed_ratio = settings.seed_ratio;
        saved.torrent_seed_minutes = settings.seed_minutes;
        saved.torrent_port = settings.port;
        saved.torrent_dht_nodes = settings.dht_nodes;
        saved.torrent_lpd = settings.lpd;
        saved.torrent_port_mapping = settings.port_mapping;
    });
    let saved = state.engine_settings();
    // The file is written by modify_settings; the jobs that are running are
    // told the new limits here, so a change is felt without a restart.
    apply_limits(&saved);
    saved_settings(&saved)
}

/// Hand the saved torrent limits to the client: the upload cap, the ratio a
/// finished job stops at and the minutes it keeps sharing. The bootstrap
/// nodes go to the distributed table, which a torrent no tracker answers
/// for finds its peers through.
///
/// Called once at start-up, before any job runs, and again whenever the reader
/// saves them. The port is kept by the file for the engine's fixed announce
/// port to use when the engine listens on one; the table listens on that same
/// port for its UDP questions, so the mapping the router made is the one the
/// table is reached on.
pub fn apply_limits(settings: &EngineSettings) {
    torrents().client.set_limits(
        settings.torrent_upload_kbps.saturating_mul(1024),
        settings.torrent_seed_ratio,
        settings.torrent_seed_minutes,
    );
    idm_torrent::set_nodes(settings.torrent_dht_nodes.clone());
    // The options start sockets and tasks, which need the async runtime;
    // the settings save itself is synchronous.
    let options = options_of(settings);
    tauri::async_runtime::spawn(async move {
        torrents().client.set_options(options);
    });
}

/// The reader's saved choices as the torrent client runs under them.
fn options_of(settings: &EngineSettings) -> idm_torrent::TorrentOptions {
    idm_torrent::TorrentOptions {
        proxy: (!settings.proxy_url.is_empty()).then(|| settings.proxy_url.clone()),
        lpd: settings.torrent_lpd,
        port_mapping: settings.torrent_port_mapping,
        port: settings.torrent_port,
    }
}

/// What the router's port mapping is doing, as the settings page reads it.
#[tauri::command]
pub fn torrent_port_mapping() -> idm_torrent::PortMappingStatus {
    torrents().client.port_mapping_status()
}

/// Give the router's mapping back and leave the local network's group.
/// Called when the application closes; the mapping's lease would expire on
/// its own, but giving it back is the polite way.
pub fn shutdown() {
    torrents().client.shutdown();
}

/// The saved choices as the window reads them.
fn saved_settings(settings: &EngineSettings) -> TorrentSettings {
    TorrentSettings {
        seeding: settings.torrent_seeding,
        upload_limit_kbps: settings.torrent_upload_kbps,
        seed_ratio: settings.torrent_seed_ratio,
        seed_minutes: settings.torrent_seed_minutes,
        port: settings.torrent_port,
        dht_nodes: settings.torrent_dht_nodes.clone(),
        lpd: settings.torrent_lpd,
        port_mapping: settings.torrent_port_mapping,
    }
}

/// Whether the source is a magnet link rather than a file or an address.
fn is_magnet(source: &str) -> bool {
    source.to_ascii_lowercase().starts_with("magnet:")
}

/// Whether the source is an address a `.torrent` can be fetched from.
fn is_address(source: &str) -> bool {
    let source = source.to_ascii_lowercase();
    source.starts_with("http://") || source.starts_with("https://")
}

/// The bytes of a `.torrent`: read from the disk, or fetched from the address
/// the reader pasted, through the proxy when one is set. Both are the same
/// thing to what follows.
async fn torrent_file(source: &str, proxy: &str) -> Result<Vec<u8>, String> {
    if is_address(source) {
        return fetch_torrent(source, proxy).await;
    }
    let path = PathBuf::from(source);
    std::fs::read(&path).map_err(|error| format!("the .torrent file could not be read: {error}"))
}

/// Fetch a `.torrent` from an address.
///
/// The body is read in pieces and counted as it goes: a server is not
/// believed about its length, so a response that keeps sending is cut off at
/// the cap rather than filling memory. A page is refused by its type, because
/// a link that answered with a login form should say so plainly instead of
/// failing later as broken bencode.
async fn fetch_torrent(source: &str, proxy: &str) -> Result<Vec<u8>, String> {
    let mut builder = reqwest::Client::builder().timeout(TORRENT_TIMEOUT);
    if !proxy.is_empty() {
        let proxy = reqwest::Proxy::all(proxy)
            .map_err(|error| format!("the proxy is not usable: {error}"))?;
        builder = builder.proxy(proxy);
    }
    let client = builder
        .build()
        .map_err(|error| format!("the .torrent address could not be opened: {error}"))?;
    let mut response = client
        .get(source)
        .send()
        .await
        .map_err(|error| format!("the .torrent address could not be reached: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "the .torrent address answered {}",
            response.status().as_u16()
        ));
    }
    let kind = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if kind.contains("text/html") {
        return Err("the address served a page, not a .torrent file".to_string());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("the .torrent file could not be read: {error}"))?
    {
        if bytes.len() + chunk.len() > MAX_TORRENT_BYTES {
            return Err("the .torrent at that address is too large to be a torrent".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        return Err("the .torrent at that address is empty".to_string());
    }
    Ok(bytes)
}

/// The torrent window. One window shows every job, the way the video window
/// shows every manifest.
pub const WINDOW: &str = "torrent";

/// The window opens wide: the left pane lists the jobs and the right one shows
/// the files, the peers and the trackers of the job that is chosen, which is
/// two panes side by side rather than a column.
const WIDTH: f64 = 1040.0;
const HEIGHT: f64 = 700.0;
const MIN_WIDTH: f64 = 760.0;
const MIN_HEIGHT: f64 = 520.0;

/// Open the torrent window, or bring it forward. The job that was just added
/// is named, so the window can show it first; a window that is only being
/// raised names none.
fn show(app: &AppHandle, id: Option<&str>) {
    let app = app.clone();
    let id = id.map(str::to_string);
    let _ = app
        .clone()
        .run_on_main_thread(move || show_now(&app, id.as_deref()));
}

fn show_now(app: &AppHandle, id: Option<&str>) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        if let Some(id) = id {
            let _ = window.emit("torrent://job", json!({ "id": id }));
        }
        return;
    }

    let mut builder = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("index.html".into()))
        .title("Torrents")
        .inner_size(WIDTH, HEIGHT)
        .min_inner_size(MIN_WIDTH, MIN_HEIGHT)
        .decorations(false)
        .center()
        .visible(false);
    // The marker says what this window is before the page loads. It is set
    // whether or not a job is named, because a window without it renders the
    // main window again and never becomes visible.
    let marker = match id {
        Some(id) => format!("window.__BDM_TORRENT__ = {{ id: \"{id}\" }};"),
        None => "window.__BDM_TORRENT__ = {};".to_string(),
    };
    builder = builder.initialization_script(marker);
    match builder.build() {
        Ok(window) => crate::window_style::round_corners(&window),
        Err(error) => {
            idm_core::log::write("warn", &format!("the torrent window did not open: {error}"))
        }
    }
}

/// Open the torrent window, or bring the one that is there forward. There is
/// one window for every job, so this never opens a second one.
///
/// Async on purpose: the window is built on the main thread, the one the
/// window toolkit runs on, and a synchronous command runs on that same
/// thread — there `run_on_main_thread` runs the build inline, inside the
/// webview's IPC handler, and WebView2 never finishes creating the window.
#[tauri::command(async)]
pub fn show_torrent_window(app: AppHandle) {
    show(&app, None);
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

    #[test]
    fn an_address_is_told_apart_from_a_path() {
        assert!(is_address("https://example.org/a.torrent"));
        // A pasted link can arrive in capitals; the scheme is not case-blind.
        assert!(is_address("HTTP://example.org/a.torrent"));
        assert!(!is_address("C:\\Downloads\\a.torrent"));
        assert!(!is_address("magnet:?xt=urn:btih:0123"));
        assert!(is_magnet("MAGNET:?xt=urn:btih:0123"));
    }

    #[test]
    fn the_saved_choices_are_read_back_as_the_window_sends_them() {
        let settings = EngineSettings {
            torrent_seeding: true,
            torrent_upload_kbps: 512,
            torrent_seed_ratio: 2.5,
            torrent_seed_minutes: 90,
            torrent_port: 6881,
            torrent_dht_nodes: vec!["dht.example.org:6881".into()],
            torrent_lpd: false,
            torrent_port_mapping: true,
            ..EngineSettings::default()
        };
        assert_eq!(
            saved_settings(&settings),
            TorrentSettings {
                seeding: true,
                upload_limit_kbps: 512,
                seed_ratio: 2.5,
                seed_minutes: 90,
                port: 6881,
                dht_nodes: vec!["dht.example.org:6881".into()],
                lpd: false,
                port_mapping: true,
            }
        );
    }
}
