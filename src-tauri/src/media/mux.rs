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
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;

use idm_core::disk;
use idm_core::transfer::Source;

use crate::media::{clean_stderr, run_sidecar, run_sidecar_stderr, MediaInfo};

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

/// Read what a media file holds with ffmpeg.
pub async fn probe(app: &AppHandle, file: &Path) -> Result<MediaInfo, String> {
    // ffmpeg prints the input's streams to stderr and exits non-zero when it
    // is only asked to read a file, so the stderr is read and the status
    // ignored. This keeps one ffmpeg binary doing both the mux and the probe.
    let args = vec![
        "-hide_banner".to_string(),
        "-i".to_string(),
        file.to_string_lossy().to_string(),
    ];
    let stderr = run_sidecar_stderr(app, "ffmpeg", &args).await?;
    parse_ffmpeg_probe(&stderr)
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

/// Read the streams and the container out of ffmpeg's `-i` report.
///
/// ffmpeg prints the input to stderr in a stable, human-readable shape: the
/// container after `Input #0,`, a `Duration: HH:MM:SS.cc` line, and one
/// `Stream #...: Video: <codec> ... WxH ...` or `Audio: <codec>` line per
/// track. Probing reads that instead of shipping a second ffprobe binary.
fn parse_ffmpeg_probe(stderr: &str) -> Result<MediaInfo, String> {
    let mut container = None;
    let mut duration = None;
    let mut has_video = false;
    let mut has_audio = false;
    let mut width = None;
    let mut height = None;
    let mut video_codec = None;
    let mut audio_codec = None;
    let mut found_input = false;

    for line in stderr.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Input #0,") {
            found_input = true;
            container = rest
                .split(", from")
                .next()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string);
        } else if let Some(rest) = line.strip_prefix("Duration:") {
            duration = rest
                .split(',')
                .next()
                .map(str::trim)
                .and_then(parse_duration);
        } else if let Some(rest) = line.strip_prefix("Stream #") {
            // `Stream #0:0[0x1](und): Video: h264 ... 320x240 ...`
            let after = rest.split_once(": ").map(|(_, tail)| tail).unwrap_or(rest);
            if let Some(tail) = after.strip_prefix("Video:") {
                has_video = true;
                let tail = tail.trim();
                video_codec = codec_name(tail);
                if let Some((w, h)) = find_resolution(tail) {
                    width = Some(w);
                    height = Some(h);
                }
            } else if let Some(tail) = after.strip_prefix("Audio:") {
                has_audio = true;
                audio_codec = codec_name(tail.trim());
            }
        }
    }

    if !found_input {
        return Err(clean_stderr(stderr));
    }

    Ok(MediaInfo {
        has_video,
        has_audio,
        duration,
        width,
        height,
        video_codec,
        audio_codec,
        container,
    })
}

/// The codec at the head of a stream line: `h264 (High ...)` becomes `h264`.
fn codec_name(tail: &str) -> Option<String> {
    tail.split(|c: char| c.is_whitespace() || c == '(' || c == ',')
        .map(str::trim)
        .find(|part| !part.is_empty())
        .map(str::to_string)
}

/// The `WxH` of a video stream, ignoring the `0x…` codec tag that also
/// contains an `x`.
fn find_resolution(line: &str) -> Option<(u32, u32)> {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let mut width_end = index;
        while width_end < bytes.len() && bytes[width_end].is_ascii_digit() {
            width_end += 1;
        }
        let width_len = width_end - index;
        if (2..=5).contains(&width_len) && width_end < bytes.len() && bytes[width_end] == b'x' {
            let mut height_end = width_end + 1;
            while height_end < bytes.len() && bytes[height_end].is_ascii_digit() {
                height_end += 1;
            }
            let height_len = height_end - (width_end + 1);
            if (2..=5).contains(&height_len) {
                let width: u32 = line[index..width_end].parse().ok()?;
                let height: u32 = line[width_end + 1..height_end].parse().ok()?;
                return Some((width, height));
            }
        }
        index = width_end;
    }
    None
}

/// `00:00:02.00` in seconds.
fn parse_duration(value: &str) -> Option<f64> {
    let mut parts = value.split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
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
        let info = parse_ffmpeg_probe(
            "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'out.mp4':\n\
             Duration: 00:00:02.00, start: 0.000000, bitrate: 131 kb/s\n\
             Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv444p(progressive), 320x240 [SAR 1:1 DAR 4:3], 46 kb/s, 30 fps\n\
             Stream #0:1[0x2](und): Audio: aac (LC) (mp4a / 0x6134706D), 44100 Hz, mono, fltp, 69 kb/s\n",
        )
        .expect("a probe");

        assert!(info.has_video);
        assert!(info.has_audio);
        assert_eq!(info.duration, Some(2.0));
        // The 0x… codec tag is not mistaken for a resolution.
        assert_eq!(info.width, Some(320));
        assert_eq!(info.height, Some(240));
        assert_eq!(info.video_codec.as_deref(), Some("h264"));
        assert_eq!(info.audio_codec.as_deref(), Some("aac"));
        assert_eq!(info.container.as_deref(), Some("mov,mp4,m4a,3gp,3g2,mj2"));
    }

    #[test]
    fn a_video_only_probe_reports_no_audio() {
        let info = parse_ffmpeg_probe(
            "Input #0, matroska,webm, from 'clip.webm':\n\
             Duration: 00:00:04.00, start: 0.000000, bitrate: 100 kb/s\n\
             Stream #0:0: Video: vp9, yuv420p(tv), 640x360, 100 kb/s, 30 fps, 30 tbr\n",
        )
        .expect("a probe");

        assert!(info.has_video);
        assert!(!info.has_audio);
        assert_eq!(info.duration, Some(4.0));
        assert_eq!(info.video_codec.as_deref(), Some("vp9"));
        assert_eq!(info.audio_codec, None);
    }

    #[test]
    fn an_unreadable_file_is_reported() {
        let error =
            parse_ffmpeg_probe("out.mp4: No such file or directory\n").expect_err("an error");
        assert!(error.contains("No such file"), "the error was: {error}");
    }
}
