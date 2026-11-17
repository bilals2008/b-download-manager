//! The caption file a finished video may carry.
//!
//! The reader either picked a file of their own, or chose one of the tracks
//! the manifest offered (an HLS `#EXT-X-MEDIA:TYPE=SUBTITLES` rendition, a
//! DASH `text` adaptation set). The file is written to the job's own folder,
//! converted to SRT when it arrived as WebVTT, and muxed into the container
//! with the video and the sound: WebVTT and SRT only, never a transcode of
//! the picture, and never a timing shift.

use std::path::{Path, PathBuf};

use crate::video::capture::DownloadRequest;
use crate::video::MediaJob;

use idm_core::media::subtitles;

/// The caption file of a job, as the muxer reads it.
pub(super) struct Captions {
    /// The SRT file to mux.
    pub path: PathBuf,
}

/// The captions this job asked for: the reader's file, or the rendition the
/// manifest offered. `Ok(None)` when none were asked for.
pub(super) async fn prepare(
    app: &tauri::AppHandle,
    job: &MediaJob,
    request: &DownloadRequest,
    work_dir: &Path,
) -> Result<Option<Captions>, String> {
    let _ = app;
    // A file the reader picked wins over a rendition.
    if let Some(file) = request
        .subtitle_file
        .as_deref()
        .map(str::trim)
        .filter(|file| !file.is_empty())
    {
        let path = PathBuf::from(file);
        if !path.is_file() {
            return Err("the caption file is not there any more".to_string());
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("the caption file could not be read: {error}"))?;
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_string())
            .unwrap_or_default();
        return finish(&text, &extension, work_dir).map(Some);
    }
    let Some(id) = request.subtitle.as_deref() else {
        return Ok(None);
    };
    let manifest = job
        .manifest
        .as_ref()
        .ok_or_else(|| "the manifest is not there any more; read the page again".to_string())?;
    let Some(rendition) = manifest
        .subtitles
        .iter()
        .find(|rendition| rendition.id == id)
    else {
        return Err("that caption track is not in the manifest any more".to_string());
    };
    let fetched = crate::video::probe::fetch(
        &rendition.url,
        &job.referer,
        &job.headers,
        job.header_origin(),
    )
    .await?;
    finish(&fetched.text, rendition.container.as_str(), work_dir).map(Some)
}

/// Write the caption text as the file the muxer reads.
fn finish(text: &str, extension: &str, work_dir: &Path) -> Result<Captions, String> {
    let extension = extension.trim().to_ascii_lowercase();
    if extension != "vtt" && extension != "srt" {
        // A rendition named for something this build does not read is an
        // honest refusal, not a silent drop of the captions.
        return Err(
            "this build muxes WebVTT and SRT captions; it cannot read that track".to_string(),
        );
    }
    let out = work_dir.join("captions.srt");
    let body = if extension == "vtt" {
        // WebVTT converts into SRT; the timings are the file's own.
        let cues = subtitles::parse_vtt(text);
        if cues.is_empty() {
            return Err("the caption track could not be read".to_string());
        }
        subtitles::to_srt(&cues)
    } else {
        let cues = subtitles::parse_srt(text);
        if cues.is_empty() {
            return Err("the caption track could not be read".to_string());
        }
        text.to_string()
    };
    std::fs::write(&out, body)
        .map_err(|error| format!("the caption file could not be written: {error}"))?;
    Ok(Captions { path: out })
}
