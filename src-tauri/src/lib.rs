mod actions;
mod bridge;
mod clipboard;
mod complete;
mod conflict;
mod dest;
mod grabber;
mod lifecycle;
mod prefs;
mod progress;
mod state;
mod tray;
mod updater;
mod window_style;

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

use idm_core::downloader::{DownloadInfo, Status};
use idm_core::session::Index;
use idm_core::transfer::Source;
use prefs::DesktopPreferences;
use state::{AppState, BrowserPolicy};

use actions::UPDATE_EVENT;

/// Everything the add dialog sends to start a download.
///
/// One object rather than a dozen positional arguments: the destination
/// question is the newest field, and the call keeps a name for each.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartRequest {
    url: String,
    dest: String,
    connections: Option<u32>,
    referer: Option<String>,
    queue: Option<bool>,
    category: Option<String>,
    /// Put the file in the folder its category saves to.
    use_category_folder: Option<bool>,
    description: Option<String>,
    /// What the File Exists dialog answered, when it was asked.
    conflict: Option<conflict::Choice>,
}

#[tauri::command]
fn start_download(
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
        },
    )?;
    show_progress_if_requested(&app, &state, &started);
    Ok(started)
}

/// Open the progress window of a start when the reader asked for it.
///
/// A skipped start has no window: there is no download behind it. The window
/// is built on the main thread, the one the window toolkit runs on.
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
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || progress::show(&handle, &info));
}

#[tauri::command]
fn list_downloads(state: State<'_, AppState>) -> Vec<DownloadInfo> {
    state.infos()
}

#[tauri::command]
fn pause_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    actions::pause(&app, &state, id)
}

#[tauri::command]
fn resume_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    actions::resume(&app, &state, id)
}

#[tauri::command]
fn cancel_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    actions::cancel(&app, &state, id)
}

/// Start a failed download again. `restart` forgets the bytes of an attempt
/// the server cannot continue; otherwise the engine resumes from its parts.
#[tauri::command]
fn retry_download(
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
#[tauri::command]
fn redownload_download(
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
#[tauri::command]
fn relocate_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    folder: String,
) -> Result<(), String> {
    actions::relocate(&app, &state, id, PathBuf::from(folder.trim()))
}

/// Record the category and the note the properties window edited.
#[tauri::command]
fn set_download_meta(
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
#[tauri::command]
fn set_download_destination(
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
}

/// Park a download in the queue, keeping the bytes it already received.
#[tauri::command]
fn queue_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    actions::queue(&app, &state, id)
}

/// Take a download out of the queue and leave it paused.
#[tauri::command]
fn dequeue_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    actions::dequeue(&app, &state, id)
}

/// Delete the file of a finished download, then forget the row.
#[tauri::command]
fn delete_download_file(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    actions::delete_file(&app, &state, id)
}

/// Open the progress window of one download, or bring it back to the front.
///
/// The window is built on the main thread, the one the window toolkit runs
/// on, and the download is handed to it before its first render.
#[tauri::command]
async fn open_progress_window(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    let info = download.info();
    let handle = app.clone();
    app.run_on_main_thread(move || progress::show(&handle, &info))
        .map_err(|error| error.to_string())
}

/// Fold the connection details of the progress window away, or open them.
///
/// The window sizes itself from this, so the page does not have to know the
/// height of its own chrome.
#[tauri::command]
fn set_progress_details(window: tauri::WebviewWindow, open: bool, animated: bool) {
    progress::set_details(&window, open, animated);
}

#[tauri::command]
fn remove_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
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
fn set_download_category(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    category: Option<String>,
) -> Result<(), String> {
    actions::set_category(&app, &state, id, category)
}

/// Cap the bandwidth of the whole application. Zero means no cap.
#[tauri::command]
fn set_speed_limit(state: State<'_, AppState>, bytes_per_second: u64) -> u64 {
    state.limits().global_speed.set_rate(bytes_per_second);
    state.limits().global_speed.rate()
}

/// Cap the bandwidth of one download. Zero means no cap.
///
/// The limiter reads its rate on every wait, so a new cap reaches the
/// connections of the download as they stream, including one that is already
/// waiting for its next bytes.
#[tauri::command]
fn set_download_speed_limit(
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

/// The proxy that new downloads go through. `None` means no proxy.
#[tauri::command]
fn set_proxy(state: State<'_, AppState>, url: Option<String>) -> Result<(), String> {
    let cleaned = match url {
        Some(value) => validate_proxy(value)?,
        None => String::new(),
    };
    state.limits().set_proxy(if cleaned.is_empty() {
        None
    } else {
        Some(&cleaned)
    });
    Ok(())
}

/// Check a proxy address before it is stored or used.
///
/// Only the schemes the engine can actually dial are accepted, the port has to
/// make sense, and a username or password is refused: the preference lives in
/// plain local storage, so it must never carry a credential. Proxy
/// authentication is not part of this build.
#[tauri::command]
fn validate_proxy(url: String) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|_| "Use a full address, for example socks5://127.0.0.1:1080.".to_string())?;
    let scheme = parsed.scheme();
    if !matches!(scheme, "http" | "https" | "socks5" | "socks5h") {
        return Err(format!(
            "{scheme} is not a proxy this build can use. Use http, https, socks5 or socks5h."
        ));
    }
    if parsed.host_str().is_none() {
        return Err("The proxy address has no host.".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(
            "Proxy authentication is not supported. Remove the username and password.".to_string(),
        );
    }
    // The engine builds its client from this exact string, so refuse what it
    // would refuse instead of storing an address that fails at the first
    // download.
    Source::parse("https://example.com/", Some(trimmed), None)
        .map_err(|error| format!("The proxy address is not usable: {error}"))?;
    Ok(trimmed.to_string())
}

/// The number of connections one site may use.
#[tauri::command]
fn set_site_connections(state: State<'_, AppState>, host: String, connections: u32) {
    state.limits().sites.set(&host, connections);
}

/// The number of connections one site may use, when there is an override.
#[tauri::command]
fn site_connections(state: State<'_, AppState>, host: String) -> Option<u32> {
    state.limits().sites.get(&host)
}

/// Leave the application for good, the way the Exit item of the menu does.
///
/// Closing the window only hides it into the tray, so the menu needs a way
/// out. The one shutdown path pauses the running transfers and writes their
/// resumes before the process ends.
#[tauri::command]
fn quit_app(app: AppHandle) {
    lifecycle::shutdown(&app);
}

/// How many connections the whole application may have open at once.
#[tauri::command]
fn set_max_connections(state: State<'_, AppState>, max: u32) -> u32 {
    state.limits().budget.resize(max);
    max
}

/// The choices the tray and the close button follow.
#[tauri::command]
fn desktop_preferences(state: State<'_, AppState>) -> DesktopPreferences {
    state.preferences()
}

/// Whether the minimize button hides the window into the tray.
#[tauri::command]
fn set_minimize_to_tray(state: State<'_, AppState>, on: bool) -> bool {
    let mut preferences = state.preferences();
    preferences.minimize_to_tray = on;
    state.set_preferences(preferences);
    state.save_preferences();
    on
}

/// Whether closing the window keeps the application in the tray.
#[tauri::command]
fn set_close_to_tray(state: State<'_, AppState>, on: bool) -> bool {
    let mut preferences = state.preferences();
    preferences.close_to_tray = on;
    state.set_preferences(preferences);
    state.save_preferences();
    on
}

/// Whether a finished download gets its own window.
#[tauri::command]
fn set_show_complete_dialog(state: State<'_, AppState>, on: bool) -> bool {
    state.set_show_complete_dialog(on);
    on
}

/// Whether a new download opens its progress window.
#[tauri::command]
fn set_show_progress_on_start(state: State<'_, AppState>, on: bool) -> bool {
    state.set_show_progress_on_start(on);
    on
}

/// The connections a browser hand-over gets, since it has no dialog to ask.
#[tauri::command]
fn set_browser_connections(state: State<'_, AppState>, count: u32) -> u32 {
    state.set_browser_connections(count);
    state.browser_connections()
}

/// Whether the clipboard is watched for download links.
#[tauri::command]
fn set_clipboard_capture(state: State<'_, AppState>, on: bool) -> bool {
    state.set_clipboard_capture(on);
    on
}

/// Register or remove the operating system's startup entry.
#[tauri::command]
fn set_launch_on_startup(app: AppHandle, on: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();
    if on {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|error| error.to_string())?;
    manager.is_enabled().map_err(|error| error.to_string())
}

/// The capture rules the extension follows while this application runs.
///
/// The lists are cleaned here as well as in the window: this is the value the
/// extension acts on, so it is the boundary that has to be safe.
#[tauri::command]
fn set_browser_policy(state: State<'_, AppState>, policy: BrowserPolicy) -> BrowserPolicy {
    let policy = normalize_policy(policy);
    state.set_browser_policy(policy.clone());
    policy
}

/// The appearance of the floating video download button the extension draws.
#[tauri::command]
fn set_video_button_settings(state: State<'_, AppState>, settings: state::VideoButtonSettings) {
    state.set_video_button_settings(settings);
}

/// The appearance of the floating video download button, served to the
/// extension over the loopback bridge.
#[tauri::command]
fn video_button_settings(state: State<'_, AppState>) -> state::VideoButtonSettings {
    state.video_button_settings()
}

/// What the window can say about the browser bridge without guessing.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserStatus {
    /// Whether the bridge bound its port.
    bridge_running: bool,
    port: u16,
    api: u32,
    /// The last authenticated extension request, in milliseconds since the
    /// Unix epoch. `None` means the extension has not reached it yet.
    last_contact_ms: Option<u64>,
}

#[tauri::command]
fn browser_status(state: State<'_, AppState>) -> BrowserStatus {
    BrowserStatus {
        bridge_running: state.bridge_running(),
        port: bridge::PORT,
        api: bridge::API_VERSION,
        last_contact_ms: state.last_extension_contact().map(|seconds| seconds * 1000),
    }
}

/// Whether a folder can hold a download, and how much room is left.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderStatus {
    /// The folder that was checked. Empty input means the system Downloads
    /// folder, and this is the folder it resolved to.
    path: String,
    exists: bool,
    writable: bool,
    free_bytes: Option<u64>,
    /// One sentence for the window when the folder cannot be used.
    error: Option<String>,
}

#[tauri::command]
fn folder_status(app: AppHandle, folder: String) -> FolderStatus {
    let trimmed = folder.trim();
    let path = if trimmed.is_empty() {
        app.path()
            .download_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(trimmed)
    };
    let shown = path.to_string_lossy().to_string();

    if !path.exists() {
        return FolderStatus {
            path: shown,
            exists: false,
            writable: false,
            free_bytes: None,
            error: Some(
                "This folder is not there yet. It is created when the first download starts."
                    .to_string(),
            ),
        };
    }
    if !path.is_dir() {
        return FolderStatus {
            path: shown,
            exists: true,
            writable: false,
            free_bytes: None,
            error: Some("This path is a file, not a folder.".to_string()),
        };
    }

    let writable = can_write(&path);
    FolderStatus {
        free_bytes: free_space(&path),
        error: if writable {
            None
        } else {
            Some("This folder cannot be written to. Choose another one.".to_string())
        },
        path: shown,
        exists: true,
        writable,
    }
}

/// Where the log and the saved download index live.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppPaths {
    data_dir: String,
    log_path: String,
}

#[tauri::command]
fn app_paths(app: AppHandle) -> AppPaths {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    AppPaths {
        log_path: dir.join("idm-clone.log").to_string_lossy().to_string(),
        data_dir: dir.to_string_lossy().to_string(),
    }
}

/// The longest list one policy carries, matching the window's limit.
const MAX_RULES: usize = 64;

fn normalize_policy(mut policy: BrowserPolicy) -> BrowserPolicy {
    policy.types = clean_list(policy.types, clean_file_type);
    policy.skip_sites = clean_list(policy.skip_sites, clean_site);
    policy
}

fn clean_list(values: Vec<String>, clean: fn(&str) -> Option<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in values {
        let Some(cleaned) = clean(&value) else {
            continue;
        };
        if !out.contains(&cleaned) {
            out.push(cleaned);
        }
        if out.len() >= MAX_RULES {
            break;
        }
    }
    out
}

fn clean_file_type(value: &str) -> Option<String> {
    let cleaned = value
        .trim()
        .to_ascii_lowercase()
        .trim_start_matches('*')
        .trim_start_matches('.')
        .to_string();
    if cleaned.is_empty() || cleaned.len() > 16 {
        return None;
    }
    let mut chars = cleaned.chars();
    if !chars.next()?.is_ascii_alphanumeric() {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '_' | '-')) {
        return None;
    }
    Some(cleaned)
}

/// A host out of anything the reader may have typed, without a scheme or a
/// path. Junk becomes `None`.
fn clean_site(value: &str) -> Option<String> {
    let text = value.trim().trim_start_matches('*').trim_start_matches('.');
    if text.is_empty() {
        return None;
    }
    let with_scheme = if text.contains("://") {
        text.to_string()
    } else {
        format!("http://{text}")
    };
    let url = reqwest::Url::parse(&with_scheme).ok()?;
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" {
        return Some(host);
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return None;
    }
    let valid = labels.iter().all(|label| {
        !label.is_empty()
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    });
    valid.then_some(host)
}

/// Whether a file can be created in the folder. The test file is removed
/// again; a folder that refuses it cannot hold a download either.
fn can_write(folder: &std::path::Path) -> bool {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let probe = folder.join(format!(".bdm-write-{}-{stamp}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// The room left on the volume of a folder, when the system can tell.
#[cfg(target_os = "windows")]
fn free_space(folder: &std::path::Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = folder
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free = 0u64;
    let mut total = 0u64;
    let mut total_free = 0u64;
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, &mut total_free) };
    (ok != 0).then_some(free)
}

#[cfg(not(target_os = "windows"))]
fn free_space(_folder: &std::path::Path) -> Option<u64> {
    None
}

/// The folder a download handed over by the browser is saved to.
///
/// The options window owns this preference and pushes it here, so a link
/// caught by the extension lands where the user expects. An empty folder
/// means the system Downloads folder.
#[tauri::command]
fn set_browser_folder(state: State<'_, AppState>, folder: String) -> String {
    let trimmed = folder.trim();
    let folder = if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    };
    state.set_browser_folder(folder);
    trimmed.to_string()
}

/// The unfinished folder and the folder each category saves to.
///
/// The options window owns both lists. The backend keeps the resolved answer,
/// because a browser hand-over has no window to read and a start needs the work
/// folder before it creates its first part. An empty unfinished folder means the
/// application's own data folder, so a fresh install still keeps the parts away
/// from the finished files.
#[tauri::command]
fn set_download_folders(
    app: AppHandle,
    state: State<'_, AppState>,
    unfinished_folder: String,
    categories: Vec<state::CategoryRule>,
) -> Result<String, String> {
    let trimmed = unfinished_folder.trim();
    let folder = if trimmed.is_empty() {
        app.path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("unfinished")
    } else {
        PathBuf::from(trimmed)
    };
    if folder.exists() && !folder.is_dir() {
        return Err(format!("{} is a file, not a folder.", folder.display()));
    }
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("the unfinished folder could not be created: {error}"))?;
    state.set_download_folders(Some(folder.clone()), normalize_category_rules(categories));
    Ok(folder.to_string_lossy().to_string())
}

/// Clean the rules a window sends. A name is required, the extensions are the
/// engine's file types, and only an absolute folder is kept: a bad rule is
/// dropped rather than failing the push, so one typo cannot stop the rest.
fn normalize_category_rules(rules: Vec<state::CategoryRule>) -> Vec<state::CategoryRule> {
    let mut out = Vec::new();
    for rule in rules {
        let name = rule.name.trim();
        if name.is_empty() {
            continue;
        }
        let mut extensions: Vec<String> = Vec::new();
        for extension in rule.extensions {
            let Some(cleaned) = clean_file_type(&extension) else {
                continue;
            };
            if !extensions.contains(&cleaned) {
                extensions.push(cleaned);
            }
            if extensions.len() >= MAX_RULES {
                break;
            }
        }
        let folder = rule.folder.trim();
        let folder = if folder.is_empty() || !PathBuf::from(folder).is_absolute() {
            String::new()
        } else {
            folder.to_string()
        };
        out.push(state::CategoryRule {
            name: name.to_string(),
            extensions,
            folder,
            builtin: rule.builtin,
            fallback: rule.fallback,
        });
        if out.len() >= MAX_RULES {
            break;
        }
    }
    out
}

/// A sensible starting point for the destination path.
#[tauri::command]
fn default_destination(app: AppHandle) -> String {
    let downloads = app
        .path()
        .download_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    downloads.join("download.bin").to_string_lossy().to_string()
}

/// What the server says about a link before the download starts.
#[derive(serde::Serialize)]
struct LinkProbe {
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
async fn probe_link(
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

/// Show the window this page is in, once it has painted.
///
/// The two small windows are built hidden: a webview that is visible while it
/// loads shows an empty white frame first, which reads as a broken window. The
/// page calls this after its first frames, so the window appears with its
/// content already drawn.
#[tauri::command]
fn show_ready(window: tauri::WebviewWindow) -> Result<(), String> {
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

/// Paint the window itself in the theme's background.
///
/// What the webview has not drawn yet shows the window's own fill. The webview
/// default is white, so every load or repaint of a hidden window could flash
/// white before the page arrived. The page sends the colour of its
/// `--background` token, and a theme change sends the next one.
#[tauri::command]
fn set_window_background(window: tauri::WebviewWindow, color: String) -> Result<(), String> {
    let (red, green, blue) = parse_hex_color(&color).ok_or("expected a #rgb or #rrggbb colour")?;
    window
        .set_background_color(Some(tauri::window::Color(red, green, blue, 255)))
        .map_err(|error| error.to_string())
}

/// Read the tokens' hex form (`#0b1416`, `#fff`) into the channels Tauri wants.
fn parse_hex_color(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.trim().strip_prefix('#')?;
    let channel = |digits: &str| u8::from_str_radix(digits, 16).ok();
    match hex.len() {
        3 => {
            let cell = |index: usize| channel(&hex[index..index + 1]).map(|value| value * 17);
            Some((cell(0)?, cell(1)?, cell(2)?))
        }
        6 => Some((
            channel(&hex[0..2])?,
            channel(&hex[2..4])?,
            channel(&hex[4..6])?,
        )),
        _ => None,
    }
}

/// Whether a destination can hold the file, and whether a file is already
/// there. The add dialog asks before it starts, so a bad name is a line in
/// the dialog instead of a failed download.
#[tauri::command]
fn inspect_destination(
    state: State<'_, AppState>,
    dest: String,
) -> Result<conflict::DestinationStatus, String> {
    conflict::inspect(&state, &PathBuf::from(dest.trim()))
}

/// Open a file or a folder with the program the system uses for it.
///
/// The path comes from the download list, so it is a place the user picked.
#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let target = PathBuf::from(path.trim());
    if target.as_os_str().is_empty() {
        return Err("a path is required".to_string());
    }
    if !target.exists() {
        return Err(format!("{} is not there yet", target.display()));
    }
    open_in_os(&target)
}

/// Whether an address is a web page rather than something else on the machine.
///
/// The only links this window offers go to a website; a `file:` or a `javascript:`
/// address would be handed to whatever program owns it.
fn is_web_address(url: &str) -> bool {
    !url.chars().any(|c| c.is_control())
        && (url.starts_with("http://") || url.starts_with("https://"))
}

/// Open a web address in the browser the system uses.
///
/// The window is a webview, not a tab: an anchor here would replace the page
/// it is on, so the address goes to the shell instead.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    let address = url.trim();
    if !is_web_address(address) {
        return Err("only a web address can be opened here".to_string());
    }
    open_in_browser(address)
}

/// Hand the path to the shell of the system.
#[cfg(target_os = "windows")]
fn open_in_os(target: &std::path::Path) -> Result<(), String> {
    let mut command = if target.is_dir() {
        let mut command = std::process::Command::new("explorer");
        command.arg(target);
        command
    } else {
        // `explorer` would open the folder of a file, so a file goes through
        // the shell handler that opens it with its own program.
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler").arg(target);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("the system could not open it: {error}"))
}

#[cfg(target_os = "macos")]
fn open_in_os(target: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("the system could not open it: {error}"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn open_in_os(target: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("the system could not open it: {error}"))
}

/// Hand a web address to the browser the system uses.
///
/// The three forms differ only in the program and, on Windows, in the handler
/// `rundll32` is asked to load. The address arrives as one argument of one
/// process, never through a shell, so it cannot carry a second command.
fn open_in_browser(address: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");

    command
        .arg(address)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("the browser could not be opened: {error}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::default())
        .manage(grabber::Grabber::default())
        .setup(|app| {
            let state = app.state::<AppState>();
            let dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            // The log of what the downloads did. Opened first so a start that
            // goes wrong anywhere below has somewhere to be written down.
            idm_core::log::open(dir.join("idm-clone.log"));
            // The downloads of an earlier run are brought back, so the list is
            // still there after a restart.
            state.set_index(Index::new(dir.join("downloads.json")));
            state.restore_downloads();
            // Parts and manifests have a folder of their own from the very
            // first moment: a hand-over from the tray or the browser can
            // arrive before the window has read its settings, and its parts
            // must not land beside the finished file. The window pushes the
            // folder the reader chose over this default when it opens.
            let unfinished = dir.join("unfinished");
            match std::fs::create_dir_all(&unfinished) {
                Ok(()) => state.set_download_folders(Some(unfinished), Vec::new()),
                Err(error) => idm_core::log::write(
                    "warn",
                    &format!("the unfinished folder could not be made: {error}"),
                ),
            }
            // The tray and the close button follow a file of their own: they
            // are consulted before a page has loaded, and a webview that has
            // not painted yet must not decide whether closing exits.
            state.set_preferences_path(dir.join(prefs::FILE_NAME));
            match tray::install(app.handle()) {
                Ok(()) => {
                    state.set_tray_available(true);
                    // A restored download is already active when the window
                    // opens; the tooltip must say so before the next report.
                    tray::refresh(app.handle());
                }
                Err(error) => {
                    // Without an icon there is no way back from a hidden
                    // window, so the close and minimize choices fall back to
                    // closing and minimizing.
                    idm_core::log::write("warn", &format!("the tray icon did not open: {error}"));
                }
            }
            if let Some(window) = app.get_webview_window("main") {
                window_style::round_corners(&window);
            }
            // The browser extension talks to the application through this.
            bridge::start(app.handle().clone());
            // A copied link is offered while the application is in the tray.
            clipboard::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // A close is never the end of the main webview: it either hides,
            // asks, or takes the one shutdown path. The smaller windows of
            // the application really close.
            WindowEvent::CloseRequested { api, .. } => {
                if window.label() != "main" {
                    return;
                }
                api.prevent_close();
                lifecycle::close_requested(window);
            }
            // The taskbar and an OS shortcut can minimize a window without
            // the title-bar button; hide it here too when that is configured.
            WindowEvent::Resized(_) => {
                if window.label() == "main"
                    && window.is_minimized().unwrap_or(false)
                    && window
                        .app_handle()
                        .state::<AppState>()
                        .preferences()
                        .minimize_to_tray
                {
                    lifecycle::minimize_to_tray(window);
                }
            }
            // A progress window that really closed takes its own tray icon
            // with it; one left behind would open nothing.
            WindowEvent::Destroyed => {
                if let Some(id) = progress::id_of_label(window.label()) {
                    progress::remove_tray(window.app_handle(), id);
                    progress::forget(window.label());
                    // The two windows report one download, so closing either
                    // closes both. Two reports of one row left open read as a
                    // close that did not work.
                    complete::close(window.app_handle(), id);
                }
                if let Some(id) = complete::id_of_label(window.label()) {
                    progress::close(window.app_handle(), id);
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            start_download,
            list_downloads,
            pause_download,
            resume_download,
            cancel_download,
            retry_download,
            redownload_download,
            relocate_download,
            set_download_meta,
            set_download_destination,
            queue_download,
            dequeue_download,
            delete_download_file,
            remove_download,
            set_speed_limit,
            set_proxy,
            set_download_speed_limit,
            set_download_category,
            open_progress_window,
            set_progress_details,
            set_site_connections,
            site_connections,
            set_max_connections,
            desktop_preferences,
            set_minimize_to_tray,
            set_close_to_tray,
            lifecycle::minimize_window,
            lifecycle::hide_main_window,
            set_show_complete_dialog,
            set_show_progress_on_start,
            set_browser_connections,
            set_clipboard_capture,
            set_launch_on_startup,
            set_browser_policy,
            set_video_button_settings,
            video_button_settings,
            set_download_folders,
            browser_status,
            folder_status,
            app_paths,
            validate_proxy,
            set_browser_folder,
            quit_app,
            default_destination,
            probe_link,
            inspect_destination,
            grabber::open_grabber_window,
            grabber::grabber_start,
            grabber::grabber_cancel,
            grabber::grabber_state,
            grabber::grabber_download,
            show_ready,
            set_window_background,
            open_path,
            open_url,
            updater::check_update,
            updater::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building B Download Manager")
        .run(|app, event| {
            // The operating system can end the process without a close
            // request (logoff, shutdown). The deliberate paths use
            // `lifecycle::shutdown`; this is the last chance for the rest.
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                lifecycle::save_on_exit(app);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_path_is_refused() {
        assert!(open_path("   ".to_string()).is_err());
    }

    #[test]
    fn only_a_web_address_is_opened_in_the_browser() {
        // Each of these is refused before anything is spawned, so the test
        // never opens a window of its own.
        assert!(!is_web_address(""));
        assert!(!is_web_address("ftp://example.org/file"));
        assert!(!is_web_address("file:///C:/Windows/system.ini"));
        assert!(!is_web_address("javascript:alert(1)"));
        assert!(!is_web_address("https://example.org\0"));
        assert!(is_web_address("https://example.org/"));
        assert!(is_web_address("http://localhost:1420"));
        assert!(open_url("mailto:someone@example.org".to_string()).is_err());
    }

    #[test]
    fn a_start_request_reads_the_names_the_window_sends() {
        let request: StartRequest = serde_json::from_value(serde_json::json!({
            "url": "https://example.org/film.mp4",
            "dest": "D:\\Downloads\\film.mp4",
            "connections": 8,
            "referer": null,
            "queue": false,
            "category": "Video",
            "description": null,
            "conflict": "rename",
        }))
        .expect("a start request");

        assert_eq!(request.url, "https://example.org/film.mp4");
        assert_eq!(request.connections, Some(8));
        assert_eq!(request.queue, Some(false));
        assert_eq!(request.category.as_deref(), Some("Video"));
        assert_eq!(request.conflict, Some(conflict::Choice::Rename));
    }

    #[test]
    fn a_start_request_from_an_older_window_asks_the_destination_question() {
        let request: StartRequest = serde_json::from_value(serde_json::json!({
            "url": "https://example.org/film.mp4",
            "dest": "D:\\Downloads\\film.mp4",
        }))
        .expect("a start request");

        assert_eq!(request.conflict, None, "an older window has no answer yet");
        assert_eq!(request.conflict.unwrap_or_default(), conflict::Choice::Ask);
    }

    #[test]
    fn a_theme_colour_is_read_for_the_window() {
        assert_eq!(parse_hex_color("#0b1416"), Some((11, 20, 22)));
        assert_eq!(parse_hex_color(" #fff "), Some((255, 255, 255)));
        assert_eq!(parse_hex_color("0b1416"), None);
        assert_eq!(parse_hex_color("#12345"), None);
    }

    #[test]
    fn a_path_that_is_not_there_is_refused() {
        let missing = std::env::temp_dir().join("idm-clone-missing-8c41.bin");
        let error = open_path(missing.to_string_lossy().to_string()).unwrap_err();
        assert!(error.contains("not there yet"), "the error was: {error}");
    }

    #[test]
    fn a_capture_policy_is_cleaned_at_the_boundary() {
        let policy = normalize_policy(BrowserPolicy {
            capture: true,
            ask: true,
            types: vec![
                " .MP4 ".to_string(),
                "mp4".to_string(),
                "not a type".to_string(),
                "*.zip".to_string(),
            ],
            skip_sites: vec![
                "https://Example.ORG/download".to_string(),
                "example.org".to_string(),
                "not a host".to_string(),
            ],
        });

        assert_eq!(policy.types, vec!["mp4", "zip"]);
        assert_eq!(policy.skip_sites, vec!["example.org"]);
    }

    #[test]
    fn a_browser_hand_over_connection_count_is_clamped() {
        let state = AppState::default();
        assert_eq!(state.browser_connections(), 8, "the engine default");
        state.set_browser_connections(0);
        assert_eq!(state.browser_connections(), 1, "a count below the minimum");
        state.set_browser_connections(99);
        assert_eq!(state.browser_connections(), 32, "a count above the ceiling");
        state.set_browser_connections(16);
        assert_eq!(state.browser_connections(), 16, "a count in range");
    }

    #[test]
    fn a_proxy_is_checked_before_it_is_stored() {
        assert_eq!(
            validate_proxy(" socks5://127.0.0.1:1080 ".to_string()),
            Ok("socks5://127.0.0.1:1080".to_string())
        );
        assert_eq!(validate_proxy("  ".to_string()), Ok(String::new()));
        assert!(validate_proxy("127.0.0.1:1080".to_string()).is_err());
        assert!(validate_proxy("ftp://127.0.0.1:1080".to_string()).is_err());
        assert!(validate_proxy("http://user:secret@127.0.0.1:8080".to_string()).is_err());
    }
}
