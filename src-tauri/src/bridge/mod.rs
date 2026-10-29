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

pub(crate) mod batch;
pub(crate) mod dest;
mod http;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

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

    // Only an authenticated request counts as the extension having reached
    // the application, so a stray page cannot make the window say it is
    // connected.
    app.state::<AppState>().touch_extension();

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
        ("GET", "/v1/capture") => (200, ok(capture_policy(app))),
        ("GET", "/v1/video-button-settings") => (200, ok(video_button_settings(app))),
        ("GET", "/v1/downloads") => (200, ok(downloads(app))),
        ("POST", "/v1/downloads") => add_download(app, &request.body).await,
        ("POST", "/v1/downloads/mux") => mux_download(app, &request.body).await,
        ("GET", "/v1/media/tools") => media_tools(app).await,
        ("GET", "/v1/extract-formats") => extract_formats(app, &request).await,
        ("GET", "/v1/batch-options") => (200, ok(batch::options(app, &app.state::<AppState>()))),
        ("POST", "/v1/downloads/batch") => batch::add(app, &app.state::<AppState>(), &request.body),
        ("POST", "/v1/focus") => {
            crate::tray::show_window(app);
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

/// Which bundled media tools can run, so the extension knows whether muxing
/// and quality selection are available.
async fn media_tools(app: &AppHandle) -> (u16, Value) {
    let tools = crate::media::media_tools(app).await;
    (
        200,
        ok(serde_json::to_value(tools).unwrap_or(Value::Null)),
    )
}

/// Enumerate the formats of a video page for the extension's quality panel.
async fn extract_formats(app: &AppHandle, request: &Request) -> (u16, Value) {
    let Some(url) = query_param(request.target.as_str(), "url") else {
        return (
            400,
            error_body("bad_request", "a url query parameter is required"),
        );
    };
    if !is_web_url(&url) {
        return (
            400,
            error_body("bad_request", "the URL must start with http:// or https://"),
        );
    }
    let proxy = app.state::<AppState>().limits().proxy();
    match crate::media::extract::extract(app, proxy.as_deref(), &url).await {
        Ok(formats) => (200, ok(json!({ "formats": formats }))),
        Err(message) => (422, error_body("extraction_failed", message)),
    }
}

/// Download and mux a video and an audio stream the extension picked.
///
/// The streams are the fresh, signed addresses the extractor returned, so the
/// download does not depend on a URL the page captured hours earlier. The mux
/// runs to completion before the bridge answers, so a successful answer means
/// the file is already on disk and has been probed as playable.
async fn mux_download(app: &AppHandle, body: &[u8]) -> (u16, Value) {
    let request: MuxRequest = match serde_json::from_slice(body) {
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

    let video_url = request.video_url.trim().to_string();
    if !is_web_url(&video_url) {
        return (
            400,
            error_body("bad_request", "the video stream is not a web address"),
        );
    }
    let audio_url = request.audio_url.trim().to_string();
    if !audio_url.is_empty() && !is_web_url(&audio_url) {
        return (
            400,
            error_body("bad_request", "the audio stream is not a web address"),
        );
    }
    let container = request
        .container
        .trim()
        .to_ascii_lowercase()
        .trim_start_matches('.')
        .to_string();
    if !matches!(container.as_str(), "mp4" | "webm" | "mkv") {
        return (
            400,
            error_body("bad_request", "the container must be mp4, webm or mkv"),
        );
    }

    let state = app.state::<AppState>();
    let folder = state
        .browser_folder()
        .or_else(|| app.path().download_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let cleaned = dest::sanitize(&request.name).unwrap_or_else(|| "video".to_string());
    let file_name = if cleaned
        .to_ascii_lowercase()
        .ends_with(&format!(".{container}"))
    {
        cleaned
    } else {
        format!("{cleaned}.{container}")
    };
    let target = dest::unique(&folder, &file_name);

    let proxy = state.limits().proxy();
    match crate::media::mux::mux(
        app,
        proxy.as_deref(),
        &video_url,
        &audio_url,
        &target,
        &container,
    )
    .await
    {
        Ok(()) => (
            200,
            ok(json!({
                "dest": target.to_string_lossy().to_string(),
                "name": target.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or(file_name),
            })),
        ),
        Err(message) => (422, error_body("mux_failed", message)),
    }
}

/// What the extension asks for when it starts a mux download.
#[derive(Debug, Deserialize)]
struct MuxRequest {
    video_url: String,
    #[serde(default)]
    audio_url: String,
    #[serde(default)]
    container: String,
    #[serde(default)]
    name: String,
}

/// Read one query parameter out of a request target, percent escapes decoded.
fn query_param(target: &str, name: &str) -> Option<String> {
    let query = target.split_once('?').map(|(_, query)| query)?;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == name {
            return Some(percent_decode(value));
        }
    }
    None
}

/// Decode the percent escapes of a query value.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Start a download, or open the add dialog when the user asked to confirm.
///
/// A direct hand-over that finds its destination already taken — the file is
/// on disk, or another row is writing it — becomes an ask as well: the window
/// shows the file, and the engine only starts after the answer. The extension
/// has already been told the app took the link, so the browser copy is stopped
/// and the download goes through the dialog instead of the browser.
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
            ask_window(app, &url, &name, &referer, "the browser asked");
            (200, ok(json!({ "mode": "ask" })))
        }
        Mode::Direct => {
            let state = app.state::<AppState>();
            let folder = state
                .browser_folder()
                .or_else(|| app.path().download_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));

            // The destination is looked at before anything is started: a file
            // that is already there is a question for the window, not a name
            // to talk around with a silent rename.
            let target = match plan_direct(&state, &folder, &name) {
                DirectPlan::Taken { dest } => {
                    ask_window(
                        app,
                        &url,
                        &name,
                        &referer,
                        &format!("{dest} is already there", dest = dest.display()),
                    );
                    return (200, ok(json!({ "mode": "ask" })));
                }
                DirectPlan::Free { dest } => dest,
            };
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
                    // The hand-over has no dialog, so it follows the default
                    // connection count the options window pushed.
                    connections: Some(state.browser_connections()),
                    // A hand-over has no dialog to ask, so it never replaces
                    // a file: it takes the next free name beside it.
                    conflict: crate::conflict::Choice::Rename,
                    // Nor does it have a folder field: the folder the file's
                    // category saves to decides where it lands.
                    use_category_folder: true,
                    ..crate::actions::StartOptions::default()
                },
            ) {
                Ok(started) => {
                    crate::show_progress_if_requested(app, &state, &started);
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

/// Show the window and hand it the link to ask about.
///
/// The answer comes back through the window's own start command, so the File
/// Exists question — rename, replace or cancel — is settled from there.
fn ask_window(app: &AppHandle, url: &str, name: &str, referer: &str, why: &str) {
    crate::tray::show_window(app);
    let _ = app.emit(
        DOWNLOAD_EVENT,
        json!({ "url": url, "name": name, "referer": referer }),
    );
    log::write(
        "info",
        &format!("the bridge asked the window for {url}: {why}"),
    );
}

/// What a direct hand-over may do with its destination.
enum DirectPlan {
    /// Nothing is in the way: the engine can start at once.
    Free { dest: PathBuf },
    /// A file is already there, or another row is writing that name: the
    /// window has to ask before anything starts.
    Taken { dest: PathBuf },
}

/// Where a direct hand-over would write, and whether that name is free.
///
/// The category's folder is applied first, exactly as [`crate::actions::start`]
/// applies it, so the question is about the file that would really be written
/// rather than one beside the wrong folder. An invalid destination is left for
/// the start to refuse, as before; `conflict::inspect` only answers for
/// destinations the engine can use.
fn plan_direct(state: &AppState, folder: &Path, name: &str) -> DirectPlan {
    let (target, _) = crate::actions::category_destination(state, folder.join(name), None, true);
    let Ok(status) = crate::conflict::inspect(state, &target) else {
        return DirectPlan::Free { dest: target };
    };
    if status.file_exists || status.active {
        DirectPlan::Taken { dest: target }
    } else {
        DirectPlan::Free { dest: target }
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

    #[test]
    fn a_query_parameter_is_read_and_decoded() {
        assert_eq!(
            query_param("/v1/extract-formats?url=https%3A%2F%2Fexample.org%2Fa%3Fb%3D1", "url"),
            Some("https://example.org/a?b=1".to_string())
        );
        assert_eq!(
            query_param("/v1/extract-formats?url=x&other=y", "url"),
            Some("x".to_string())
        );
        assert_eq!(query_param("/v1/extract-formats", "url"), None);
        assert_eq!(query_param("/v1/extract-formats?other=y", "url"), None);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "my-idm-clone-bridge-plan-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn row(id: u64, dest: &Path) -> std::sync::Arc<idm_core::downloader::Download> {
        idm_core::downloader::Download::new(
            id,
            "https://example.org/film.mp4".to_string(),
            dest.to_path_buf(),
            4,
            None,
            None,
        )
        .expect("a download")
    }

    #[test]
    fn a_file_that_is_already_there_sends_the_handover_to_the_window() {
        let dir = temp_dir("exists");
        std::fs::write(dir.join("film.mp4"), b"old").expect("write");
        let state = AppState::default();

        match plan_direct(&state, &dir, "film.mp4") {
            DirectPlan::Taken { dest } => {
                assert_eq!(
                    dest,
                    dir.join("film.mp4"),
                    "the question is about the real file"
                );
            }
            DirectPlan::Free { .. } => {
                panic!("a taken name is a question, not a silent rename")
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_free_name_starts_without_a_question() {
        let dir = temp_dir("free");
        let state = AppState::default();

        match plan_direct(&state, &dir, "film.mp4") {
            DirectPlan::Free { dest } => assert_eq!(dest, dir.join("film.mp4")),
            DirectPlan::Taken { .. } => panic!("nothing holds that name"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_another_row_is_writing_is_a_question_too() {
        let dir = temp_dir("active");
        let state = AppState::default();
        state.insert(row(1, &dir.join("film.mp4")));

        // The file itself is not on disk yet — a running download keeps parts
        // elsewhere — but the name belongs to that row.
        match plan_direct(&state, &dir, "film.mp4") {
            DirectPlan::Taken { .. } => {}
            DirectPlan::Free { dest } => panic!("that name is held, but it starts at {dest:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_question_follows_the_category_folder() {
        let dir = temp_dir("category");
        let films = dir.join("Films");
        std::fs::create_dir_all(&films).expect("folder");
        std::fs::write(films.join("film.mp4"), b"old").expect("write");

        let state = AppState::default();
        state.set_download_folders(
            None,
            vec![crate::state::CategoryRule {
                name: "Films".to_string(),
                extensions: vec!["mp4".to_string()],
                folder: films.to_string_lossy().to_string(),
                builtin: false,
                fallback: false,
            }],
        );

        // The Downloads folder is free; the folder the file's category saves
        // to is not. The question must be about the file that would really be
        // written.
        match plan_direct(&state, &dir, "film.mp4") {
            DirectPlan::Taken { dest } => assert_eq!(dest, films.join("film.mp4")),
            DirectPlan::Free { dest } => panic!("the category folder is taken, not {dest:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
