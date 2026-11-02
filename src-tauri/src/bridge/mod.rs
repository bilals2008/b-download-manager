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

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

use idm_core::downloader::Status;
use idm_core::log;

use crate::state::AppState;

use auth::Denial;

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
        ("GET", "/v1/capture") => (200, ok(capture_policy(app))),
        ("POST", "/v1/capture") => set_capture_policy(app, &request.body),
        ("GET", "/v1/video-button-settings") => (200, ok(video_button_settings(app))),
        ("POST", "/v1/media") => crate::video::capture::register(app, &request.body).await,
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
        "resume" => crate::actions::resume(app, &state, id, false),
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

/// A URL the engine may fetch. It has to name a scheme the engine speaks and a
/// non-empty host, so `http://` alone is not a download.
fn is_download_url(url: &str) -> bool {
    is_url(url, &["http", "https", "ftp"])
}

/// A URL that is a web page, which is what a referrer may be.
fn is_web_url(url: &str) -> bool {
    is_url(url, &["http", "https"])
}

/// The largest address the bridge accepts. A real download link is far
/// shorter; the limit stops a hand-over from carrying an unbounded string.
const MAX_URL: usize = 8 * 1024;

/// Whether `url` uses one of `schemes` and has a host to fetch.
fn is_url(url: &str, schemes: &[&str]) -> bool {
    if url.is_empty() || url.len() > MAX_URL {
        return false;
    }
    if url
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return false;
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    if !schemes
        .iter()
        .any(|known| scheme.eq_ignore_ascii_case(known))
    {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !authority.is_empty() && !authority.starts_with(':')
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

    const EXTENSION_ORIGIN: &str = "moz-extension://3c1e8f2a-1";

    fn request(method: &str, path: &str, headers: &[(&str, &str)]) -> Request {
        let mut all = vec![("host", "127.0.0.1:27500")];
        all.extend_from_slice(headers);
        Request::for_test(method, path, &all, b"")
    }

    fn extension(method: &str, path: &str, origin: &str) -> Request {
        request(
            method,
            path,
            &[("origin", origin), ("x-bdm-client", auth::CLIENT_VALUE)],
        )
    }

    #[test]
    fn an_extension_is_recognised_without_any_code() {
        let gate = gate(&extension("GET", "/v1/downloads", EXTENSION_ORIGIN));
        assert_eq!(gate.denial, None);
        assert!(gate.extension);
        assert_eq!(gate.cors.as_deref(), Some(EXTENSION_ORIGIN));
    }

    #[test]
    fn a_download_without_the_extension_header_is_refused() {
        let gate = gate(&request(
            "POST",
            "/v1/downloads",
            &[("origin", EXTENSION_ORIGIN)],
        ));
        assert_eq!(gate.denial.map(|denial| denial.status), Some(403));
        assert!(!gate.extension);
    }

    #[test]
    fn the_ping_is_open_and_still_names_the_application() {
        let gate = gate(&extension("GET", "/v1/ping", EXTENSION_ORIGIN));
        assert_eq!(gate.denial, None);
        assert!(gate.extension, "the popup can say it is connected");
    }

    #[test]
    fn a_preflight_passes_but_does_not_count_as_a_contact() {
        let gate = gate(&request(
            "OPTIONS",
            "/v1/downloads",
            &[("origin", EXTENSION_ORIGIN)],
        ));
        assert_eq!(gate.denial, None);
        assert!(
            !gate.extension,
            "a preflight is not the browser reaching us"
        );
    }

    #[test]
    fn a_local_caller_without_an_origin_is_answered_but_not_called_connected() {
        let gate = gate(&request(
            "GET",
            "/v1/downloads",
            &[("x-bdm-client", auth::CLIENT_VALUE)],
        ));
        assert_eq!(gate.denial, None, "no origin is nothing to refuse");
        assert!(!gate.extension);
        assert_eq!(gate.cors, None, "and it gets no CORS header back");
    }

    #[test]
    fn a_page_or_a_wrong_host_is_refused() {
        let page = gate(&extension("GET", "/v1/downloads", "https://example.org"));
        assert_eq!(page.denial.map(|denial| denial.status), Some(403));
        assert_eq!(page.cors, None, "a page origin is never echoed back");

        let rebinding = Request::for_test(
            "GET",
            "/v1/downloads",
            &[
                ("host", "evil.example:27500"),
                ("origin", EXTENSION_ORIGIN),
                ("x-bdm-client", auth::CLIENT_VALUE),
            ],
            b"",
        );
        let gate = gate(&rebinding);
        assert_eq!(gate.denial.map(|denial| denial.status), Some(403));
    }

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
        assert!(is_download_url("FTP://example.org/a.bin"));
        assert!(!is_download_url("http://"));
        assert!(!is_download_url("https:///a.bin"));
        assert!(!is_download_url("https://example.org/a b.bin"));
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
    fn a_media_route_names_a_job_and_an_action() {
        assert_eq!(media_route("/v1/media/7"), Some((7, None)));
        assert_eq!(
            media_route("/v1/media/7/download"),
            Some((7, Some("download")))
        );
        assert_eq!(media_route("/v1/media/7/cancel"), Some((7, Some("cancel"))));
        assert_eq!(media_route("/v1/media/7/download/x"), None);
        assert_eq!(media_route("/v1/media/nope"), None);
        assert_eq!(media_route("/v1/downloads/7"), None);
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
        let answer = ping(false);
        assert_eq!(answer["app"], APP_ID);
        assert_eq!(answer["api"], API_VERSION);
        assert_eq!(answer["port"], PORT);
        assert_eq!(answer["connected"], false);
        assert_eq!(ping(true)["connected"], true);
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
