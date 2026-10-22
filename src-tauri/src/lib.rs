mod state;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};

use idm_core::downloader::{Download, DownloadInfo, Reporter, Status, DEFAULT_CONNECTIONS};
use idm_core::limits::Limits;
use idm_core::session::Index;
use state::AppState;

/// Event carrying the state of one download to the UI.
const UPDATE_EVENT: &str = "download://update";

/// The reporter the desktop app hands to the engine: it pushes the state of
/// a download into the user interface.
struct AppReporter(AppHandle);

impl Reporter for AppReporter {
    fn report(&self, info: &DownloadInfo) {
        let _ = self.0.emit(UPDATE_EVENT, info.clone());
    }
}

#[tauri::command]
fn start_download(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    dest: String,
    connections: Option<u32>,
) -> Result<u64, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("a URL is required".to_string());
    }
    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ftp://")) {
        return Err("the URL must start with http://, https:// or ftp://.".to_string());
    }

    let dest = PathBuf::from(dest.trim());
    if dest.as_os_str().is_empty() {
        return Err("a destination file is required".to_string());
    }

    let download = Download::new(
        state.next_id(),
        url,
        dest,
        connections.unwrap_or(DEFAULT_CONNECTIONS).clamp(1, 32),
        state.limits().proxy().as_deref(),
    )?;

    let id = download.id;
    state.insert(download.clone());
    state.save_index();

    let limits: Limits = state.limits().clone();
    tauri::async_runtime::spawn(async move {
        idm_core::downloader::run(Arc::new(AppReporter(app)), limits, download).await;
    });

    Ok(id)
}

#[tauri::command]
fn list_downloads(state: State<'_, AppState>) -> Vec<DownloadInfo> {
    state.infos()
}

#[tauri::command]
fn pause_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    download.control.pause();
    download.set_status(Status::Paused);
    let _ = app.emit(UPDATE_EVENT, download.info());
    state.save_index();
    Ok(())
}

#[tauri::command]
fn resume_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    download.control.resume();

    // A download whose task is gone — it was cancelled, or it failed — is
    // started again, from the parts that are on disk.
    if !download.is_running() && download.info().status != Status::Completed {
        let limits = state.limits().clone();
        let reporter = Arc::new(AppReporter(app));
        let running = download.clone();
        tauri::async_runtime::spawn(async move {
            idm_core::downloader::run(reporter, limits, running).await;
        });
    } else {
        download.set_status(Status::Running);
        let _ = app.emit(UPDATE_EVENT, download.info());
    }

    state.save_index();
    Ok(())
}

#[tauri::command]
fn cancel_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    download.cancel();
    download.set_status(Status::Cancelled);
    let _ = app.emit(UPDATE_EVENT, download.info());
    state.save_index();
    Ok(())
}

#[tauri::command]
fn remove_download(state: State<'_, AppState>, id: u64) -> Result<(), String> {
    // The download must stop before it leaves the list, or it would keep
    // writing to the disk with nothing to show for it.
    if let Some(download) = state.get(id) {
        download.cancel();
    }
    state.remove(id);
    state.save_index();
    Ok(())
}

/// Cap the bandwidth of the whole application. Zero means no cap.
#[tauri::command]
fn set_speed_limit(state: State<'_, AppState>, bytes_per_second: u64) -> u64 {
    state.limits().global_speed.set_rate(bytes_per_second);
    state.limits().global_speed.rate()
}

/// Cap the bandwidth of one download. Zero means no cap.
#[tauri::command]
fn set_download_speed_limit(state: State<'_, AppState>, id: u64, bytes_per_second: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    download.set_speed_limit(bytes_per_second);
    Ok(())
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

/// A sensible starting point for the destination path.
#[tauri::command]
fn default_destination(app: AppHandle) -> String {
    let downloads = app
        .path()
        .download_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    downloads.join("download.bin").to_string_lossy().to_string()
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_download,
            list_downloads,
            pause_download,
            resume_download,
            cancel_download,
            remove_download,
            set_speed_limit,
            set_proxy,
            set_download_speed_limit,
            set_site_connections,
            site_connections,
            default_destination,
            open_path,
        ])
        .build(tauri::generate_context!())
        .expect("error while building my-idm-clone")
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
    fn a_path_that_is_not_there_is_refused() {
        let missing = std::env::temp_dir().join("idm-clone-missing-8c41.bin");
        let error = open_path(missing.to_string_lossy().to_string()).unwrap_err();
        assert!(error.contains("not there yet"), "the error was: {error}");
    }
}
