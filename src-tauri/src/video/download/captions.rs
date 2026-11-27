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
    /// The language of the words in it, when it has one: the track the reader
    /// chose names it, and a file the reader picked does not. It goes into the
    /// name the file is saved under, so a folder says which language a `.srt`
    /// is in without the file being opened.
    pub language: Option<String>,
}

/// What a job's caption request came to.
pub(super) struct Prepared {
    /// The SRT to mux, and to write beside the video.
    pub captions: Option<Captions>,
    /// Why the captions are not there, when they are not. It is said on the
    /// finished row: a caption the site would not hand over must never cost
    /// the reader a video whose bytes are already on disk.
    pub warning: Option<String>,
    /// What is worth knowing about captions that *are* there: the video's
    /// transcript stood in for the chosen track, so the words may be another
    /// language than the manifest named, and which one is said rather than
    /// passed off as the track the reader picked.
    pub note: Option<String>,
}

impl Prepared {
    /// No captions were asked for, or there is nowhere for them to go.
    pub(super) fn none() -> Self {
        Prepared {
            captions: None,
            warning: None,
            note: None,
        }
    }

    /// The captions are not there, and here is why.
    fn missing(reason: impl Into<String>) -> Self {
        Prepared {
            captions: None,
            warning: Some(reason.into()),
            note: None,
        }
    }

    /// The captions, and what the reader should know about them.
    fn with(captions: Captions, note: Option<String>) -> Self {
        Prepared {
            captions: Some(captions),
            warning: None,
            note,
        }
    }
}

/// The captions this job asked for: the reader's file, or the rendition the
/// manifest offered. A caption that cannot be had is reported, not thrown:
/// it is the video the reader asked for, and the captions are the extra.
pub(super) async fn prepare(
    app: &tauri::AppHandle,
    job: &MediaJob,
    request: &DownloadRequest,
    work_dir: &Path,
) -> Prepared {
    // A file the reader picked wins over a rendition.
    if let Some(file) = request
        .subtitle_file
        .as_deref()
        .map(str::trim)
        .filter(|file| !file.is_empty())
    {
        let path = PathBuf::from(file);
        if !path.is_file() {
            return Prepared::missing("the caption file is not there any more");
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                return Prepared::missing(format!("the caption file could not be read: {error}"))
            }
        };
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_string())
            .unwrap_or_default();
        // A file the reader picked says nothing about its language.
        return match finish(&text, &extension, None, work_dir) {
            Ok(captions) => Prepared::with(captions, None),
            Err(reason) => Prepared::missing(reason),
        };
    }
    let Some(id) = request.subtitle.as_deref() else {
        return Prepared::none();
    };
    let Some(manifest) = job.manifest.as_ref() else {
        return Prepared::missing("the manifest is not there any more; read the page again");
    };
    let Some(rendition) = manifest
        .subtitles
        .iter()
        .find(|rendition| rendition.id == id)
    else {
        return Prepared::missing("that caption track is not in the manifest any more");
    };
    // The language the transcript is asked in, and the one the saved file is
    // named after. A rendition with no language of its own is asked in the
    // panel's own default, and is not labelled with anything.
    let language = rendition
        .language
        .as_deref()
        .map(str::trim)
        .filter(|language| !language.is_empty())
        .map(str::to_string);
    let fetched = match fetch_caption(app, job, rendition).await {
        Ok(fetched) => fetched,
        Err(reason) => {
            return Prepared::missing(format!(
                "the captions could not be saved ({reason}); the video was saved without them"
            ))
        }
    };
    let note = fetched.transcript.then(|| match &language {
        Some(code) => format!(
            "the chosen caption track did not come; the video's transcript was saved in {} ({code})",
            rendition.name
        ),
        None => "the chosen caption track did not come; the video's transcript was saved instead"
            .to_string(),
    });
    match finish(&fetched.text, &fetched.container, language, work_dir) {
        Ok(captions) => Prepared::with(captions, note),
        Err(reason) => Prepared::missing(reason),
    }
}

/// One caption body, and the format the bytes are written in.
struct Fetched {
    text: String,
    /// `srt` or `vtt`. The site names it in the manifest, and a track that
    /// names SRT is handed the SRT parser: YouTube lists both forms, and a
    /// comma-decimal file read as WebVTT loses every cue.
    container: String,
    /// Whether this text is the video's own transcript rather than the track
    /// the reader chose.
    transcript: bool,
}

/// Fetch one caption track: the address the manifest holds, asked again while
/// the site says it is busy; then the video's own transcript, which YouTube
/// answers from a different service; then the video helper, which asks the
/// same track the way the page's own player does.
///
/// YouTube answers its timed-text endpoint with 429 on many addresses, and a
/// helper that asks that same address is throttled with it; the transcript
/// panel is a separate call and still answers. Whichever of them gives the
/// text, the answer to a caption that none of them give is what the finished
/// row says, not a stopped download.
async fn fetch_caption(
    app: &tauri::AppHandle,
    job: &MediaJob,
    rendition: &idm_core::media::subtitles::SubtitleRendition,
) -> Result<Fetched, String> {
    let mut last = "the caption track came back empty".to_string();
    for attempt in 1..=3u32 {
        match crate::video::probe::fetch(
            &rendition.url,
            &job.referer,
            &job.headers,
            job.header_origin(),
        )
        .await
        {
            Ok(fetched) if !fetched.text.trim().is_empty() => {
                return Ok(Fetched {
                    text: fetched.text,
                    container: container_of(rendition),
                    transcript: false,
                });
            }
            Ok(_) => break,
            Err(message) => {
                // A busy site is asked again after its own pause; a refusal
                // for another reason will not pass by waiting.
                let busy = message.contains("429") || message.contains("503");
                last = message;
                if !busy || attempt == 3 {
                    break;
                }
                tokio::time::sleep(idm_core::http::backoff(attempt)).await;
            }
        }
    }
    // The panel is asked before the helper, because it is a different service
    // than the throttled one, it answers in one request, and it can be asked
    // in the language of the chosen track.
    if let Ok(text) = crate::video::yt::transcript::fetch(
        &job.page_url,
        rendition.language.as_deref(),
    )
    .await
    {
        if !text.trim().is_empty() {
            idm_core::log::write(
                "info",
                &format!(
                    "the caption address did not answer; the transcript was read in {}",
                    rendition.language.as_deref().unwrap_or("the video's own language")
                ),
            );
            return Ok(Fetched {
                text,
                container: "srt".to_string(),
                transcript: true,
            });
        }
    }
    match helper_caption(app, job, rendition).await {
        // The helper is asked for WebVTT, whatever the site listed.
        Some(text) if !text.trim().is_empty() => Ok(Fetched {
            text,
            container: "vtt".to_string(),
            transcript: false,
        }),
        _ => Err(last),
    }
}

/// The format a rendition's own answer is written in.
///
/// The manifest names it, and the name decides the parser: YouTube lists both
/// an `.srt` and a `.vtt` rendition of the same track, and only one of them
/// has dots as its decimal separator.
fn container_of(rendition: &idm_core::media::subtitles::SubtitleRendition) -> String {
    if rendition.container.eq_ignore_ascii_case("srt") {
        "srt".to_string()
    } else {
        "vtt".to_string()
    }
}

/// Ask the video helper for one caption track. `None` when it is not there,
/// or it did not produce a file.
async fn helper_caption(
    app: &tauri::AppHandle,
    job: &MediaJob,
    rendition: &idm_core::media::subtitles::SubtitleRendition,
) -> Option<String> {
    let helper = crate::video::tools::ytdlp(app)?;
    let folder = std::env::temp_dir().join(format!("bdm-caption-{}", job.id));
    std::fs::create_dir_all(&folder).ok()?;
    let language = rendition
        .language
        .clone()
        .or_else(|| rendition.id.rsplit('.').next().map(str::to_string))
        .unwrap_or_default();
    let template = folder.join("caption.%(ext)s");
    let mut command = tokio::process::Command::new(helper);
    command.args([
        "--skip-download",
        "--no-playlist",
        "--write-subs",
        "--write-auto-subs",
        "--sub-format",
        "vtt",
    ]);
    if !language.trim().is_empty() {
        command.args(["--sub-langs", language.trim()]);
    }
    command.arg("--output").arg(&template).arg(&job.page_url);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let done = tokio::time::timeout(std::time::Duration::from_secs(120), command.output())
        .await
        .ok()?
        .ok()?;
    let _ = done;
    let found = std::fs::read_dir(&folder)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "vtt"));
    let text = found.and_then(|path| std::fs::read_to_string(path).ok());
    let _ = std::fs::remove_dir_all(&folder);
    text
}

/// Write the caption text beside the finished video.
///
/// The name starts with the video's own, which is what a player looks for by
/// itself, and carries the language when the file has one, which is the form a
/// media library reads the language from (`A video 1080p.de.srt`). The
/// captions are there whether or not the container took them, and the reader
/// can open and edit the text.
pub(super) fn beside(captions: &Captions, video: &Path) -> std::io::Result<PathBuf> {
    let name = match &captions.language {
        Some(language) => {
            let stem = video
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| "captions".to_string());
            video.with_file_name(format!("{stem}.{language}.srt"))
        }
        None => video.with_extension("srt"),
    };
    std::fs::copy(&captions.path, &name)?;
    Ok(name)
}

/// Write the caption text as the file the muxer reads.
fn finish(
    text: &str,
    extension: &str,
    language: Option<String>,
    work_dir: &Path,
) -> Result<Captions, String> {
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
    Ok(Captions {
        path: out,
        language,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rendition_is_read_in_the_format_its_manifest_named() {
        let rendition = |container: &str| idm_core::media::subtitles::SubtitleRendition {
            id: "sub-en".to_string(),
            name: "English".to_string(),
            language: Some("en".to_string()),
            url: "https://www.youtube.com/api/timedtext?v=x".to_string(),
            container: container.to_string(),
            is_default: false,
        };
        assert_eq!(container_of(&rendition("srt")), "srt");
        assert_eq!(container_of(&rendition("SRT")), "srt");
        assert_eq!(container_of(&rendition("vtt")), "vtt");
        assert_eq!(container_of(&rendition("")), "vtt");
    }

    #[test]
    fn the_captions_are_written_beside_the_video_under_its_name_and_language() {
        let dir = std::env::temp_dir().join(format!("bdm-sidecar-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let captions = dir.join("captions.srt");
        std::fs::write(&captions, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").expect("text");
        let video = dir.join("A video 1080p.mp4");

        let named = Captions {
            path: captions.clone(),
            language: Some("de".to_string()),
        };
        let sidecar = beside(&named, &video).expect("a sidecar");
        assert_eq!(sidecar, dir.join("A video 1080p.de.srt"));
        assert!(std::fs::read_to_string(&sidecar)
            .expect("the sidecar reads")
            .contains("Hello"));

        // A file with no language of its own keeps the video's own name.
        let plain = Captions {
            path: captions,
            language: None,
        };
        assert_eq!(
            beside(&plain, &video).expect("a sidecar"),
            dir.join("A video 1080p.srt")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
