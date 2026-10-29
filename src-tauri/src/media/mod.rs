//! Media tooling: ffmpeg muxing and probing, and yt-dlp format extraction.
//!
//! The application bundles three external binaries as Tauri sidecars — ffmpeg,
//! ffprobe and yt-dlp — so the end user never has to install them. This module
//! is the only place that talks to those binaries. The commands it exposes are
//! also the backend of the browser extension's quality selection, through the
//! loopback bridge.
//!
//! A video and its sound are often two separate streams (YouTube serves them
//! as separate DASH itags). Downloading a video-only stream alone produces a
//! file with no sound, so the download path here always has the option of
//! muxing the two into one playable file and probing the result before it is
//! reported as finished.

pub(crate) mod extract;
pub(crate) mod mux;

use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_shell::ShellExt;

use crate::state::AppState;

/// One downloadable format of a video, as the extractor reports it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaFormat {
    /// The extractor's format id: the itag on YouTube, e.g. "137".
    pub format_id: String,
    /// A human label: "1080p", "720p", or "" for an audio stream.
    pub resolution: String,
    /// Pixel height, when the stream has a picture.
    pub height: Option<u32>,
    /// Pixel width, when the stream has a picture.
    pub width: Option<u32>,
    /// Frames per second, when known.
    pub fps: Option<f64>,
    /// The video codec ("avc1", "vp9"), or null for an audio stream.
    pub vcodec: Option<String>,
    /// The audio codec ("opus", "mp4a"), or null for a video-only stream.
    pub acodec: Option<String>,
    /// The container the stream belongs in: "mp4", "webm", "m4a", ...
    pub ext: String,
    /// The size in bytes, when the extractor knows it.
    pub filesize: Option<u64>,
    /// "video-only", "audio-only" or "muxed".
    pub kind: String,
    /// The direct stream address, freshly resolved by the extractor.
    pub url: Option<String>,
}

/// What ffprobe found in a media file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub has_video: bool,
    pub has_audio: bool,
    /// Duration in seconds, when the file has one.
    pub duration: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    /// The container format, e.g. "mov,mp4,m4a,3gp,3g2,mj2".
    pub container: Option<String>,
}

/// Whether each bundled media tool can run. Reported to the extension over the
/// bridge so it can hide options the application cannot honour.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaTools {
    pub ffmpeg: bool,
    pub ffprobe: bool,
    pub ytdlp: bool,
}

/// Run a bundled sidecar and return its stdout, or a readable error from its
/// stderr. This is the single point where the application spawns ffmpeg,
/// ffprobe and yt-dlp.
pub(crate) async fn run_sidecar(
    app: &AppHandle,
    name: &str,
    args: &[String],
) -> Result<String, String> {
    let command = app
        .shell()
        .sidecar(name)
        .map_err(|error| format!("{name} is not available: {error}"))?;
    let output = command
        .args(args)
        .output()
        .await
        .map_err(|error| format!("{name} could not run: {error}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(clean_stderr(&String::from_utf8_lossy(&output.stderr)))
    }
}

/// Trim a program's stderr down to one short, readable line for the user.
fn clean_stderr(stderr: &str) -> String {
    let mut message = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if message.is_empty() {
        message = "the tool failed without saying why".to_string();
    }
    if message.chars().count() > 400 {
        let cut = message
            .char_indices()
            .take_while(|(index, _)| *index < 400)
            .map(|(_, character)| character)
            .collect::<String>();
        message = format!("{cut}…");
    }
    message
}

/// Ask each bundled tool for its version; a tool that cannot run is reported
/// as unavailable. The answer is what both the desktop window and the
/// extension use to decide whether muxing and quality selection are offered.
pub async fn media_tools(app: &AppHandle) -> MediaTools {
    let (ffmpeg, ffprobe, ytdlp) = tokio::join!(
        version_ok(app, "ffmpeg", "-version"),
        version_ok(app, "ffprobe", "-version"),
        version_ok(app, "yt-dlp", "--version"),
    );
    MediaTools {
        ffmpeg,
        ffprobe,
        ytdlp,
    }
}

async fn version_ok(app: &AppHandle, name: &str, flag: &str) -> bool {
    run_sidecar(app, name, &[flag.to_string()]).await.is_ok()
}

/// Whether ffmpeg and ffprobe are both present, which is what muxing needs.
#[tauri::command]
pub async fn ffmpeg_available(app: AppHandle) -> bool {
    let tools = media_tools(&app).await;
    tools.ffmpeg && tools.ffprobe
}

/// Enumerate the formats of a video page, refusing protected (DRM) content.
#[tauri::command]
pub async fn extract_formats(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<Vec<MediaFormat>, String> {
    extract::extract(&app, state.limits().proxy().as_deref(), &url).await
}

/// Download a video and an audio stream and mux them into one playable file.
#[tauri::command]
pub async fn mux_streams(
    app: AppHandle,
    state: State<'_, AppState>,
    video_url: String,
    audio_url: String,
    dest: String,
    container: String,
) -> Result<(), String> {
    mux::mux(
        &app,
        state.limits().proxy().as_deref(),
        &video_url,
        &audio_url,
        Path::new(dest.trim()),
        &container,
    )
    .await
}

/// Verify a media file is playable, returning what its streams hold.
#[tauri::command]
pub async fn probe_media(app: AppHandle, file: String) -> Result<MediaInfo, String> {
    mux::probe(&app, Path::new(file.trim())).await
}
