//! The loopback bridge the Firefox extension talks to.
//!
//! The extension cannot call Tauri's IPC, so the application offers it a small
//! JSON API on the loopback interface. Only this machine can reach the port,
//! and every request has to carry the extension's client header, so a web page
//! cannot start a download with an `<img>` tag. The endpoint the browser uses
//! is also the one the extension's popup uses to read progress and to pause,
//! resume or stop a download.
//!
//! The contract is versioned: the extension says which API it expects in the
//! `X-BDM-Client` value and falls back to an offline state when the ping does
//! not name this application. The Rust side of the contract is tested in this
//! module.

mod dest;
mod http;

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use idm_core::downloader::Status;
use idm_core::log;

use crate::state::AppState;

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

/// The header every request has to carry. A web page cannot set it on a
/// cross-origin request without a preflight, and the preflight is refused
/// unless the origin is the extension's.
const CLIENT_HEADER: &str = "x-bdm-client";
const CLIENT_VALUE: &str = "browser-extension";

/// The event the window listens to for a hand-over that asks the user first.
const DOWNLOAD_EVENT: &str = "bridge://download";

/// Start the server. It runs until the application ends.
pub fn start(app: AppHandle) {
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
        log::write(
            "info",
            &format!("the browser bridge waits on 127.0.0.1:{PORT}"),
        );

        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = serve(app, stream).await;
            });
        }
    });
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

    let origin = request.header("origin").map(str::to_string);

    // A page in the browser must not reach the bridge. An extension page may,
    // and a local process without an origin may, which is what a native host
    // or a test looks like.
    if let Some(origin) = origin.as_deref() {
        if !is_extension_origin(origin) {
            return refuse(
                &mut stream,
                403,
                "forbidden",
                "this origin is not the extension",
            )
            .await;
        }
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
            origin.as_deref(),
            &headers,
        )
        .await;
    }

    // The client header is the second lock: an `<img>` or a form can send a
    // simple request without an origin, but it cannot add this header.
    if request.header(CLIENT_HEADER) != Some(CLIENT_VALUE) {
        return refuse(
            &mut stream,
            403,
            "forbidden",
            "the browser extension header is missing",
        )
        .await;
    }

    let (status, body) = route(&app, &request).await;
    let bytes = serde_json::to_vec(&body).unwrap_or_else(|_| {
        serde_json::to_vec(&error_body("internal", "the answer could not be written"))
            .unwrap_or_default()
    });
    http::write(
        &mut stream,
        status,
        "application/json",
        &bytes,
        origin.as_deref(),
        &[],
    )
    .await
}

/// The routes of API version 1.
async fn route(app: &AppHandle, request: &Request) -> (u16, Value) {
    let path = request.path();
    match (request.method.as_str(), path) {
        ("GET", "/v1/ping") => (200, ok(ping())),
        ("GET", "/v1/downloads") => (200, ok(downloads(app))),
        ("POST", "/v1/downloads") => add_download(app, &request.body).await,
        ("POST", "/v1/focus") => {
            crate::show_window(app);
            (200, ok(json!({ "focused": true })))
        }
        ("POST", _) => match control_route(path) {
            Some((id, action)) => control(app, id, action),
            None => (404, error_body("not_found", "no such route")),
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

/// What the extension says about itself.
fn ping() -> Value {
    json!({
        "app": APP_ID,
        "api": API_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "port": PORT,
    })
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

/// Start a download, or open the add dialog when the user asked to confirm.
async fn add_download(app: &AppHandle, body: &[u8]) -> (u16, Value) {
    let request: AddRequest = match serde_json::from_slice(body) {
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

    let referer = if is_web_url(request.referer.trim()) {
        request.referer.trim().to_string()
    } else {
        String::new()
    };

    let name = dest::sanitize(&request.name)
        .or_else(|| dest::name_from_url(&url))
        .unwrap_or_else(|| "download".to_string());

    match request.mode {
        Mode::Ask => {
            crate::show_window(app);
            let _ = app.emit(
                DOWNLOAD_EVENT,
                json!({ "url": url, "name": name, "referer": referer }),
            );
            log::write("info", &format!("the browser asked for {url}"));
            (200, ok(json!({ "mode": "ask" })))
        }
        Mode::Direct => {
            let state = app.state::<AppState>();
            let folder = state
                .browser_folder()
                .or_else(|| app.path().download_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            let target = dest::unique(&folder, &name);
            let referer = if referer.is_empty() {
                None
            } else {
                Some(referer)
            };

            match crate::actions::start(
                app,
                &state,
                url.clone(),
                target.clone(),
                crate::actions::StartOptions {
                    referer,
                    // A hand-over has no dialog to ask, so it never replaces
                    // a file: it takes the next free name beside it.
                    conflict: crate::conflict::Choice::Rename,
                    ..crate::actions::StartOptions::default()
                },
            ) {
                Ok(started) => {
                    let id = started.id;
                    log::write("info", &format!("the browser started {url} as {id:?}"));
                    (
                        200,
                        ok(json!({
                            "mode": "direct",
                            "id": id,
                            "name": Path::new(&started.dest).file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or(name),
                            "dest": started.dest,
                        })),
                    )
                }
                Err(refusal) => (400, error_body("bad_request", refusal.message)),
            }
        }
    }
}

/// Pause, resume, stop or show one download.
fn control(app: &AppHandle, id: u64, action: &str) -> (u16, Value) {
    let state = app.state::<AppState>();
    let result = match action {
        "pause" => crate::actions::pause(app, &state, id),
        "resume" => crate::actions::resume(app, &state, id),
        "cancel" => crate::actions::cancel(app, &state, id),
        "show" => {
            let Some(download) = state.get(id) else {
                return (404, error_body("not_found", "unknown download"));
            };
            let info = download.info();
            let handle = app.clone();
            match app.run_on_main_thread(move || crate::progress::show(&handle, &info)) {
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

/// An extension origin. Firefox uses `moz-extension://`, Chromium
/// `chrome-extension://`; the older extension build is still recognised.
fn is_extension_origin(origin: &str) -> bool {
    origin.starts_with("moz-extension://") || origin.starts_with("chrome-extension://")
}

/// A URL the engine may fetch.
fn is_download_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ftp://")
}

/// A URL that is a web page, which is what a referrer may be.
fn is_web_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// A successful answer.
fn ok(data: Value) -> Value {
    json!({ "ok": true, "data": data })
}

/// A refusal.
fn error_body(code: &str, message: impl Into<String>) -> Value {
    json!({ "ok": false, "error": { "code": code, "message": message.into() } })
}

/// Answer a request with a JSON error.
async fn refuse(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    code: &str,
    message: &str,
) -> std::io::Result<()> {
    let body = serde_json::to_vec(&error_body(code, message)).unwrap_or_default();
    http::write(stream, status, "application/json", &body, None, &[]).await
}

/// What the extension asks for when it starts a download.
#[derive(Debug, Deserialize)]
struct AddRequest {
    url: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    referer: String,
    #[serde(default)]
    mode: Mode,
}

/// How a hand-over is started: at once, or after the user confirms it in the
/// application's add dialog.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Mode {
    #[default]
    Direct,
    Ask,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extension_origin_is_recognised() {
        assert!(is_extension_origin("moz-extension://3c1e8f2a"));
        assert!(is_extension_origin("chrome-extension://abc"));
        assert!(!is_extension_origin("https://example.org"));
        assert!(!is_extension_origin("null"));
    }

    #[test]
    fn a_page_cannot_be_a_download_source() {
        assert!(is_download_url("https://example.org/a.bin"));
        assert!(!is_download_url("blob:https://example.org/abc"));
        assert!(!is_download_url("javascript:alert(1)"));
        assert!(!is_download_url("file:///C:/Windows/notepad.exe"));
    }

    #[test]
    fn a_control_route_names_a_download_and_an_action() {
        assert_eq!(control_route("/v1/downloads/42/pause"), Some((42, "pause")));
        assert_eq!(
            control_route("/v1/downloads/42/resume"),
            Some((42, "resume"))
        );
        assert_eq!(
            control_route("/v1/downloads/42/cancel"),
            Some((42, "cancel"))
        );
        assert_eq!(control_route("/v1/downloads/42/show"), Some((42, "show")));
        assert_eq!(control_route("/v1/downloads/42"), None);
        assert_eq!(control_route("/v1/downloads/42/pause/extra"), None);
        assert_eq!(control_route("/v1/downloads/not-a-number/pause"), None);
        assert_eq!(control_route("/v1/other/42/pause"), None);
    }

    #[test]
    fn the_body_defaults_to_a_direct_handover() {
        let request: AddRequest =
            serde_json::from_str(r#"{"url":"https://example.org/a.bin"}"#).expect("a body");
        assert_eq!(request.mode, Mode::Direct);
        assert_eq!(request.name, "");
        assert_eq!(request.referer, "");
    }

    #[test]
    fn an_explicit_ask_is_read() {
        let request: AddRequest = serde_json::from_str(
            r#"{"url":"https://example.org/a.bin","name":"a.bin","referer":"https://example.org/","mode":"ask"}"#,
        )
        .expect("a body");
        assert_eq!(request.mode, Mode::Ask);
        assert_eq!(request.name, "a.bin");
        assert!(is_web_url(&request.referer));
    }

    #[test]
    fn the_answers_carry_an_ok_flag() {
        assert_eq!(ok(json!({"id": 1}))["ok"], true);
        let failure = error_body("bad_request", "no");
        assert_eq!(failure["ok"], false);
        assert_eq!(failure["error"]["code"], "bad_request");
    }

    #[test]
    fn the_ping_names_this_application() {
        let answer = ping();
        assert_eq!(answer["app"], APP_ID);
        assert_eq!(answer["api"], API_VERSION);
        assert_eq!(answer["port"], PORT);
    }
}
