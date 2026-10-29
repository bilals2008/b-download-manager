//! Downloading separate streams and muxing them into one playable file.
//!
//! This is the fix for the corrupted-video bug: the engine used to download
//! one URL as one file, so a video-only stream saved as an `.mp4` with no
//! audio. Here a video stream and an audio stream are downloaded side by side,
//! muxed with ffmpeg, and probed with ffprobe — both tracks present, and a
//! real duration — before the file is moved into place. A failed mux never
//! reports success and never leaves temporary files behind.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::Deserialize;
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;

use idm_core::disk;
use idm_core::transfer::Source;

use crate::media::{run_sidecar, MediaInfo};

/// The containers ffmpeg is asked to mux into. Anything else is a mistake the
/// caller should hear about rather than have guessed.
const CONTAINERS: [&str; 3] = ["mp4", "webm", "mkv"];

/// Download two streams and mux them into `dest`.
///
/// `container` is the extension of the output (`mp4`, `webm` or `mkv`), which
/// the caller derives from the chosen video format so the codecs stay legal
/// for the container. `audio_url` may be empty for a stream that already
/// carries its own sound, in which case the video is probed and saved as it is.
pub async fn mux(
    app: &AppHandle,
    proxy: Option<&str>,
    video_url: &str,
    audio_url: &str,
    dest: &Path,
    container: &str,
) -> Result<(), String> {
    let container = normalize_container(container)?;
    if !video_url.starts_with("http://") && !video_url.starts_with("https://") {
        return Err("the video address must start with http:// or https://".to_string());
    }

    let folder = dest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(folder)
        .map_err(|error| format!("cannot create {}: {error}", folder.display()))?;

    let stem = dest
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "media".to_string());

    // The temporaries live beside the destination so the final move is a
    // rename on the same volume, and they are removed on every exit path.
    let mut temps = TempFiles::default();
    let video_part = folder.join(format!(".{stem}.bdm-video.part"));
    let audio_part = folder.join(format!(".{stem}.bdm-audio.part"));
    let muxed = folder.join(format!(".{stem}.bdm-mux.{container}"));
    temps.push(video_part.clone());
    temps.push(audio_part.clone());
    temps.push(muxed.clone());

    download_to_file(video_url, proxy, &video_part).await?;

    // A stream that already has sound is saved as it is, after it is proven
    // to be a video.
    if audio_url.trim().is_empty() {
        let info = probe(app, &video_part).await?;
        if !info.has_video {
            return Err("the download is not a video".to_string());
        }
        disk::replace_file(&video_part, dest)
            .map_err(|error| format!("the video could not be saved: {error}"))?;
        return Ok(());
    }

    download_to_file(audio_url, proxy, &audio_part).await?;
    mux_with_ffmpeg(app, &video_part, &audio_part, &muxed, &container).await?;

    let info = probe(app, &muxed).await?;
    if !info.has_video || !info.has_audio {
        return Err(
            "the muxed file is missing a track; it was not saved".to_string(),
        );
    }
    if info.duration.is_some_and(|duration| duration <= 0.0) {
        return Err("the muxed file has no playable length; it was not saved".to_string());
    }

    disk::replace_file(&muxed, dest)
        .map_err(|error| format!("the muxed video could not be saved: {error}"))?;
    Ok(())
}

/// Read what a media file holds with ffprobe.
pub async fn probe(app: &AppHandle, file: &Path) -> Result<MediaInfo, String> {
    let args = vec![
        "-v".to_string(),
        "error".to_string(),
        "-print_format".to_string(),
        "json".to_string(),
        "-show_format".to_string(),
        "-show_streams".to_string(),
        file.to_string_lossy().to_string(),
    ];
    let stdout = run_sidecar(app, "ffprobe", &args).await?;
    parse_probe(&stdout)
}

/// Download one stream to a file, reusing the engine's HTTP layer so the
/// user's proxy and the browser user-agent apply.
async fn download_to_file(url: &str, proxy: Option<&str>, path: &Path) -> Result<(), String> {
    let source = Source::parse(url, proxy, None)?;
    let mut stream = source
        .open(0, None, true, None, None)
        .await
        .map_err(|error| error.message)?;

    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|error| error.to_string())?;
        file.write_all(&bytes)
            .await
            .map_err(|error| error.to_string())?;
    }
    file.flush().await.map_err(|error| error.to_string())?;
    file.sync_all().await.map_err(|error| error.to_string())?;
    Ok(())
}

/// Mux a video and an audio stream with ffmpeg, copying both tracks without
/// re-encoding. The first video stream and the first audio stream are chosen,
/// so a container that carries several does not pull in a second track.
async fn mux_with_ffmpeg(
    app: &AppHandle,
    video: &Path,
    audio: &Path,
    out: &Path,
    container: &str,
) -> Result<(), String> {
    let args = mux_args(video, audio, out, container);
    run_sidecar(app, "ffmpeg", &args).await?;
    Ok(())
}

/// The argument list ffmpeg runs with for a copy-mux.
fn mux_args(video: &Path, audio: &Path, out: &Path, container: &str) -> Vec<String> {
    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(),
        video.to_string_lossy().to_string(),
        "-i".to_string(),
        audio.to_string_lossy().to_string(),
        "-map".to_string(),
        "0:v:0".to_string(),
        "-map".to_string(),
        "1:a:0".to_string(),
        "-c".to_string(),
        "copy".to_string(),
    ];
    // `+faststart` moves the moov atom to the front so an mp4 streams; it only
    // means anything to the mov/mp4 muxer, so it is added for that container.
    if container == "mp4" {
        args.push("-movflags".to_string());
        args.push("+faststart".to_string());
    }
    args.push("-nostats".to_string());
    args.push("-loglevel".to_string());
    args.push("error".to_string());
    args.push(out.to_string_lossy().to_string());
    args
}

/// Normalise a container choice to one ffmpeg can write.
fn normalize_container(container: &str) -> Result<String, String> {
    let cleaned = container.trim().trim_start_matches('.').to_ascii_lowercase();
    if CONTAINERS.contains(&cleaned.as_str()) {
        Ok(cleaned)
    } else {
        Err(format!(
            "the container must be one of {}, not \"{container}\"",
            CONTAINERS.join(", ")
        ))
    }
}

/// The slice of ffprobe's JSON this module reads.
#[derive(Deserialize)]
struct ProbeJson {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    #[serde(default)]
    format: Option<ProbeFormat>,
}

#[derive(Deserialize)]
struct ProbeStream {
    #[serde(default)]
    codec_type: Option<String>,
    #[serde(default)]
    codec_name: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
}

#[derive(Deserialize)]
struct ProbeFormat {
    #[serde(default)]
    duration: Option<String>,
    #[serde(default)]
    format_name: Option<String>,
}

/// Read the streams and the container out of ffprobe's JSON answer.
fn parse_probe(json: &str) -> Result<MediaInfo, String> {
    let parsed: ProbeJson = serde_json::from_str(json)
        .map_err(|error| format!("the probe answered with unreadable data: {error}"))?;

    let video = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("video"));
    let audio = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"));

    let duration = parsed
        .format
        .as_ref()
        .and_then(|format| format.duration.as_deref())
        .and_then(|value| value.parse::<f64>().ok());

    Ok(MediaInfo {
        has_video: video.is_some(),
        has_audio: audio.is_some(),
        duration,
        width: video.and_then(|stream| stream.width),
        height: video.and_then(|stream| stream.height),
        video_codec: video.and_then(|stream| stream.codec_name.clone()),
        audio_codec: audio.and_then(|stream| stream.codec_name.clone()),
        container: parsed
            .format
            .as_ref()
            .and_then(|format| format.format_name.clone()),
    })
}

/// Temporary files that are removed when they leave scope, whatever the exit
/// path. The destination is moved out of them before they are dropped, so a
/// successful mux removes nothing but an already-renamed name.
#[derive(Default)]
struct TempFiles {
    paths: Vec<PathBuf>,
}

impl TempFiles {
    fn push(&mut self, path: PathBuf) {
        self.paths.push(path);
    }
}

impl Drop for TempFiles {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_container_is_cleaned_and_accepted() {
        assert_eq!(normalize_container("mp4").unwrap(), "mp4");
        assert_eq!(normalize_container(".WEBM").unwrap(), "webm");
        assert_eq!(normalize_container(" mkv ").unwrap(), "mkv");
    }

    #[test]
    fn an_unknown_container_is_refused() {
        let error = normalize_container("avi").unwrap_err();
        assert!(error.contains("mp4"), "the error was: {error}");
    }

    #[test]
    fn the_mp4_mux_puts_the_moov_atom_first() {
        let args = mux_args(
            Path::new("v.part"),
            Path::new("a.part"),
            Path::new("out.mp4"),
            "mp4",
        );
        assert_eq!(args[0], "-y");
        assert!(args.windows(2).any(|pair| pair == ["-movflags", "+faststart"]));
        assert_eq!(args.last().map(String::as_str), Some("out.mp4"));
    }

    #[test]
    fn a_webm_mux_does_not_add_movflags() {
        let args = mux_args(
            Path::new("v.part"),
            Path::new("a.part"),
            Path::new("out.webm"),
            "webm",
        );
        assert!(!args.iter().any(|arg| arg == "-movflags"));
    }

    #[test]
    fn a_probe_reads_both_tracks_and_the_duration() {
        let info = parse_probe(
            r#"{
                "streams": [
                    {"codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080},
                    {"codec_type": "audio", "codec_name": "aac", "width": null, "height": null}
                ],
                "format": {"duration": "12.340000", "format_name": "mov,mp4,m4a,3gp,3g2,mj2"}
            }"#,
        )
        .expect("a probe");

        assert!(info.has_video);
        assert!(info.has_audio);
        assert_eq!(info.duration, Some(12.34));
        assert_eq!(info.width, Some(1920));
        assert_eq!(info.height, Some(1080));
        assert_eq!(info.video_codec.as_deref(), Some("h264"));
        assert_eq!(info.audio_codec.as_deref(), Some("aac"));
        assert_eq!(
            info.container.as_deref(),
            Some("mov,mp4,m4a,3gp,3g2,mj2")
        );
    }

    #[test]
    fn a_video_only_probe_reports_no_audio() {
        let info = parse_probe(
            r#"{
                "streams": [{"codec_type": "video", "codec_name": "vp9", "width": 640, "height": 360}],
                "format": {"duration": "4.000000", "format_name": "matroska,webm"}
            }"#,
        )
        .expect("a probe");

        assert!(info.has_video);
        assert!(!info.has_audio);
        assert_eq!(info.audio_codec, None);
    }
}
