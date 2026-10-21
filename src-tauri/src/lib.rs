mod downloader;
mod segmenter;
mod state;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};

use downloader::{Download, DownloadInfo, Status, DEFAULT_CONNECTIONS};
use state::AppState;

/// Event carrying the state of one download to the UI.
const UPDATE_EVENT: &str = "download://update";

fn emit(app: &AppHandle, download: &Arc<Download>) {
    let _ = app.emit(UPDATE_EVENT, download.info());
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
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("the URL must start with http:// or https://".to_string());
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
    )?;

    let id = download.id;
    state.insert(download.clone());

    tauri::async_runtime::spawn(async move {
        downloader::run(app, download).await;
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
    emit(&app, &download);
    Ok(())
}

#[tauri::command]
fn resume_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    download.control.resume();
    download.set_status(Status::Running);
    emit(&app, &download);
    Ok(())
}

#[tauri::command]
fn cancel_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    download.control.cancel();
    download.set_status(Status::Cancelled);
    emit(&app, &download);
    Ok(())
}

#[tauri::command]
fn remove_download(state: State<'_, AppState>, id: u64) -> Result<(), String> {
    state.remove(id);
    Ok(())
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_download,
            list_downloads,
            pause_download,
            resume_download,
            cancel_download,
            remove_download,
            default_destination,
        ])
        .run(tauri::generate_context!())
        .expect("error while running my-idm-clone");
}
