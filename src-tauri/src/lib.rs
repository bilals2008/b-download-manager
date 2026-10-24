mod actions;
mod bridge;
mod complete;
mod conflict;
mod dest;
mod progress;
mod state;
mod window_style;

use std::path::PathBuf;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

use idm_core::downloader::{DownloadInfo, Status};
use idm_core::session::Index;
use idm_core::transfer::Source;
use state::AppState;

use actions::UPDATE_EVENT;

/// Event carrying a command from the tray to the UI.
const TRAY_EVENT: &str = "tray://command";

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
    actions::start(
        &app,
        &state,
        request.url,
        PathBuf::from(request.dest.trim()),
        actions::StartOptions {
            connections: request.connections,
            referer: request.referer,
            queue: request.queue.unwrap_or(false),
            category: request.category,
            description: request.description,
            conflict: request.conflict.unwrap_or_default(),
        },
    )
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
    actions::redownload(&app, &state, id, conflict.unwrap_or_default())
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
    }
    state.remove(id);
    state.save_index();
    // The progress window of a row that is gone has nothing to show.
    progress::close(&app, id);
    Ok(())
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
fn set_proxy(state: State<'_, AppState>, url: Option<String>) {
    state.limits().set_proxy(url.as_deref());
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
/// out.
#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// How many connections the whole application may have open at once.
#[tauri::command]
fn set_max_connections(state: State<'_, AppState>, max: u32) -> u32 {
    state.limits().budget.resize(max);
    max
}

/// Whether closing the window hides it into the tray.
#[tauri::command]
fn set_minimize_to_tray(state: State<'_, AppState>, on: bool) -> bool {
    state.set_minimize_to_tray(on);
    on
}

/// Whether a finished download gets its own window.
#[tauri::command]
fn set_show_complete_dialog(state: State<'_, AppState>, on: bool) -> bool {
    state.set_show_complete_dialog(on);
    on
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
    /// honours ranges and knows how big the file is. This is the engine's own
    /// rule, so the dialog cannot promise a resume the engine would not make.
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
    Ok(LinkProbe {
        name: probe.name,
        total: probe.total,
        ranges: probe.ranges,
        resumable: probe.ranges && probe.total.is_some(),
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

/// The tray icon: the window comes back from it, the queue is started and
/// stopped from it, and it is the way out of the application.
///
/// Each progress window owns a tray icon of its own (`progress.rs`); this one
/// is the application itself.
fn build_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show B Download Manager", true, None::<&str>)?;
    let start = MenuItem::with_id(app, "start-queue", "Start queue", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop-queue", "Stop queue", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &start, &stop, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("B Download Manager");

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app),
            "start-queue" => {
                let _ = app.emit(TRAY_EVENT, "start-queue");
            }
            "stop-queue" => {
                let _ = app.emit(TRAY_EVENT, "stop-queue");
            }
            "quit" => {
                let state = app.state::<AppState>();
                state.save_manifests();
                state.save_index();
                app.exit(0);
            }
            // The menu of a progress window's own tray icon. Tauri's menu
            // listener is global, so this is the one place those items are
            // routed from; `handle_menu` ignores an id it does not own.
            id => progress::handle_menu(app, id),
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Bring the window back from the tray.
pub(crate) fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            // The downloads of an earlier run are brought back, so the list is
            // still there after a restart.
            let state = app.state::<AppState>();
            let dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            // The log of what the downloads did.
            idm_core::log::open(dir.join("idm-clone.log"));
            state.set_index(Index::new(dir.join("downloads.json")));
            state.restore_downloads();
            build_tray(app.handle())?;
            if let Some(window) = app.get_webview_window("main") {
                window_style::round_corners(&window);
            }
            // The browser extension talks to the application through this.
            bridge::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing the window hides it into the tray, the way IDM does.
            // The tray menu is the way out. Only the main window is kept;
            // the smaller windows of the application really close.
            WindowEvent::CloseRequested { api, .. } => {
                if window.label() != "main" {
                    return;
                }
                let state = window.app_handle().state::<AppState>();
                if state.minimize_to_tray() {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            // A progress window that really closed takes its own tray icon
            // with it; one left behind would open nothing.
            WindowEvent::Destroyed => {
                if let Some(id) = progress::id_of_label(window.label()) {
                    progress::remove_tray(window.app_handle(), id);
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
            remove_download,
            set_speed_limit,
            set_proxy,
            set_download_speed_limit,
            open_progress_window,
            set_progress_details,
            set_site_connections,
            site_connections,
            set_max_connections,
            set_minimize_to_tray,
            set_show_complete_dialog,
            set_browser_folder,
            quit_app,
            default_destination,
            probe_link,
            inspect_destination,
            show_ready,
            set_window_background,
            open_path,
        ])
        .build(tauri::generate_context!())
        .expect("error while building B Download Manager")
        .run(|app, event| {
            // When the application closes, the resume state of the running
            // downloads is written to disk, so nothing that was fetched is
            // lost.
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                let state = app.state::<AppState>();
                state.save_manifests();
                state.save_index();
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
}
