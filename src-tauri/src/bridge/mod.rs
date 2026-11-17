//! The loopback bridge the browser extension talks to.
//!
//! The extension cannot call Tauri's IPC, so the application offers it a small
//! JSON API on the loopback interface. Only this machine can reach the port,
//! only under its own address, and only a caller that presents an extension
//! origin and the extension's client header, so a web page cannot start a
//! download with an `<img>` tag. There is no code to copy and nothing to pair:
//! install the app, install the extension, and they find each other. The
//! endpoint the browser uses is also the one the extension's popup uses to read
//! progress and to pause, resume or stop a download.
//!
//! The contract is versioned: the extension says which API it expects in the
//! `X-BDM-Client` value and falls back to an offline state when the ping does
//! not name this application. The Rust side of the contract is tested in this
//! module.

pub(crate) mod auth;
pub(crate) mod batch;
pub(crate) mod dest;
mod http;

use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

use idm_core::downloader::Status;
use idm_core::log;

use crate::state::AppState;

use auth::Denial;
use handover::*;

mod handover;

/// The port the extension talks to. It is on the loopback interface, so it is
/// only reachable from this machine.
pub const PORT: u16 = 27500;

/// The version of the JSON API. The extension refuses a different one instead
/// of guessing what an older or newer host means.
pub const API_VERSION: u32 = 1;

/// The name the ping answers with. A stale process on the port will not use
/// it, so the extension can tell "nothing is listening" from "something else
/// is listening".
pub const APP_ID: &str = "b-download-manager";

/// The event the window listens to for a hand-over that asks the user first.
const DOWNLOAD_EVENT: &str = "bridge://download";

/// The shutdown signal of the one listener. The bridge starts once, with the
/// application; a genuine exit closes the port instead of leaving tasks to be
/// killed by the process teardown.
static STOP: OnceLock<watch::Sender<bool>> = OnceLock::new();

/// Start the server. It runs until [`stop`] is called or the application ends.
pub fn start(app: AppHandle) {
    let (sender, mut stop) = watch::channel(false);
    if STOP.set(sender).is_err() {
        log::write("warn", "the browser bridge was started twice");
    }

    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await {
            Ok(listener) => listener,
            Err(error) => {
                log::write(
                    "warn",
                    &format!("the browser bridge did not start: {error}"),
                );
                return;
            }
        };
        app.state::<AppState>().set_bridge_running(true);
        log::write(
            "info",
            &format!("the browser bridge waits on 127.0.0.1:{PORT}"),
        );

        loop {
            tokio::select! {
                _ = stop.changed() => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else {
                        continue;
                    };
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = serve(app, stream).await;
                    });
                }
            }
        }

        app.state::<AppState>().set_bridge_running(false);
        log::write("info", "the browser bridge stopped");
    });
}

/// Close the bridge to new requests. Called once, on the way out.
pub fn stop(state: &AppState) {
    if let Some(sender) = STOP.get() {
        let _ = sender.send(true);
    }
    state.set_bridge_running(false);
}

/// Read one request, answer it, and close the connection.
async fn serve(app: AppHandle, mut stream: tokio::net::TcpStream) -> std::io::Result<()> {
    let request = match http::read(&mut stream).await {
        Ok(request) => request,
        Err(error) => {
            log::write(
                "warn",
                &format!("the browser bridge refused a request: {error:?}"),
            );
            return http::write(
                &mut stream,
                error.status(),
                "application/json",
                &serde_json::to_vec(&error_body("bad_request", "the request could not be read"))
                    .unwrap_or_default(),
                None,
                &[],
            )
            .await;
        }
    };

    let state = app.state::<AppState>();
    let gate = gate(&request);

    if let Some(denial) = gate.denial {
        log::write(
            "warn",
            &format!("the browser bridge refused a request: {}", denial.code),
        );
        return refuse(&mut stream, denial, gate.cors.as_deref()).await;
    }

    if request.method == "OPTIONS" {
        let headers = [
            ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
            ("Access-Control-Allow-Headers", "Content-Type, X-BDM-Client"),
            ("Access-Control-Max-Age", "600"),
        ];
        return http::write(
            &mut stream,
            204,
            "text/plain",
            b"",
            gate.cors.as_deref(),
            &headers,
        )
        .await;
    }

    // A request from an extension origin is the browser reaching the
    // application, so the window can say it is connected without a guess.
    if gate.extension {
        state.touch_extension();
    }

    let (status, body) = route(&app, &request, gate.extension).await;
    let bytes = serde_json::to_vec(&body).unwrap_or_else(|_| {
        serde_json::to_vec(&error_body("internal", "the answer could not be written"))
            .unwrap_or_default()
    });
    http::write(
        &mut stream,
        status,
        "application/json",
        &bytes,
        gate.cors.as_deref(),
        &[],
    )
    .await
}

/// What the rule set says about one request, before any route runs.
///
/// Kept apart from the socket so the whole matrix — the origin, the host, the
/// preflight — is testable as a plain function.
struct Gate {
    /// The refusal to answer with, when the request does not pass.
    denial: Option<Denial>,
    /// The caller carries an extension origin, so it is the browser.
    extension: bool,
    /// The origin to echo back in the CORS header: the extension's, never a
    /// page's.
    cors: Option<String>,
}

fn gate(request: &Request) -> Gate {
    let cors = request
        .header("origin")
        .filter(|origin| is_extension_origin(origin))
        .map(str::to_string);
    let denial = auth::check(request).err();

    Gate {
        denial,
        extension: denial.is_none() && request.method != "OPTIONS" && cors.is_some(),
        cors,
    }
}

/// The routes of API version 1.
async fn route(app: &AppHandle, request: &Request, connected: bool) -> (u16, Value) {
    let path = request.path();
    match (request.method.as_str(), path) {
        ("GET", "/v1/ping") => (200, ok(ping(connected))),
        ("GET", "/v1/health") => (200, ok(health(app).await)),
        ("GET", "/v1/capture") => (200, ok(capture_policy(app))),
        ("POST", "/v1/capture") => set_capture_policy(app, &request.body),
        ("GET", "/v1/video-button-settings") => (200, ok(video_button_settings(app))),
        ("GET", "/v1/theme") => (200, ok(browser_theme(app))),
        ("POST", "/v1/media") => crate::video::capture::register(app, &request.body).await,
        ("GET", "/v1/mcp/downloads") | ("GET", "/v1/mcp/queue") | ("POST", "/v1/mcp/downloads") => {
            mcp_route(app, request)
        }
        ("POST", path) if path.starts_with("/v1/mcp/downloads/") => mcp_route(app, request),
        ("GET", "/v1/downloads") => (200, ok(downloads(app))),
        ("POST", "/v1/downloads") => add_download(app, &request.body).await,
        ("GET", "/v1/batch-options") => (200, ok(batch::options(app, &app.state::<AppState>()))),
        ("POST", "/v1/downloads/batch") => batch::add(app, &app.state::<AppState>(), &request.body),
        ("POST", "/v1/focus") => {
            crate::tray::show_window(app);
            (200, ok(json!({ "focused": true })))
        }
        ("POST", _) => match media_route(path) {
            Some((id, Some("download"))) => {
                crate::video::capture::download(app, id, &request.body).await
            }
            Some((id, Some("cancel"))) => crate::video::capture::cancel(app, id),
            Some((id, Some("allow-cookies"))) => {
                crate::video::capture::allow_cookies_bridge(app, id)
            }
            Some((_, Some(_))) => (404, error_body("not_found", "no such media action")),
            Some((_, None)) => (
                405,
                error_body("method_not_allowed", "use GET for this route"),
            ),
            None => match control_route(path) {
                Some((id, action)) => control(app, id, action),
                None => (404, error_body("not_found", "no such route")),
            },
        },
        ("GET", _) if media_route(path).is_some() => match media_route(path) {
            Some((id, None)) => crate::video::capture::view(app, id),
            _ => (
                405,
                error_body("method_not_allowed", "use POST for this route"),
            ),
        },
        ("GET", _) if control_route(path).is_some() => (
            405,
            error_body("method_not_allowed", "use POST for this route"),
        ),
        ("GET", _) => (404, error_body("not_found", "no such route")),
        _ => (
            405,
            error_body("method_not_allowed", "this method is not used"),
        ),
    }
}

type Request = http::Request;

/// The door the MCP server of the download manager talks through.
///
/// Every route below is refused with a sentence while the reader has not
/// turned "let AI assistants drive the app" on in the Advanced page; the
/// extension's own routes are not affected. A file on disk is deleted only
/// through a second, explicit call that says `deleteFile: true`.
fn mcp_route(app: &AppHandle, request: &Request) -> (u16, Value) {
    let state = app.state::<AppState>();
    if !state.engine_settings().mcp_enabled {
        return (
            403,
            error_body(
                "forbidden",
                "let AI assistants drive the app in Settings > Advanced first",
            ),
        );
    }
    let path = request.path();
    match (request.method.as_str(), path) {
        ("GET", "/v1/mcp/downloads") => (200, ok(downloads(app))),
        ("GET", "/v1/mcp/queue") => (
            200,
            ok(serde_json::to_value(state.queue().snapshot()).unwrap_or(Value::Null)),
        ),
        ("POST", "/v1/mcp/downloads") => mcp_add_download(app, &state, &request.body),
        ("POST", path) if path.starts_with("/v1/mcp/downloads/") && path.ends_with("/pause") => {
            mcp_control(app, &state, path, "pause")
        }
        ("POST", path) if path.starts_with("/v1/mcp/downloads/") && path.ends_with("/resume") => {
            mcp_control(app, &state, path, "resume")
        }
        ("POST", path) if path.starts_with("/v1/mcp/downloads/") && path.ends_with("/remove") => {
            mcp_remove(app, &state, path, &request.body)
        }
        _ => (404, error_body("not_found", "no such route")),
    }
}

/// The id in a `/v1/mcp/downloads/<id>/<action>` path.
fn mcp_id(path: &str, suffix: &str) -> Option<u64> {
    let rest = path.strip_prefix("/v1/mcp/downloads/")?;
    let id = rest.strip_suffix(suffix)?;
    id.parse().ok()
}

/// Pause or resume one download, the way the extension asks for it.
fn mcp_control(app: &AppHandle, state: &AppState, path: &str, action: &str) -> (u16, Value) {
    let Some(id) = mcp_id(path, &format!("/{action}")) else {
        return (400, error_body("bad_request", "a download id is a number"));
    };
    let result = match action {
        "pause" => crate::actions::pause(app, state, id),
        "resume" => crate::actions::resume(app, state, id, false),
        _ => return (404, error_body("not_found", "no such route")),
    };
    match result {
        Ok(()) => (200, ok(json!({ "id": id, "status": status_of(state, id) }))),
        Err(message) if message == "unknown download" => {
            (404, error_body("not_found", "unknown download"))
        }
        Err(message) => (409, error_body("conflict", message)),
    }
}

/// Add a download, as a direct start with the folder the caller chose.
fn mcp_add_download(app: &AppHandle, state: &AppState, body: &[u8]) -> (u16, Value) {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Add {
        url: String,
        folder: Option<String>,
        connections: Option<u32>,
    }

    let request: Add = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return (
                400,
                error_body(
                    "bad_request",
                    format!("the body could not be read: {error}"),
                ),
            )
        }
    };
    let url = request.url.trim().to_string();
    if !is_download_url(&url) {
        return (
            400,
            error_body(
                "bad_request",
                "the URL must start with http://, https:// or ftp://",
            ),
        );
    }
    // The connections are the caller's to ask for, within the same limits
    // the window enforces.
    let connections = request.connections.map(|count| count.clamp(1, 32));
    let pretty = dest::name_from_url(&url).unwrap_or_else(|| "download".to_string());
    let folder = request
        .folder
        .as_deref()
        .map(str::trim)
        .filter(|folder| !folder.is_empty())
        .map(std::path::PathBuf::from);
    let base = folder.clone().unwrap_or_else(|| {
        state
            .browser_folder()
            .or_else(|| app.path().download_dir().ok())
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    });
    let name = dest::sanitize(&pretty).unwrap_or(pretty);
    let target = base.join(&name);
    match crate::actions::start(
        app,
        state,
        url,
        target,
        crate::actions::StartOptions {
            connections,
            // A call with no dialog never replaces a file: it takes the
            // next free name beside it.
            conflict: crate::conflict::Choice::Rename,
            use_category_folder: folder.is_none(),
            ..crate::actions::StartOptions::default()
        },
    ) {
        Ok(started) => (
            200,
            ok(json!({
                "id": started.id,
                "name": std::path::Path::new(&started.dest)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or(name),
                "dest": started.dest,
            })),
        ),
        Err(refusal) => (400, error_body("bad_request", refusal.message)),
    }
}

/// Remove a download. The file on disk goes only with the second, explicit
/// call that says `deleteFile: true`.
fn mcp_remove(app: &AppHandle, state: &AppState, path: &str, body: &[u8]) -> (u16, Value) {
    let Some(id) = mcp_id(path, "/remove") else {
        return (400, error_body("bad_request", "a download id is a number"));
    };
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Remove {
        #[serde(default)]
        delete_file: bool,
    }
    let request: Remove = serde_json::from_slice(body).unwrap_or(Remove { delete_file: false });
    if request.delete_file {
        // Destroying a byte is the second call and never the first: the
        // download stops, and only a finished file is removed from disk.
        let _ = crate::actions::cancel(app, state, id);
        return match crate::actions::delete_file(app, state, id) {
            Ok(()) => (200, ok(json!({ "id": id, "deleted": true }))),
            Err(message) => (409, error_body("conflict", message)),
        };
    }
    // A row that leaves the list leaves nothing behind: the parts and the
    // manifest are the resume state of a row that will not exist any more.
    if let Some(download) = state.get(id) {
        download.cancel();
        download.forget_progress();
    }
    state.remove(id);
    state.save_index();
    crate::progress::close(app, id);
    crate::complete::close(app, id);
    crate::tray::refresh(app);
    (200, ok(json!({ "id": id, "deleted": false })))
}

/// How long the main thread took to answer, in milliseconds.
///
/// The main thread is the one that draws the window and answers the commands
/// of the webview. When it is blocked, the window stops answering even though
/// the bridge is still there and still says everything is fine. This is the
/// measurement that tells a frozen window from a busy one: the answer is the
/// time the main thread needed to pick up a task that was waiting for it.
///
/// A main thread that needs more than a second is in trouble: the window
/// cannot be drawn, so nothing the reader does is seen.
async fn health(app: &AppHandle) -> Value {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let started = std::time::Instant::now();
    let _ = app.run_on_main_thread(move || {
        let _ = sender.send(started.elapsed());
    });
    let answered = tokio::time::timeout(std::time::Duration::from_secs(5), receiver).await;
    match answered {
        Ok(Ok(waited)) => json!({
            "mainThreadMs": waited.as_millis() as u64,
            "frozen": false,
        }),
        // The main thread did not pick the task up in five seconds: it is
        // blocked, and the window is frozen whatever the bridge says.
        _ => json!({
            "mainThreadMs": 5000u64,
            "frozen": true,
        }),
    }
}

/// What the extension says about itself. `connected` tells a caller that the
/// application heard the header that only the extension sends, so the popup can
/// say "connected" instead of guessing.
fn ping(connected: bool) -> Value {
    json!({
        "app": APP_ID,
        "api": API_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "port": PORT,
        "connected": connected,
    })
}

/// The capture rules the extension follows while the application is running.
///
/// The extension keeps its own rules for when the app is closed; this answer
/// is the newer one whenever it can be reached.
fn capture_policy(app: &AppHandle) -> Value {
    let policy = app.state::<AppState>().browser_policy();
    json!({
        "capture": policy.capture,
        "ask": policy.ask,
        "types": policy.types,
        "skipSites": policy.skip_sites,
    })
}

/// What the extension sends when the reader changes a capture rule there.
///
/// Every field is optional: an older extension that only knows two of them
/// leaves the rest as the app has them instead of resetting them.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CaptureRequest {
    capture: Option<bool>,
    ask: Option<bool>,
    types: Option<Vec<String>>,
    skip_sites: Option<Vec<String>>,
}

/// The longest list of rules the bridge accepts. The app's own Settings page
/// keeps the same ceiling.
const MAX_CAPTURE_RULES: usize = 64;
const MAX_CAPTURE_RULE: usize = 128;

/// Lower-case, trimmed, without blanks or repeats. Junk is dropped rather than
/// stored, because the app's Settings page shows this list back to the reader.
fn clean_capture_rules(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let entry = value.trim().to_ascii_lowercase();
        if entry.is_empty()
            || entry.len() > MAX_CAPTURE_RULE
            || entry
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
        {
            continue;
        }
        if !out.contains(&entry) {
            out.push(entry);
        }
        if out.len() >= MAX_CAPTURE_RULES {
            break;
        }
    }
    out
}

/// `POST /v1/capture`
///
/// The extension's options page edits the same rules the app's Settings page
/// does. Writing them here goes through the engine's settings file, so both
/// pages, a restart and the next hand-over all see one answer.
fn set_capture_policy(app: &AppHandle, body: &[u8]) -> (u16, Value) {
    let request: CaptureRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(error) => {
            return (
                400,
                error_body(
                    "bad_request",
                    format!("the body could not be read: {error}"),
                ),
            )
        }
    };

    let state = app.state::<AppState>();
    let mut policy = state.browser_policy();
    if let Some(capture) = request.capture {
        policy.capture = capture;
    }
    if let Some(ask) = request.ask {
        policy.ask = ask;
    }
    if let Some(types) = request.types {
        policy.types = clean_capture_rules(types);
    }
    if let Some(sites) = request.skip_sites {
        policy.skip_sites = clean_capture_rules(sites);
    }

    state.modify_settings(|settings| settings.browser_policy = policy.clone());
    state.set_browser_policy(policy.clone());
    // The settings window has the old rules open; tell it what the extension
    // just wrote instead of leaving it to find out after a restart.
    let _ = app.emit("settings://browser-policy", capture_policy(app));
    log::write(
        "info",
        &format!(
            "the extension set the capture rules: capture={}, ask={}",
            policy.capture, policy.ask
        ),
    );

    (200, ok(capture_policy(app)))
}

/// The list the popup polls, and the number the section caption shows.
fn downloads(app: &AppHandle) -> Value {
    let state = app.state::<AppState>();
    let infos = state.infos();
    let active = infos
        .iter()
        .filter(|info| {
            matches!(
                info.status,
                Status::Queued | Status::Running | Status::Paused
            )
        })
        .count();
    json!({ "downloads": infos, "active": active })
}

/// The video download button appearance the extension applies to its overlay.
fn video_button_settings(app: &AppHandle) -> Value {
    let settings = app.state::<AppState>().video_button_settings();
    json!({
        "style": settings.style,
        "label": settings.label,
        "hover": settings.hover,
    })
}

/// The appearance the window last published, or `null` before it has one.
fn browser_theme(app: &AppHandle) -> Value {
    serde_json::to_value(app.state::<AppState>().browser_theme()).unwrap_or(Value::Null)
}

/// Pause, resume, stop or show one download.
fn control(app: &AppHandle, id: u64, action: &str) -> (u16, Value) {
    let state = app.state::<AppState>();
    let result = match action {
        "pause" => crate::actions::pause(app, &state, id),
        "resume" => crate::actions::resume(app, &state, id, false),
        "cancel" => crate::actions::cancel(app, &state, id),
        "show" => {
            let Some(download) = state.get(id) else {
                return (404, error_body("not_found", "unknown download"));
            };
            let info = download.info();
            let handle = app.clone();
            match app.run_on_main_thread(move || crate::progress::show(&handle, &info, true)) {
                Ok(()) => Ok(()),
                Err(error) => Err(format!("the window did not open: {error}")),
            }
        }
        _ => return (404, error_body("not_found", "no such download action")),
    };

    match result {
        Ok(()) => (
            200,
            ok(json!({ "id": id, "action": action, "status": status_of(&state, id) })),
        ),
        Err(message) if message == "unknown download" => {
            (404, error_body("not_found", "unknown download"))
        }
        Err(message) => (409, error_body("conflict", message)),
    }
}

/// The state of a download after a control, so the popup can draw it at once.
fn status_of(state: &AppState, id: u64) -> Value {
    state
        .get(id)
        .map(|download| serde_json::to_value(download.info()).unwrap_or(Value::Null))
        .unwrap_or(Value::Null)
}

/// The id and the action of a `/v1/downloads/<id>/<action>` route.
fn control_route(path: &str) -> Option<(u64, &str)> {
    let rest = path.strip_prefix("/v1/downloads/")?;
    let (id, action) = rest.split_once('/')?;
    if action.is_empty() || action.contains('/') {
        return None;
    }
    Some((id.parse().ok()?, action))
}

/// The id and the optional action of a `/v1/media/<id>[/<action>]` route.
fn media_route(path: &str) -> Option<(u64, Option<&str>)> {
    let rest = path.strip_prefix("/v1/media/")?;
    match rest.split_once('/') {
        Some((id, action)) => {
            if action.is_empty() || action.contains('/') {
                return None;
            }
            Some((id.parse().ok()?, Some(action)))
        }
        None => Some((rest.parse().ok()?, None)),
    }
}

/// An extension origin. Firefox uses `moz-extension://`, Chromium
/// `chrome-extension://`; the older extension build is still recognised. The
/// real rule lives in [`auth`], where the token is checked beside it.
fn is_extension_origin(origin: &str) -> bool {
    auth::is_extension_origin(origin)
}

/// A successful answer.
pub(crate) fn ok(data: Value) -> Value {
    json!({ "ok": true, "data": data })
}

/// A refusal.
pub(crate) fn error_body(code: &str, message: impl Into<String>) -> Value {
    json!({ "ok": false, "error": { "code": code, "message": message.into() } })
}

/// Answer a request with a JSON error. The origin is echoed only when it was
/// the extension's, so a refusal can be read in the browser instead of looking
/// like an application that is not running.
async fn refuse(
    stream: &mut tokio::net::TcpStream,
    denial: Denial,
    origin: Option<&str>,
) -> std::io::Result<()> {
    let body = serde_json::to_vec(&error_body(denial.code, denial.message)).unwrap_or_default();
    http::write(
        stream,
        denial.status,
        "application/json",
        &body,
        origin,
        &[],
    )
    .await
}

#[cfg(test)]
mod tests;
