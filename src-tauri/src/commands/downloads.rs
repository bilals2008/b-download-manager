//! Starting, stopping and inspecting one download.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};

use idm_core::downloader::{DownloadInfo, Status};
use idm_core::transfer::Source;

use crate::actions::{self, UPDATE_EVENT};
use crate::complete;
use crate::conflict;
use crate::progress;
use crate::shower;
use crate::state::AppState;
use crate::tray;

/// Everything the add dialog sends to start a download.
///
/// One object rather than a dozen positional arguments: the destination
/// question is the newest field, and the call keeps a name for each.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartRequest {
    pub url: String,
    pub dest: String,
    pub connections: Option<u32>,
    pub referer: Option<String>,
    pub queue: Option<bool>,
    pub category: Option<String>,
    /// Put the file in the folder its category saves to.
    pub use_category_folder: Option<bool>,
    pub description: Option<String>,
    /// What the File Exists dialog answered, when it was asked.
    pub conflict: Option<conflict::Choice>,
    /// The export format the reader picked for a Drive editor link, when
    /// this is one. `pdf`, `docx`, `csv`, `xlsx`, `txt`, `pptx` or `png`.
    pub drive_export: Option<String>,
}

/// Runs off the main thread on purpose: `show_progress_if_requested` hands the
/// window to the event loop, and building a webview inside the IPC handler the
/// main thread is already in would never finish.
#[tauri::command(async)]
pub(crate) fn start_download(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartRequest,
) -> Result<conflict::Started, conflict::Refusal> {
    let started = actions::start(
        &app,
        &state,
        request.url,
        PathBuf::from(request.dest.trim()),
        actions::StartOptions {
            connections: request.connections,
            referer: request.referer,
            queue: request.queue.unwrap_or(false),
            category: request.category,
            use_category_folder: request.use_category_folder.unwrap_or(false),
            description: request.description,
            conflict: request.conflict.unwrap_or_default(),
            drive_export: request.drive_export,
        },
    )?;
    show_progress_if_requested(&app, &state, &started);
    Ok(started)
}

/// Open the progress window of a start when the reader asked for it.
///
/// A skipped start has no window: there is no download behind it. The window
/// is built on the main thread, the one the window toolkit runs on, so this
/// must be called off that thread: there `run_on_main_thread` runs the build
/// inline, inside the webview's IPC handler, and the window never finishes
/// being created.
pub(crate) fn show_progress_if_requested(
    app: &AppHandle,
    state: &AppState,
    started: &conflict::Started,
) {
    if !state.show_progress_on_start() || started.skipped {
        return;
    }
    let Some(id) = started.id else {
        return;
    };
    let Some(download) = state.get(id) else {
        return;
    };
    let info = download.info();
    shower::build(app, move |app| progress::show(app, &info, false));
}

#[tauri::command]
pub(crate) fn list_downloads(state: State<'_, AppState>) -> Vec<DownloadInfo> {
    state.infos()
}

#[tauri::command]
pub(crate) fn pause_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), String> {
    actions::pause(&app, &state, id)
}

#[tauri::command]
pub(crate) fn resume_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), String> {
    actions::resume(&app, &state, id, false)
}

#[tauri::command]
pub(crate) fn cancel_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), String> {
    actions::cancel(&app, &state, id)
}

/// Start a failed download again. `restart` forgets the bytes of an attempt
/// the server cannot continue; otherwise the engine resumes from its parts.
#[tauri::command]
pub(crate) fn retry_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    restart: Option<bool>,
) -> Result<(), String> {
    actions::retry(&app, &state, id, restart.unwrap_or(false))
}

/// Start a finished or failed download again from the first byte. `conflict`
/// is the answer to the File Exists question; the row stays as it is until
/// that question is settled.
/// Runs off the main thread for the same reason as `start_download`.
#[tauri::command(async)]
pub(crate) fn redownload_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    conflict: Option<conflict::Choice>,
) -> Result<conflict::Started, conflict::Refusal> {
    let started = actions::redownload(&app, &state, id, conflict.unwrap_or_default())?;
    show_progress_if_requested(&app, &state, &started);
    Ok(started)
}

/// Move a download that is not running to another folder, parts included.
///
/// The bytes of a part file travel with it, and a copy across volumes takes a
/// while, so the command runs off the main thread.
#[tauri::command(async)]
pub(crate) async fn relocate_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    folder: String,
) -> Result<(), String> {
    actions::relocate(&app, &state, id, PathBuf::from(folder.trim())).await
}

/// Record the category and the note the properties window edited.
#[tauri::command]
pub(crate) fn set_download_meta(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    category: Option<String>,
    description: Option<String>,
) -> Result<(), String> {
    actions::set_meta(&app, &state, id, category, description)
}

/// Point a download at another file, moving the file or its parts with it.
///
/// `conflict` is the answer to the File Exists question; a taken name is not
/// replaced without one.
/// Runs off the main thread: the file can be copied across volumes, and a
/// copy of a large file takes a while.
#[tauri::command(async)]
pub(crate) async fn set_download_destination(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    dest: String,
    conflict: Option<conflict::Choice>,
) -> Result<String, conflict::Refusal> {
    actions::set_destination(
        &app,
        &state,
        id,
        PathBuf::from(dest.trim()),
        conflict.unwrap_or_default(),
    )
    .await
}

/// Delete the file of a finished download, then forget the row.
#[tauri::command]
pub(crate) fn delete_download_file(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), String> {
    actions::delete_file(&app, &state, id)
}

/// Open the progress window of one download, or bring it back to the front.
///
/// The window is built on the main thread, the one the window toolkit runs
/// on, and the download is handed to it before its first render.
#[tauri::command]
pub(crate) async fn open_progress_window(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    let info = download.info();
    let handle = app.clone();
    app.run_on_main_thread(move || progress::show(&handle, &info, true))
        .map_err(|error| error.to_string())
}

/// Fold the connection details of the progress window away, or open them.
///
/// The window sizes itself from this, so the page does not have to know the
/// height of its own chrome.
#[tauri::command]
pub(crate) fn set_progress_details(window: tauri::WebviewWindow, open: bool, animated: bool) {
    progress::set_details(&window, open, animated);
}

#[tauri::command]
pub(crate) fn remove_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), String> {
    // The download must stop before it leaves the list, or it would keep
    // writing to the disk with nothing to show for it.
    if let Some(download) = state.get(id) {
        download.cancel();
        // The parts and the manifest are the resume state of a row that will
        // not exist any more. Left behind, they sit in the download folder as
        // files nothing in the list explains.
        download.forget_progress();
    }
    state.remove(id);
    state.save_index();
    // The progress window of a row that is gone has nothing to show.
    progress::close(&app, id);
    complete::close(&app, id);
    tray::refresh(&app);
    Ok(())
}

/// Move a download to another category. The file and its folder stay put.
#[tauri::command]
pub(crate) fn set_download_category(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    category: Option<String>,
) -> Result<(), String> {
    actions::set_category(&app, &state, id, category)
}

/// Cap the bandwidth of the whole application. Zero means no cap.
#[tauri::command]
pub(crate) fn set_speed_limit(state: State<'_, AppState>, bytes_per_second: u64) -> u64 {
    state.modify_settings(|settings| {
        settings.speed_limit_enabled = bytes_per_second > 0;
        settings.speed_limit_bytes = bytes_per_second;
    });
    state.limits().global_speed.rate()
}

/// Cap the bandwidth of one download. Zero means no cap.
///
/// The limiter reads its rate on every wait, so a new cap reaches the
/// connections of the download as they stream, including one that is already
/// waiting for its next bytes.
#[tauri::command]
pub(crate) fn set_download_speed_limit(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    bytes_per_second: u64,
) -> Result<u64, String> {
    let download = state.get(id).ok_or("unknown download")?;
    // A finished file has no connection left to pace, and the update that
    // follows would open its completion window again.
    if download.status() == Status::Completed {
        return Err("this download has already finished".to_string());
    }
    // One gigabyte a second is far above any link this app will see, and the
    // cap keeps a bad value from being stored as a meaningless one.
    download.set_speed_limit(bytes_per_second.min(1024 * 1024 * 1024));
    let rate = download.speed_limit();
    let _ = app.emit(UPDATE_EVENT, download.info());
    state.save_index();
    Ok(rate)
}

/// What the server says about a link before the download starts.
#[derive(serde::Serialize)]
pub(crate) struct LinkProbe {
    /// The name the server gives the file, when it gives one.
    name: Option<String>,
    /// The size of the file. `None` means the server did not tell it.
    total: Option<u64>,
    /// Whether the server honours a range request.
    ranges: bool,
    /// Whether a stopped download of this link could continue: the server
    /// honours ranges and either knows how big the file is or gives a
    /// validator that proves the bytes on disk are still the first bytes of
    /// the file. This is the engine's own rule, so the dialog cannot promise
    /// a resume the engine would not make.
    resumable: bool,
}

/// What the server says about the link at this address.
///
/// A link like `https://example.org/get?id=12` carries no name, but the
/// answer of the server does, in its `Content-Disposition` header. The same
/// answer tells the size, out of `Content-Range` or `Content-Length`. The
/// dialog asks for both so the user sees what they are about to download and
/// does not have to invent a name.
#[tauri::command]
pub(crate) async fn probe_link(
    state: State<'_, AppState>,
    url: String,
    referer: Option<String>,
) -> Result<LinkProbe, String> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(LinkProbe {
            name: None,
            total: None,
            ranges: false,
            resumable: false,
        });
    }
    let proxy = state.limits().proxy();
    let source = Source::parse(url, proxy.as_deref(), referer.as_deref())?;
    let probe = source.probe().await.map_err(|error| error.message)?;
    let resumable = probe.ranges && (probe.total.is_some() || probe.validator().is_some());
    Ok(LinkProbe {
        name: probe.name,
        total: probe.total,
        ranges: probe.ranges,
        resumable,
    })
}

/// A sensible starting point for the destination path.
#[tauri::command]
pub(crate) fn default_destination(app: AppHandle) -> String {
    use tauri::Manager;

    let downloads = app
        .path()
        .download_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    downloads.join("download.bin").to_string_lossy().to_string()
}
