//! The bridge handlers of the media path, and the panel's own commands.
//!
//! The extension registers a manifest; parsing happens on a task of its own so
//! the bridge answer is immediate. The panel then asks for the job, chooses a
//! pair and starts the download. Every address that is fetched goes through
//! [`crate::grabber::policy::check_public`] in the probe.

use std::path::{Path, PathBuf};

use reqwest::Url;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use idm_core::media::youtube;
use idm_core::media::{dash, hls, quality, Format, Manifest, MediaError, MediaKind, Pair};

use crate::bridge::{dest, error_body, ok};
use crate::state::AppState;

use super::{FileTrack, MediaJob, MediaJobView, MediaKindHint, MediaState, SourcePlan};

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
#[derive(Debug, Clone, Deserialize)]
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

    let folder = match request.dest.trim() {
        "" => state
            .browser_folder()
            .or_else(|| app.path().download_dir().ok())
            .unwrap_or_else(|| PathBuf::from(".")),
        value => {
            let folder = PathBuf::from(value);
            if !folder.is_absolute() || !folder.is_dir() {
                return (400, error_body("bad_request", "the folder is not usable"));
            }
            folder
        }
    };

    let name = dest::sanitize(&request.name)
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
    let folder = match request.dest.trim() {
        "" => state
            .browser_folder()
            .or_else(|| app.path().download_dir().ok())
            .unwrap_or_else(|| PathBuf::from(".")),
        value => {
            let folder = PathBuf::from(value);
            if !folder.is_absolute() || !folder.is_dir() {
                return Err("the folder is not usable".to_string());
            }
            folder
        }
    };
    let name = dest::sanitize(&request.name)
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
    Ok(())
}

/* --------------------------------------------------------------- parsing */

async fn parse_job(app: AppHandle, id: u64) {
    let job = match app.state::<AppState>().media_job(id) {
        Some(job) => job,
        None => return,
    };
    let result = parse_manifest(&job).await;
    let state = app.state::<AppState>();
    match result {
        Ok((manifest, source)) => {
            // Two candidates from one page (a YouTube picture and its sound)
            // become one job. The gate is held across the check and the store,
            // so the second candidate cannot look before the first has landed.
            if let SourcePlan::Files { tracks } = &source {
                if let Some(track) = tracks.first().cloned() {
                    let gate = state.media_gate();
                    if let Some(target_id) = state.merge_file_target(id, &job.page_url) {
                        let mut merged = false;
                        state.update_media_job(target_id, |target| {
                            let Some(target_manifest) = target.manifest.as_mut() else {
                                return;
                            };
                            if target_manifest
                                .formats
                                .iter()
                                .any(|format| format.id == track.format.id)
                            {
                                merged = true;
                                return;
                            }
                            target_manifest.formats.push(track.format.clone());
                            target_manifest.pairs =
                                quality::pair_file_formats(&target_manifest.formats);
                            quality::finish(
                                &mut target_manifest.formats,
                                &mut target_manifest.pairs,
                                target_manifest.duration,
                            );
                            if let Some(SourcePlan::Files { tracks }) = target.source.as_mut() {
                                tracks.push(track.clone());
                            }
                            merged = true;
                        });
                        if merged {
                            // The extension is still watching the id it was
                            // given. Keep that id answerable by pointing it at
                            // the merged job instead of letting the next poll
                            // read "unknown media job".
                            state.alias_media_job(id, target_id);
                            return;
                        }
                    }

                    // A reused YouTube address inherits the site's own
                    // throttling; the reader is told instead of wondering
                    // why the file is slow.
                    let notice = super::download::is_youtube_job(&job).then(|| {
                        "This uses the address the page already played, so the site's own speed limit applies."
                            .to_string()
                    });
                    state.update_media_job(id, |job| {
                        job.state = MediaState::Ready;
                        job.error = None;
                        job.notice = notice;
                        job.manifest = Some(manifest);
                        job.source = Some(source);
                    });
                    let inline = job.inline;
                    drop(gate);
                    if !inline {
                        super::window::show(&app, id);
                    }
                    return;
                }
            }

            let notice = match &source {
                SourcePlan::Dash { notice, .. } => notice.clone(),
                _ => None,
            };
            state.update_media_job(id, |job| {
                job.state = MediaState::Ready;
                job.error = None;
                job.notice = notice;
                job.manifest = Some(manifest);
                job.source = Some(source);
            });
            if !job.inline {
                super::window::show(&app, id);
            }
        }
        Err(error) => {
            state.update_media_job(id, |job| {
                job.state = MediaState::Failed;
                job.error = Some(error);
            });
        }
    }
}

async fn parse_manifest(job: &MediaJob) -> Result<(Manifest, SourcePlan), String> {
    // A YouTube page is resolved through its iOS player client, which still
    // returns direct addresses; the page's own formats carry no usable `url`.
    if job.kind == MediaKindHint::Youtube || super::yt::is_youtube_page(&job.page_url) {
        return super::yt::resolve(&job.page_url, &job.title).await;
    }
    match infer_kind(job.kind, &job.manifest_url) {
        MediaKindHint::Youtube => super::yt::resolve(&job.page_url, &job.title).await,
        MediaKindHint::Hls => parse_hls(job).await,
        MediaKindHint::Dash => parse_dash(job).await,
        MediaKindHint::File => parse_file(job).await,
        MediaKindHint::Auto => {
            // The address does not say what it is. Read it as text: a
            // manifest is small, and anything that is not one is refused
            // rather than downloaded as a page.
            let fetched = super::probe::fetch(
                &job.manifest_url,
                &job.referer,
                &job.headers,
                job.header_origin(),
            )
            .await?;
            if fetched.text.starts_with("#EXTM3U") {
                parse_hls_text(job, fetched)
            } else if looks_like_mpd(&fetched.text) {
                parse_dash_text(job, fetched)
            } else {
                Err("the address is not a media manifest".to_string())
            }
        }
    }
}

async fn parse_hls(job: &MediaJob) -> Result<(Manifest, SourcePlan), String> {
    let fetched = super::probe::fetch(
        &job.manifest_url,
        &job.referer,
        &job.headers,
        job.header_origin(),
    )
    .await?;
    parse_hls_text(job, fetched)
}

fn parse_hls_text(
    job: &MediaJob,
    fetched: super::probe::Fetched,
) -> Result<(Manifest, SourcePlan), String> {
    let title = title_of(job, &fetched.url);
    if hls::looks_like_master(&fetched.text) {
        let master =
            hls::parse_master(&fetched.text, &fetched.url).map_err(|error| error.to_string())?;
        let mut manifest = quality::manifest_from_hls(&master, None);
        manifest.title = title;
        Ok((
            manifest,
            SourcePlan::Hls {
                master: Some(master),
                media: None,
                base: fetched.url,
            },
        ))
    } else {
        let media =
            hls::parse_media(&fetched.text, &fetched.url).map_err(|error| error.to_string())?;
        let duration = Some(media.duration);
        let mut formats = vec![quality::hls_media_format(&media)];
        let mut pairs = vec![Pair {
            video: "media".to_string(),
            audio: None,
            label: String::new(),
        }];
        quality::finish(&mut formats, &mut pairs, duration);
        let manifest = Manifest {
            title,
            duration,
            formats,
            pairs,
            container_hint: Some("hls".to_string()),
        };
        Ok((
            manifest,
            SourcePlan::Hls {
                master: None,
                media: Some(media),
                base: fetched.url,
            },
        ))
    }
}

async fn parse_dash(job: &MediaJob) -> Result<(Manifest, SourcePlan), String> {
    let fetched = super::probe::fetch(
        &job.manifest_url,
        &job.referer,
        &job.headers,
        job.header_origin(),
    )
    .await?;
    parse_dash_text(job, fetched)
}

fn parse_dash_text(
    job: &MediaJob,
    fetched: super::probe::Fetched,
) -> Result<(Manifest, SourcePlan), String> {
    let mut parsed =
        dash::parse_full(&fetched.text, &fetched.url).map_err(|error| error.to_string())?;
    parsed.manifest.title = title_of(job, &fetched.url);
    Ok((
        parsed.manifest,
        SourcePlan::Dash {
            tracks: parsed.tracks,
            notice: parsed.notice,
        },
    ))
}

async fn parse_file(job: &MediaJob) -> Result<(Manifest, SourcePlan), String> {
    let probe = super::probe::probe_file(
        &job.manifest_url,
        &job.referer,
        &job.headers,
        job.header_origin(),
    )
    .await?;
    if probe
        .content_type
        .as_deref()
        .is_some_and(is_protected_content_type)
    {
        return Err(MediaError::Protected.to_string());
    }
    let title = title_of(job, &probe.url);
    let params = youtube_params(&probe.url);
    let described = params.itag.and_then(youtube::describe);
    let content_type = probe.content_type.as_deref().or(params.mime.as_deref());
    let container = described
        .map(|itag| itag.container.to_string())
        .unwrap_or_else(|| file_container(&probe.url, content_type));
    let kind = described.map(|itag| itag.kind).unwrap_or_else(|| {
        if is_audio(&container, content_type) {
            MediaKind::Audio
        } else {
            MediaKind::Muxed
        }
    });
    let id = params
        .itag
        .map(|itag| itag.to_string())
        .unwrap_or_else(|| "file".to_string());
    let mut format = Format::new(id, kind, container);
    format.height = described.and_then(|itag| itag.height);
    format.fps = described.and_then(|itag| itag.fps);
    format.size = params.size.or(probe.total);

    let manifest = file_manifest(title, vec![format.clone()]);
    let format = manifest.formats.first().cloned().unwrap_or(format);
    Ok((
        manifest,
        SourcePlan::Files {
            tracks: vec![FileTrack {
                format,
                url: probe.url,
            }],
        },
    ))
}

/// The visible manifest of plain files: every picture paired with a sound
/// track in the same container, audio-only files left as their own row.
fn file_manifest(title: String, mut formats: Vec<Format>) -> Manifest {
    let mut pairs = quality::pair_file_formats(&formats);
    quality::finish(&mut formats, &mut pairs, None);
    Manifest {
        title,
        duration: None,
        formats,
        pairs,
        container_hint: Some("file".to_string()),
    }
}

#[derive(Default)]
struct YoutubeParams {
    itag: Option<u64>,
    size: Option<u64>,
    mime: Option<String>,
}

/// The itag, size and mime a player put in its own address. The page's text
/// is never used for a label: the itag table decides what the number means.
fn youtube_params(url: &str) -> YoutubeParams {
    let Ok(url) = Url::parse(url) else {
        return YoutubeParams::default();
    };
    let mut params = YoutubeParams::default();
    for (name, value) in url.query_pairs() {
        match name.as_ref() {
            "itag" => params.itag = value.parse().ok(),
            "clen" => params.size = value.parse().ok(),
            "mime" => params.mime = Some(value.into_owned()),
            _ => {}
        }
    }
    params
}

/// What a manifest address appears to be, before it is fetched.
pub(crate) fn infer_kind(hint: MediaKindHint, url: &str) -> MediaKindHint {
    if hint != MediaKindHint::Auto {
        return hint;
    }
    let Ok(url) = Url::parse(url) else {
        return MediaKindHint::Auto;
    };
    let path = url.path().to_ascii_lowercase();
    if path.ends_with(".m3u8") || path.ends_with(".m3u") {
        return MediaKindHint::Hls;
    }
    if path.ends_with(".mpd") || path.ends_with(".ism") || path.ends_with(".ismc") {
        return MediaKindHint::Dash;
    }
    if url.query_pairs().any(|(name, _)| name == "itag")
        || path.ends_with("/videoplayback")
        || [
            ".mp4", ".m4v", ".webm", ".mkv", ".mov", ".ts", ".m4s", ".m4a", ".mp3", ".aac",
            ".flac", ".ogg", ".opus",
        ]
        .iter()
        .any(|extension| path.ends_with(extension))
    {
        return MediaKindHint::File;
    }
    MediaKindHint::Auto
}

fn looks_like_mpd(text: &str) -> bool {
    text.trim_start().starts_with("<?xml") || text.contains("<MPD")
}

fn title_of(job: &MediaJob, final_url: &str) -> String {
    let title = job.title.trim().trim_end_matches(" - YouTube").trim();
    if !title.is_empty() {
        return title.chars().take(200).collect();
    }
    if let Ok(url) = Url::parse(final_url) {
        if let Some(name) = dest::name_from_url(url.as_str()) {
            return name;
        }
        if let Some(host) = url.host_str() {
            return host.to_string();
        }
    }
    "Video".to_string()
}

fn clean_title(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(" - YouTube")
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(200)
        .collect()
}

fn file_container(url: &str, content_type: Option<&str>) -> String {
    if let Some(content_type) = content_type {
        let content_type = content_type.to_ascii_lowercase();
        if content_type.starts_with("audio/mp4") || content_type.starts_with("audio/aac") {
            return "m4a".to_string();
        }
        if content_type.starts_with("audio/mpeg") {
            return "mp3".to_string();
        }
        if content_type.starts_with("audio/webm") || content_type.starts_with("audio/ogg") {
            return "webm".to_string();
        }
        if content_type.starts_with("video/webm") {
            return "webm".to_string();
        }
        if content_type.starts_with("video/mp4") {
            return "mp4".to_string();
        }
    }
    let path = Url::parse(url)
        .map(|url| url.path().to_ascii_lowercase())
        .unwrap_or_else(|_| url.to_ascii_lowercase());
    for extension in [
        "mp4", "m4v", "webm", "mkv", "mov", "ts", "m4s", "m4a", "mp3", "aac", "flac", "ogg", "opus",
    ] {
        if path.ends_with(&format!(".{extension}")) {
            return match extension {
                "m4v" => "mp4".to_string(),
                "m4s" => "mp4".to_string(),
                other => other.to_string(),
            };
        }
    }
    "mp4".to_string()
}

/// YouTube's UMP and any explicit DRM type are the same refusal.
fn is_protected_content_type(content_type: &str) -> bool {
    let content_type = content_type.to_ascii_lowercase();
    content_type.contains("vnd.yt-ump") || content_type.contains("application/drm")
}

fn is_audio(container: &str, content_type: Option<&str>) -> bool {
    if let Some(content_type) = content_type {
        if content_type.to_ascii_lowercase().starts_with("audio/") {
            return true;
        }
    }
    matches!(container, "m4a" | "mp3" | "aac" | "flac" | "opus" | "ogg")
}

fn is_web_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let itag = super::youtube::describe(params.itag.expect("an itag")).expect("a row");
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
    fn a_ump_content_type_is_the_protected_refusal() {
        assert!(is_protected_content_type("application/vnd.yt-ump"));
        assert!(!is_protected_content_type("video/mp4"));
        assert_eq!(
            MediaError::Protected.to_string(),
            "this stream is protected (DRM), so it cannot be downloaded"
        );
    }

    #[test]
    fn a_title_is_one_line_and_bounded() {
        let title = clean_title("  My film - YouTube  ");
        assert_eq!(title, "My film");
        assert!(clean_title(&"a".repeat(500)).chars().count() <= 200);
        assert_eq!(clean_title("a\nb"), "ab");
    }
}
