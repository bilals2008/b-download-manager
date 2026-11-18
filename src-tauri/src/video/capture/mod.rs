//! The bridge handlers of the media path, and the panel's own commands.
//!
//! The extension registers a manifest; parsing happens on a task of its own so
//! the bridge answer is immediate. The panel then asks for the job, chooses a
//! pair and starts the download. Every address that is fetched goes through
//! [`crate::grabber::policy::check_public`] in the probe.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::bridge::{dest, error_body, ok};
use crate::state::AppState;

use super::{MediaJob, MediaJobView, MediaKindHint, MediaState};

mod helpers;
mod parsing;

use helpers::*;
use parsing::*;

/// What the extension posts when a player offers a manifest.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRequest {
    #[serde(default)]
    page_url: String,
    manifest_url: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    referer: String,
    #[serde(default)]
    headers: std::collections::HashMap<String, String>,
    /// `inline` means the browser extension draws the quality list; the app
    /// must not open its own window.
    #[serde(default)]
    presentation: String,
}

/// What the panel posts when a quality is chosen.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub video: String,
    #[serde(default)]
    pub audio: Option<String>,
    #[serde(default)]
    pub name: String,
    /// The folder the file goes to. Empty means the browser folder.
    #[serde(default)]
    pub dest: String,
    /// The caption rendition the reader chose, when any.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// A caption file the reader picked from their own disk, when any. It
    /// wins over `subtitle`.
    #[serde(default)]
    pub subtitle_file: Option<String>,
    /// What the sound is saved as: nothing, or `ba` (the track as it came),
    /// or a conversion. Empty means an ordinary download.
    #[serde(default)]
    pub audio_format: Option<String>,
}

/// The folder the reader picked, when they picked one. An empty choice is
/// left to the start, which routes the file by its category once the
/// finished name is known.
fn picked_folder(dest: &str) -> Option<PathBuf> {
    let trimmed = dest.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

/// `POST /v1/media`
pub async fn register(app: &AppHandle, body: &[u8]) -> (u16, Value) {
    let request: MediaRequest = match serde_json::from_slice(body) {
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

    let manifest_url = request.manifest_url.trim().to_string();
    if !is_web_url(&manifest_url) {
        return (
            400,
            error_body(
                "bad_request",
                "the manifest address must start with http:// or https://",
            ),
        );
    }
    let page_url = request.page_url.trim().to_string();
    if !page_url.is_empty() && !is_web_url(&page_url) {
        return (
            400,
            error_body("bad_request", "the page address is not a web address"),
        );
    }
    let referer = if is_web_url(request.referer.trim()) {
        request.referer.trim().to_string()
    } else {
        String::new()
    };
    let kind = match request.kind.trim().to_ascii_lowercase().as_str() {
        "hls" => MediaKindHint::Hls,
        "dash" => MediaKindHint::Dash,
        "file" => MediaKindHint::File,
        "youtube" => MediaKindHint::Youtube,
        _ => MediaKindHint::Auto,
    };

    let state = app.state::<AppState>();
    let host = super::host_of(&page_url, &manifest_url);
    let preferred = state.video_quality(&host);
    let inline = request.presentation.eq_ignore_ascii_case("inline");

    // Opening the quality list again asks for the same manifest a second time.
    // The live job already holds it, so it is returned instead of creating a
    // twin that the merge below would then have to retire.
    if let Some(existing) = state.media_job_for_manifest(&manifest_url) {
        state.update_media_job(existing, |job| job.inline = inline);
        return (202, ok(json!({ "job": existing })));
    }

    // The headers are scoped here and nowhere else: allowed names only, the
    // manifest's origin only. A cookie or an authorization value waits for
    // the one-time consent before it is used.
    let scoped = super::policy::scope(&manifest_url, &request.headers);
    let consent = state.preferences().cookie_consent;
    let sensitive = scoped.sensitive();
    let active = if sensitive && !consent {
        Vec::new()
    } else {
        scoped.headers.clone()
    };

    let id = state.next_media_id();
    let mut job = MediaJob::new(
        id,
        page_url,
        manifest_url,
        kind,
        clean_title(&request.title),
        referer,
        active,
        preferred,
    );
    job.header_origin = scoped.origin;
    job.offered_headers = scoped.headers;
    job.cookies_offered = sensitive;
    job.cookie_consent = consent;
    job.inline = inline;
    state.insert_media_job(job);

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        parse_job(handle, id).await;
    });

    (202, ok(json!({ "job": id })))
}

/// `GET /v1/media/{job}`
pub fn view(app: &AppHandle, id: u64) -> (u16, Value) {
    match app.state::<AppState>().media_job(id) {
        Some(job) => (200, ok(json!(job.view()))),
        None => (404, error_body("not_found", "unknown media job")),
    }
}

/// `POST /v1/media/{job}/download`
pub async fn download(app: &AppHandle, id: u64, body: &[u8]) -> (u16, Value) {
    let request: DownloadRequest = match serde_json::from_slice(body) {
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
    let Some(job) = state.media_job(id) else {
        return (404, error_body("not_found", "unknown media job"));
    };
    if job.state != MediaState::Ready {
        return (
            409,
            error_body("not_ready", "the manifest has not been read yet"),
        );
    }
    if job.downloading {
        return (
            409,
            error_body("busy", "this video is already being downloaded"),
        );
    }

    // An empty folder leaves the choice to the start, which routes the file
    // by its category once the finished name is known. A folder the reader
    // picked is checked there too.
    let folder = picked_folder(&request.dest);

    let name = dest::sanitize(&request.name)
        .filter(|name| !name.contains("open the failure details"))
        .or_else(|| dest::sanitize(&job.title))
        .unwrap_or_else(|| "video".to_string());

    match super::download::start(app, &job, &request, folder, name) {
        Ok(started) => (
            200,
            ok(json!({
                "downloads": [started.id],
                "download": started.id,
                "dest": started.dest,
            })),
        ),
        Err(message) => (400, error_body("bad_request", message)),
    }
}

/// `POST /v1/media/{job}/cancel`
pub fn cancel(app: &AppHandle, id: u64) -> (u16, Value) {
    let state = app.state::<AppState>();
    let Some(job) = state.media_job(id) else {
        return (404, error_body("not_found", "unknown media job"));
    };
    if let Some(control) = &job.control {
        control.cancel();
    }
    if let Some(dest) = job.dest.as_deref() {
        super::mux::cancel(Path::new(dest));
    }
    state.update_media_job(id, |job| {
        job.stage = Some("Stopped".to_string());
        job.downloading = false;
    });
    state.save_media_index();
    (200, ok(json!({ "job": id, "state": "stopped" })))
}

/* --------------------------------------------------------------- the panel */

#[tauri::command]
pub fn media_job(app: AppHandle, job: u64) -> Result<MediaJobView, String> {
    app.state::<AppState>()
        .media_job(job)
        .map(|job| job.view())
        .ok_or_else(|| "unknown media job".to_string())
}

#[tauri::command]
pub fn media_download(
    app: AppHandle,
    job: u64,
    request: DownloadRequest,
) -> Result<MediaJobView, String> {
    let state = app.state::<AppState>();
    let media = state
        .media_job(job)
        .ok_or_else(|| "unknown media job".to_string())?;
    if media.state != MediaState::Ready {
        return Err("the manifest has not been read yet".to_string());
    }
    let folder = picked_folder(&request.dest);
    let name = dest::sanitize(&request.name)
        .filter(|name| !name.contains("open the failure details"))
        .or_else(|| dest::sanitize(&media.title))
        .unwrap_or_else(|| "video".to_string());
    super::download::start(&app, &media, &request, folder, name)?;
    state
        .media_job(job)
        .map(|job| job.view())
        .ok_or_else(|| "unknown media job".to_string())
}

/// The bridge's form of the consent action.
pub fn allow_cookies_bridge(app: &AppHandle, job: u64) -> (u16, Value) {
    match media_allow_cookies(app.clone(), job) {
        Ok(view) => (200, ok(json!(view))),
        Err(message) => (404, error_body("not_found", message)),
    }
}

/// Every media job, for the home list. A job that was never started is not
/// shown there; the list ignores it.
#[tauri::command]
pub fn media_jobs(app: AppHandle) -> Vec<MediaJobView> {
    app.state::<AppState>()
        .media_jobs_snapshot()
        .iter()
        .map(|job| job.view())
        .collect()
}

/// Forget a media job. The file is deleted only when the caller asked for it;
/// the unfinished folder goes either way.
#[tauri::command]
pub fn media_forget(app: AppHandle, job: u64, delete_file: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let Some(media) = state.media_job(job) else {
        return Ok(());
    };
    if let Some(control) = &media.control {
        control.cancel();
    }
    if let Some(dest) = media.dest.as_deref() {
        let dest = Path::new(dest);
        super::mux::cancel(dest);
        if delete_file {
            match std::fs::remove_file(dest) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("the file could not be deleted: {error}")),
            }
        }
    }
    if let Some(work_dir) = media.work_dir.as_deref() {
        let work_dir = std::path::Path::new(work_dir);
        if work_dir
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("video-"))
        {
            let _ = std::fs::remove_dir_all(work_dir);
        }
    }
    state.remove_media_job(job);
    // A removed row's folder is gone, and so is any older one no live job
    // claims: the media tree and the list agree again.
    state.sweep_media_folders();
    state.save_media_index();
    Ok(())
}

/// Allow the browser's cookies for every site, once, and read the job again.
#[tauri::command]
pub fn media_allow_cookies(app: AppHandle, job: u64) -> Result<MediaJobView, String> {
    let state = app.state::<AppState>();
    let media = state
        .media_job(job)
        .ok_or_else(|| "unknown media job".to_string())?;
    state.set_cookie_consent(true);
    state.update_media_job(job, |job| {
        job.cookie_consent = true;
        job.headers = job.offered_headers.clone();
        job.state = MediaState::Parsing;
        job.error = None;
        job.manifest = None;
        job.source = None;
        job.notice = None;
        job.downloading = false;
        job.done = false;
        job.progress = Default::default();
    });
    state.save_media_index();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        parse_job(handle, job).await;
    });
    state
        .media_job(media.id)
        .map(|job| job.view())
        .ok_or_else(|| "unknown media job".to_string())
}

#[tauri::command]
pub fn media_cancel(app: AppHandle, job: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let media = state
        .media_job(job)
        .ok_or_else(|| "unknown media job".to_string())?;
    if let Some(control) = &media.control {
        control.cancel();
    }
    if let Some(dest) = media.dest.as_deref() {
        super::mux::cancel(Path::new(dest));
    }
    state.update_media_job(job, |job| {
        job.stage = Some("Stopped".to_string());
        job.downloading = false;
    });
    state.save_media_index();
    Ok(())
}

/// Continue a stopped or failed media download from the bytes on disk.
#[tauri::command]
pub fn media_resume(app: AppHandle, job: u64) -> Result<MediaJobView, String> {
    let state = app.state::<AppState>();
    let media = state
        .media_job(job)
        .ok_or_else(|| "unknown media job".to_string())?;
    super::download::resume(&app, &media)?;
    state
        .media_job(job)
        .map(|job| job.view())
        .ok_or_else(|| "unknown media job".to_string())
}

/// Read the page again, for example after the video helper was installed.
#[tauri::command]
pub fn media_reparse(app: AppHandle, job: u64) -> Result<MediaJobView, String> {
    let state = app.state::<AppState>();
    let media = state
        .media_job(job)
        .ok_or_else(|| "unknown media job".to_string())?;
    state.update_media_job(job, |job| {
        job.state = MediaState::Parsing;
        job.error = None;
        job.notice = None;
        job.manifest = None;
        job.source = None;
        job.progress = Default::default();
        job.fraction = 0.0;
    });
    state.save_media_index();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        parse_job(handle, job).await;
    });
    state
        .media_job(media.id)
        .map(|job| job.view())
        .ok_or_else(|| "unknown media job".to_string())
}

/// Show the progress window of a video download, or bring it to the front.
///
/// Async on purpose: the window is built on the main thread, the one the
/// window toolkit runs on, and a synchronous command runs on that same
/// thread — there `run_on_main_thread` runs the build inline, inside the
/// webview's IPC handler, and WebView2 never finishes creating the window.
#[tauri::command(async)]
pub fn open_media_progress(app: AppHandle, job: u64) -> Result<(), String> {
    if app.state::<AppState>().media_job(job).is_none() {
        return Err("unknown media job".to_string());
    }
    let handle = app.clone();
    app.run_on_main_thread(move || crate::progress::show_media(&handle, job, true))
        .map_err(|error| error.to_string())
}

/// Take a finished video again. The new file gets its own name beside the
/// old one; the page is not read again, and the quality is the one that was
/// chosen the first time.
#[tauri::command]
pub fn media_redownload(app: AppHandle, job: u64) -> Result<MediaJobView, String> {
    let state = app.state::<AppState>();
    let media = state
        .media_job(job)
        .ok_or_else(|| "unknown media job".to_string())?;
    if media.downloading {
        return Err("this video is already being downloaded".to_string());
    }
    let request = media
        .request
        .clone()
        .ok_or_else(|| "this video has not been downloaded yet".to_string())?;
    // The folder of the first run is kept when it was a real one; an empty
    // choice is routed by category again, like a fresh start.
    let folder = picked_folder(&request.dest);
    let name = dest::sanitize(&media.title).unwrap_or_else(|| "video".to_string());
    crate::video::download::start(&app, &media, &request, folder, name)?;
    state
        .media_job(job)
        .map(|job| job.view())
        .ok_or_else(|| "unknown media job".to_string())
}

/// Bring the quality panel of a media job back to the front.
///
/// Media rows in the download list are not engine downloads, so they have no
/// progress window of their own; their window is the quality panel.
///
/// Async on purpose: the window is built on the main thread, and a
/// synchronous command runs on that same thread, where the build would run
/// inline inside the IPC handler and never finish.
#[tauri::command(async)]
pub fn open_video_window(app: AppHandle, job: u64) -> Result<(), String> {
    if app.state::<AppState>().media_job(job).is_none() {
        return Err("unknown media job".to_string());
    }
    super::window::show(&app, job);
    Ok(())
}

/// Which video helpers are installed.
#[tauri::command]
pub fn video_helper_status(app: AppHandle) -> super::tools::HelperStatus {
    super::tools::status(&app)
}

/// Download and check the missing helpers, then answer what is ready.
#[tauri::command]
pub async fn video_helper_install(app: AppHandle) -> Result<super::tools::HelperStatus, String> {
    super::tools::install(&app).await?;
    Ok(super::tools::status(&app))
}

/// Keep a helper the person downloaded themselves; `true` means it was checked
/// against the publisher's list.
#[tauri::command]
pub async fn video_helper_import(app: AppHandle, path: String) -> Result<bool, String> {
    super::tools::import(&app, Path::new(&path)).await
}

/// Forget the helper the person chose; go back to the app's own copy.
#[tauri::command]
pub async fn video_helper_forget(app: AppHandle) -> Result<(), String> {
    super::tools::forget(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    use idm_core::media::{youtube, Format, MediaError, MediaKind};

    #[test]
    fn a_manifest_address_is_recognised_by_its_path() {
        assert_eq!(
            infer_kind(
                MediaKindHint::Auto,
                "https://cdn.example.org/master.m3u8?t=1"
            ),
            MediaKindHint::Hls
        );
        assert_eq!(
            infer_kind(
                MediaKindHint::Auto,
                "https://cdn.example.org/a/manifest.mpd"
            ),
            MediaKindHint::Dash
        );
        assert_eq!(
            infer_kind(MediaKindHint::Auto, "https://cdn.example.org/film.mp4"),
            MediaKindHint::File
        );
        assert_eq!(
            infer_kind(
                MediaKindHint::Auto,
                "https://rr1---sn.googlevideo.com/videoplayback?itag=137&mime=video%2Fmp4"
            ),
            MediaKindHint::File
        );
        assert_eq!(
            infer_kind(MediaKindHint::Auto, "https://cdn.example.org/stream"),
            MediaKindHint::Auto
        );
        assert_eq!(
            infer_kind(MediaKindHint::Dash, "https://cdn.example.org/master.m3u8"),
            MediaKindHint::Dash,
            "the extension may know better than the path"
        );
    }

    #[test]
    fn a_content_type_decides_the_container_and_kind() {
        assert_eq!(file_container("https://x/a", Some("video/mp4")), "mp4");
        assert_eq!(file_container("https://x/a", Some("audio/mp4")), "m4a");
        assert_eq!(file_container("https://x/a", Some("video/webm")), "webm");
        assert_eq!(file_container("https://x/a.MKV", None), "mkv");
        assert!(is_audio("m4a", None));
        assert!(!is_audio("mp4", Some("video/mp4")));
    }

    #[test]
    fn a_youtube_address_is_read_from_its_own_query() {
        let params = youtube_params(
            "https://rr1---sn.googlevideo.com/videoplayback?itag=137&clen=331000000&mime=video%2Fmp4&signature=x",
        );
        assert_eq!(params.itag, Some(137));
        assert_eq!(params.size, Some(331_000_000));
        assert_eq!(params.mime.as_deref(), Some("video/mp4"));

        let itag = youtube::describe(params.itag.expect("an itag")).expect("a row");
        assert_eq!(itag.kind, MediaKind::Video);
        assert_eq!(itag.height, Some(1080));
    }

    #[test]
    fn a_file_manifest_pairs_a_video_with_its_sound() {
        let mut video = Format::new("137", MediaKind::Video, "mp4");
        video.height = Some(1080);
        let mut hfr = Format::new("299", MediaKind::Video, "mp4");
        hfr.height = Some(1080);
        hfr.fps = Some(60.0);
        let audio = Format::new("140", MediaKind::Audio, "m4a");
        let manifest = file_manifest("Film".to_string(), vec![video, hfr, audio]);
        assert_eq!(manifest.pairs.len(), 2);
        assert_eq!(manifest.pairs[0].audio.as_deref(), Some("140"));
        assert_eq!(manifest.pairs[1].audio.as_deref(), Some("140"));
        assert_eq!(
            manifest
                .formats
                .iter()
                .find(|format| format.id == "299")
                .map(|format| format.label.as_str()),
            Some("1080p 60fps")
        );
    }

    #[test]
    fn a_video_only_file_stays_a_video_only_pair() {
        let mut video = Format::new("137", MediaKind::Video, "mp4");
        video.height = Some(1080);
        let manifest = file_manifest("Film".to_string(), vec![video]);
        assert_eq!(manifest.pairs.len(), 1);
        assert!(manifest.pairs[0].audio.is_none());
        assert_eq!(manifest.formats[0].kind, MediaKind::Video);
    }

    #[test]
    fn a_ump_content_type_is_a_transport_not_encryption() {
        let refusal = refused_content_type("application/vnd.yt-ump").expect("a refusal");
        assert!(
            matches!(refusal, MediaError::PlayerTransport(_)),
            "{refusal:?}"
        );
        let sentence = refusal.to_string();
        assert!(sentence.contains("UMP"), "{sentence}");
        assert!(!sentence.contains("DRM"), "{sentence}");

        let drm = refused_content_type("application/drm").expect("a refusal");
        assert!(drm.to_string().contains("protected"), "{drm}");

        let widevine = refused_content_type("video/vnd.widevine").expect("a refusal");
        assert!(widevine.to_string().contains("Widevine"), "{widevine}");

        assert!(refused_content_type("video/mp4").is_none());
        assert!(refused_content_type("application/x-mpegURL").is_none());
    }

    #[test]
    fn a_title_is_one_line_and_bounded() {
        let title = clean_title("  My film - YouTube  ");
        assert_eq!(title, "My film");
        assert!(clean_title(&"a".repeat(500)).chars().count() <= 200);
        assert_eq!(clean_title("a\nb"), "ab");
    }
}
