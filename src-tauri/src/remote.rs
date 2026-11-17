//! The phone and LAN remote.
//!
//! The server itself lives in the crate; this is the app's side of it: the
//! setting that turns it on, the backend that answers its questions, the
//! devices it may talk to, and the commands the settings page calls.
//!
//! It is off until the reader turns it on, it binds to one address (this
//! machine's address on its own network, never every address), and every
//! request from a phone outside that network or naming something else in its
//! `Host` header is refused before it sees anything.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, UNIX_EPOCH};

use tauri::{AppHandle, Manager, State};

use idm_core::downloader::Status;

use idm_remote::{
    DeviceStore, RemoteBackend, RemoteConfig, RemoteDownload, RemoteServer, DEFAULT_PORT,
};

use serde_json::json;

use crate::actions;
use crate::state::AppState;

/// The state of the remote, shared by the app and the listener.
pub struct Remote {
    /// The devices that may talk to it, and the tokens they hold.
    store: std::sync::Arc<DeviceStore>,
    /// The listener, when it is on.
    server: Mutex<Option<RemoteServer>>,
    /// The downloads the listener answers about, and the last sample of each
    /// to work out a speed from two polls of the phone.
    backend: std::sync::Arc<AppBackend>,
}

/// The one remote of the application.
fn remote() -> &'static Remote {
    REMOTE.get_or_init(|| Remote {
        store: std::sync::Arc::new(DeviceStore::new()),
        server: Mutex::new(None),
        backend: std::sync::Arc::new(AppBackend::new()),
    })
}

static REMOTE: OnceLock<Remote> = OnceLock::new();

/// The backend the listener talks to the app through.
struct AppBackend {
    /// The application, filled in when the remote starts.
    app: Mutex<Option<AppHandle>>,
    /// The last poll of each download: when it was and how far it had got,
    /// so two polls of the phone give it a speed.
    last: Mutex<HashMap<u64, (Instant, u64)>>,
}

impl AppBackend {
    fn new() -> Self {
        Self {
            app: Mutex::new(None),
            last: Mutex::new(HashMap::new()),
        }
    }

    fn handle(&self) -> Option<AppHandle> {
        self.app.lock().expect("backend lock poisoned").clone()
    }
}

impl RemoteBackend for AppBackend {
    fn downloads(&self) -> Vec<RemoteDownload> {
        let Some(app) = self.handle() else {
            return Vec::new();
        };
        let state = app.state::<AppState>();
        let now = Instant::now();
        let mut downloads = Vec::new();
        for info in state.infos() {
            let name = std::path::Path::new(&info.dest)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| info.dest.clone());
            let speed = self
                .last
                .lock()
                .expect("backend lock poisoned")
                .get(&info.id)
                .map(|(then, before)| {
                    let span = now.duration_since(*then).as_secs_f64();
                    if span > 0.0 && info.downloaded >= *before {
                        ((info.downloaded - before) as f64 / span) as u64
                    } else {
                        0
                    }
                })
                .unwrap_or(0);
            if let Ok(mut last) = self.last.lock() {
                last.insert(info.id, (now, info.downloaded));
            }
            downloads.push(RemoteDownload {
                id: info.id,
                name,
                size: info.total,
                downloaded: info.downloaded,
                speed,
                state: status_name(info.status).to_string(),
            });
        }
        downloads
    }

    fn add(&self, url: &str, folder: Option<&str>) -> Result<String, String> {
        let Some(app) = self.handle() else {
            return Err("the app is not running".to_string());
        };
        let state = app.state::<AppState>();
        let url = url.trim().to_string();
        if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ftp://"))
        {
            return Err("the URL must start with http://, https:// or ftp://".to_string());
        }
        let name = crate::bridge::dest::name_from_url(&url).unwrap_or_else(|| "download".into());
        let name = crate::bridge::dest::sanitize(&name).unwrap_or(name);
        let base = match folder.map(str::trim).filter(|folder| !folder.is_empty()) {
            Some(folder) => PathBuf::from(folder),
            None => state
                .browser_folder()
                .or_else(|| app.path().download_dir().ok())
                .unwrap_or_else(|| PathBuf::from(".")),
        };
        let target = base.join(&name);
        actions::start(
            &app,
            &state,
            url,
            target,
            actions::StartOptions {
                conflict: crate::conflict::Choice::Rename,
                use_category_folder: false,
                ..actions::StartOptions::default()
            },
        )
        .map_err(|refusal| refusal.message)
        .and_then(|started| {
            started
                .id
                .map(|id| id.to_string())
                .ok_or_else(|| "the download was skipped".to_string())
        })
    }

    fn pause(&self, id: u64) -> Result<(), String> {
        let Some(app) = self.handle() else {
            return Err("the app is not running".to_string());
        };
        let state = app.state::<AppState>();
        actions::pause(&app, &state, id)
    }

    fn resume(&self, id: u64) -> Result<(), String> {
        let Some(app) = self.handle() else {
            return Err("the app is not running".to_string());
        };
        let state = app.state::<AppState>();
        actions::resume(&app, &state, id, false)
    }

    fn remove(&self, id: u64) -> Result<(), String> {
        let Some(app) = self.handle() else {
            return Err("the app is not running".to_string());
        };
        let state = app.state::<AppState>();
        // A row that leaves the list leaves nothing behind: the parts and
        // the manifest are its resume state.
        if let Some(download) = state.get(id) {
            download.cancel();
            download.forget_progress();
        }
        state.remove(id);
        state.save_index();
        crate::progress::close(&app, id);
        crate::complete::close(&app, id);
        crate::tray::refresh(&app);
        Ok(())
    }
}

/// The status of a download, as a phone reads it.
fn status_name(status: Status) -> &'static str {
    match status {
        Status::Queued => "queued",
        Status::Running => "running",
        Status::Paused => "paused",
        Status::Completed => "completed",
        Status::Failed => "failed",
        Status::Cancelled => "cancelled",
    }
}

/// The state of the remote and the devices that may talk to it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    /// Whether the listener is on at all.
    pub enabled: bool,
    /// The address the phone reaches it on, with the port it listens on.
    pub address: Option<String>,
    /// The paired devices, with the time each paired.
    pub devices: Vec<serde_json::Value>,
}

/// The devices that may talk to the remote, as the window shows them.
fn devices_of(store: &DeviceStore) -> Vec<serde_json::Value> {
    store
        .list()
        .into_iter()
        .map(|device| {
            let paired = device
                .paired_at
                .duration_since(UNIX_EPOCH)
                .map(|since| since.as_millis() as u64)
                .unwrap_or(0);
            json!({
                "id": device.id,
                "name": device.name,
                "pairedAt": paired,
            })
        })
        .collect()
}

/// Bring the remote up or take it down, and keep the choice.
///
/// The device list is kept beside the settings: the hash of each token stays
/// in the file, never the token itself.
#[tauri::command]
pub async fn remote_set_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<RemoteStatus, String> {
    state.modify_settings(|settings| {
        settings.remote_enabled = enabled;
    });
    state.save_engine_settings();
    if enabled {
        start(&app, state.inner())?;
    } else {
        stop();
    }
    Ok(status(state.inner()))
}

/// Switch the phone remote on or off, from the tray, with no window.
///
/// A reader with the remote on who wants it gone does not open the options
/// window first; the tray is the door out of it.
pub(crate) fn toggle_from_tray(app: &AppHandle) {
    let state = app.state::<AppState>();
    let on = !state.engine_settings().remote_enabled;
    state.modify_settings(|settings| {
        settings.remote_enabled = on;
    });
    if on {
        let _ = start(app, &state);
    } else {
        stop();
    }
    idm_core::log::write(
        "info",
        if on {
            "the phone remote is on"
        } else {
            "the phone remote is off"
        },
    );
}

/// What the remote is doing, and who may talk to it.
#[tauri::command]
pub fn remote_status(state: State<'_, AppState>) -> RemoteStatus {
    status(&state)
}

/// The pairing code: the one that is good now, and a fresh one if there is
/// none.
#[tauri::command]
pub fn remote_pairing_code(state: State<'_, AppState>) -> Option<serde_json::Value> {
    let remote = remote();
    let held = remote.server.lock().expect("remote server lock poisoned");
    let server = held.as_ref()?;
    // The code the last pairing showed is still good; a fresh one is made
    // only when it was spent or when there is none.
    let code = match server.pairing_code() {
        Some(code) if !code.is_expired() => code,
        _ => server.rotate_pairing_code(),
    };
    let _ = state;
    let expires = code
        .expires_at
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0);
    Some(json!({ "code": code.code, "expiresAt": expires }))
}

/// Forget a device; its token stops working at once.
#[tauri::command]
pub fn remote_revoke(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    let id = device_id.trim();
    if id.is_empty() {
        return Err("a device id is needed".to_string());
    }
    if !remote().store.revoke(id) {
        return Err("no such device".to_string());
    }
    save_devices(&state)?;
    Ok(devices_of(&remote().store))
}

/// Start the listener, load the devices that may talk to it, and tell the
/// backend where the downloads are.
///
/// The crate's start is async and this one is called from plain, synchronous
/// places (the tray, the setup), so the listener is bound in a task of the
/// module's own runtime and the answer is waited on in this thread. The
/// runtime is not the application's own: the start waits for the answer
/// while the application holds its runtime, and a task of that runtime
/// cannot be driven while the start holds it.
pub(crate) fn start(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let remote = remote();
    *remote.backend.app.lock().expect("backend lock poisoned") = Some(app.clone());
    load_devices(state, remote)?;
    let enabled = state.engine_settings().remote_enabled;
    let store = remote.store.clone();
    let config = RemoteConfig {
        enabled,
        interface: None,
        port: DEFAULT_PORT,
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let runtime = runtime();
    std::thread::spawn(move || {
        let answer = runtime.block_on(RemoteServer::start(config, store));
        let _ = tx.send(answer);
    });
    let server = rx
        .recv_timeout(ANSWER_WAIT)
        .map_err(|_| "the remote did not answer in time".to_string())?
        .map_err(|error| error.to_string())?;
    server.set_backend(remote.backend.clone());
    let address = server.local_addr();
    *remote.server.lock().expect("remote server lock poisoned") = Some(server);
    idm_core::log::write("info", &format!("the phone remote is on: http://{address}"));
    Ok(())
}

/// How long the listener may take to bind.
const ANSWER_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// A runtime of the remote module's own.
///
/// The server keeps a task of this runtime alive for as long as it listens,
/// so the runtime has threads of its own: a runtime that is only driven
/// while a task is waited for would take the listener down with it.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("the runtime of the phone remote")
    })
}

/// Stop the listener.
fn stop() {
    if let Some(server) = remote()
        .server
        .lock()
        .expect("remote server lock poisoned")
        .take()
    {
        server.stop();
    }
}

/// The state of the remote, as the window shows it.
fn status(state: &AppState) -> RemoteStatus {
    let remote = remote();
    let enabled = state.engine_settings().remote_enabled;
    let address = {
        let held = remote.server.lock().expect("remote server lock poisoned");
        held.as_ref()
            .map(|server| format!("http://{}:{}", server.local_addr().ip(), server.port()))
    };
    RemoteStatus {
        enabled: enabled && address.is_some(),
        address,
        devices: devices_of(&remote.store),
    }
}

/// Load the devices from the file they are kept in.
fn load_devices(state: &AppState, remote: &'static Remote) -> Result<(), String> {
    let Some(path) = device_file(state) else {
        return Ok(());
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let Ok(devices) = serde_json::from_str::<Vec<idm_remote::Device>>(&text) else {
        return Ok(());
    };
    remote.store.replace_with(devices);
    Ok(())
}

/// Write the devices to the file they are kept in.
fn save_devices(state: &AppState) -> Result<(), String> {
    let Some(path) = device_file(state) else {
        return Ok(());
    };
    let json = serde_json::to_vec_pretty(&remote().store.list())
        .map_err(|error| format!("the devices could not be written: {error}"))?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, &json)
        .map_err(|error| format!("the devices could not be written: {error}"))
}

/// The file the devices are kept in.
fn device_file(state: &AppState) -> Option<PathBuf> {
    let dir = state.data_directory();
    Some(dir?.join("remote-devices.json"))
}
