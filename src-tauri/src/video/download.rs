//! Starting one media download: fetch the chosen playlists, run the tracks
//! through the engine, and put the result in place.
//!
//! A track is downloaded by [`idm_core::media::download`]: one engine download
//! per segment, resumed from the bytes on disk. The remux step (V4) sits
//! between the tracks and the destination and is reached through one function,
//! so this coordinator does not care which muxer is behind it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager};

use idm_core::disk;
use idm_core::media::download::{
    self, ProgressSink, TrackControl, TrackError, TrackOutcome, TrackProblem, TrackProgress,
    TrackSource,
};
use idm_core::media::{hls, Format, MediaKind, TrackSegment};

use crate::state::AppState;

use super::capture::DownloadRequest;
use super::lease::RequestLease;
use super::mux::{self, Container};
use super::verify::{self, Expect};
use super::{MediaJob, MediaState, SourcePlan};

/// The row id a started media download answers with.
pub struct Started {
    pub id: u64,
    pub dest: String,
}

/// What the finished file has to be, decided before a byte is fetched.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Delivery {
    /// The extension the file is given. It is what the finished bytes are
    /// checked against.
    pub extension: String,
    /// The container the tracks are joined into. `None` when no join is
    /// needed; a track that turns out not to be in the claimed container is
    /// still joined into the one this name implies.
    pub container: Option<Container>,
    /// The finished file must hold a picture and/or sound.
    pub video: bool,
    pub audio: bool,
    /// An audio track was chosen alongside the picture, so a finished file
    /// without sound means the join lost it. `audio` alone is not enough: a
    /// progressive file is called muxed by the manifest and can still be a
    /// silent clip, which is the file the user asked for.
    pub audio_selected: bool,
    /// The `ffmpeg` a join may use when the native muxers refuse the pair.
    pub ffmpeg: Option<PathBuf>,
    /// The caption file to mux into the finished file, when the reader asked
    /// for one. `None` means the file carries no captions.
    pub captions: Option<PathBuf>,
    /// What the sound is saved as, when the reader asked for a conversion.
    /// `None` means an ordinary video download.
    pub convert: Option<idm_core::media::audio::AudioSelector>,
}

/// Start the chosen track(s). The work is done on a task of its own; the
/// answer carries the destination the panel shows at once.
///
/// `folder` is the folder the reader picked, when they picked one. `None`
/// leaves the choice to the file's category, decided once the finished name
/// and its extension are known: a video lands with the rest of its type, and
/// a sound saved as MP3 lands in Music rather than Video.
pub fn start(
    app: &AppHandle,
    job: &MediaJob,
    request: &DownloadRequest,
    folder: Option<PathBuf>,
    name: String,
) -> Result<Started, String> {
    if job.state != MediaState::Ready {
        return Err("the manifest has not been read yet".to_string());
    }
    if job.downloading {
        return Err("this video is already being downloaded".to_string());
    }
    let Some(manifest) = job.manifest.as_ref() else {
        return Err("the manifest has not been read yet".to_string());
    };
    let Some(format) = manifest
        .formats
        .iter()
        .find(|format| format.id == request.video)
    else {
        return Err("that quality is not in the manifest".to_string());
    };
    let audio_format = match request.audio.as_deref() {
        Some(id) => manifest
            .formats
            .iter()
            .find(|format| format.id == id)
            // A quality list can outlive the exact sound id it was drawn
            // from: the page was resolved again, or a second candidate was
            // merged into the job. The sound paired with the chosen picture
            // is the next best answer, and a download with the right picture
            // beats one that refuses before its first byte.
            .or_else(|| {
                manifest
                    .pairs
                    .iter()
                    .find(|pair| pair.video == request.video)
                    .and_then(|pair| pair.audio.as_deref())
                    .and_then(|audio| manifest.formats.iter().find(|format| format.id == audio))
            }),
        None => None,
    };

    // The choice belongs to the page's host, so the next video there opens on
    // the same quality instead of starting from the default.
    let host = super::host_of(&job.page_url, &job.manifest_url);
    app.state::<AppState>()
        .remember_video_quality(&host, &quality_token(format));

    // The sound may be asked for as the original track or as a conversion. A
    // conversion is a sound-only outcome whatever row it was asked from, so
    // it is the one thing that changes what gets delivered.
    let convert = request
        .audio_format
        .as_deref()
        .map(str::trim)
        .filter(|format| !format.is_empty())
        .map(idm_core::media::audio::AudioSelector::parse)
        .transpose()
        .map_err(|error| error.to_string())?
        .filter(|selector| selector.is_conversion());

    // An audio-only row is saved with its own container. A plain file that is
    // already in a final container and has no second track is already the
    // deliverable, so it is placed as it is. Everything else goes through the
    // remuxer: a fragmented stream or a pair needs one file.
    let direct = convert.is_none()
        && audio_format.is_none()
        && matches!(job.source, Some(SourcePlan::Files { .. }))
        && matches!(format.kind, MediaKind::Muxed | MediaKind::Video)
        && matches!(format.container.as_str(), "mp4" | "webm" | "mkv");
    let audio_only = format.kind == MediaKind::Audio;
    let (extension, container) = if let Some(selector) = &convert {
        // The conversion writes its own container; no join is involved.
        (selector.extension().to_string(), None)
    } else if audio_only {
        // Sound alone keeps its own name and its own container. The track may
        // still arrive as a bare ADTS stream under an `.m4a` name, which the
        // delivery step joins into the container the name promises.
        let extension = output_container(format);
        let container = Container::from_extension(&extension);
        (extension, container)
    } else if direct {
        (
            format.container.clone(),
            Container::from_extension(&format.container),
        )
    } else {
        let container = mux::choose_container(Some(format), audio_format);
        (container.extension().to_string(), Some(container))
    };
    let delivery = Delivery {
        video: convert.is_none() && !audio_only,
        audio: convert.is_some()
            || audio_only
            || audio_format.is_some()
            || matches!(format.kind, MediaKind::Muxed),
        audio_selected: convert.is_some() || audio_only || audio_format.is_some(),
        extension: extension.clone(),
        container,
        ffmpeg: super::tools::ffmpeg(app),
        captions: None,
        convert,
    };
    let file_name = with_extension(&name, &extension);
    let folder = destination_folder(app, folder, &file_name)?;
    let dest = crate::bridge::dest::unique(&folder, &file_name);
    let stem = dest
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("video-{}", job.id));
    // The tracks wait in the application's own media folder, never in the
    // destination: a video job that dies halfway leaves nothing in the folder
    // the user reads as theirs.
    let work_dir = app
        .state::<AppState>()
        .media_work_dir(job.id, &dest, &safe_component(&stem))
        .ok_or_else(|| {
            "the application's storage folder is not available. Free some space and try again."
                .to_string()
        })?;
    std::fs::create_dir_all(&work_dir)
        .map_err(|error| format!("the application's storage folder could not be made: {error}"))?;

    let control = TrackControl::new();
    let dest_text = dest.to_string_lossy().to_string();
    let resume_request = DownloadRequest {
        video: request.video.clone(),
        audio: request.audio.clone(),
        name: file_name.clone(),
        dest: folder.to_string_lossy().to_string(),
        subtitle: request.subtitle.clone(),
        subtitle_file: request.subtitle_file.clone(),
        audio_format: request.audio_format.clone(),
    };
    let app_state = app.state::<AppState>();
    app_state.update_media_job(job.id, |job| {
        job.downloading = true;
        job.done = false;
        job.error = None;
        job.dest = Some(dest_text.clone());
        job.stage = Some("Starting".to_string());
        job.progress = TrackProgress::default();
        job.fraction = 0.0;
        job.request = Some(resume_request);
        job.delivery = Some(delivery.clone());
        job.control = Some(control.clone());
        job.work_dir = Some(work_dir.to_string_lossy().to_string());
    });
    app_state.save_media_index();

    show_progress(app, job.id);

    let job = job.clone();
    let job_id = job.id;
    let request = request.clone();
    let app = app.clone();
    let dest_clone = dest.clone();
    tauri::async_runtime::spawn(async move {
        run(
            &app, job, request, dest_clone, work_dir, delivery, control, false,
        )
        .await;
    });

    Ok(Started {
        id: job_id,
        dest: dest.to_string_lossy().to_string(),
    })
}

/// The folder the finished file goes to.
///
/// A folder the reader picked is used as it is, and must already be there.
/// Without one, the file's category decides — the same rule a browser
/// hand-over follows — under the default save folder, or the system's
/// Downloads folder when none was chosen. The folder is created here: the
/// delivery step moves a finished file into place and does not make folders.
fn destination_folder(
    app: &AppHandle,
    picked: Option<PathBuf>,
    file_name: &str,
) -> Result<PathBuf, String> {
    let folder = match picked {
        Some(folder) => {
            if !folder.is_absolute() || !folder.is_dir() {
                return Err("the folder is not usable".to_string());
            }
            folder
        }
        None => {
            let state = app.state::<AppState>();
            let root = state
                .browser_folder()
                .or_else(|| app.path().download_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            state
                .detect_category(Path::new(file_name))
                .and_then(|category| state.category_folder_in(&category, Some(&root)))
                .unwrap_or(root)
        }
    };
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("the destination folder could not be made: {error}"))?;
    Ok(folder)
}

/// Open the panel that shows the download going.
///
/// A media download shows its progress in the application's one progress
/// window, the same one a download gets — under the same switch, so the two
/// kinds of download behave alike. A registration that came in inline never
/// opened a window at all.
///
/// The window is built on the main thread, and building a webview inside the
/// IPC handler the main thread is already in would never finish, so the build
/// is asked for from the main thread's own queue.
fn show_progress(app: &AppHandle, job: u64) {
    if !app.state::<AppState>().show_progress_on_start() {
        return;
    }
    let handle = app.clone();
    let inside = handle.clone();
    let _ = handle.run_on_main_thread(move || crate::progress::show_media(&inside, job, true));
}

/// Continue a stopped or failed download from the bytes it already has.
///
/// The same destination and unfinished folder are reused, and the page is not
/// resolved again, so a stopped download picks up where it stopped instead of
/// asking for a new plan (and possibly a new representation).
pub fn resume(app: &AppHandle, job: &MediaJob) -> Result<Started, String> {
    if job.state != MediaState::Ready {
        return Err("the manifest has not been read yet".to_string());
    }
    if job.downloading {
        return Err("this video is already being downloaded".to_string());
    }
    if job.done {
        return Err("this video has already been downloaded".to_string());
    }
    let Some(request) = job.request.clone() else {
        return Err("this download cannot be resumed; choose a quality again".to_string());
    };
    let Some(dest) = job.dest.as_deref().map(PathBuf::from) else {
        return Err("this download cannot be resumed; choose a quality again".to_string());
    };
    let Some(work_dir) = job.work_dir.as_deref().map(PathBuf::from) else {
        return Err("this download cannot be resumed; choose a quality again".to_string());
    };
    let Some(mut delivery) = job.delivery.clone() else {
        return Err("this download cannot be resumed; choose a quality again".to_string());
    };
    // A helper installed after the stop is used by the join of this run.
    delivery.ffmpeg = super::tools::ffmpeg(app);

    let control = TrackControl::new();
    let dest_text = dest.to_string_lossy().to_string();
    app.state::<AppState>().update_media_job(job.id, |job| {
        job.downloading = true;
        job.done = false;
        job.error = None;
        job.dest = Some(dest_text.clone());
        job.stage = Some("Starting".to_string());
        job.control = Some(control.clone());
    });
    app.state::<AppState>().save_media_index();

    show_progress(app, job.id);

    let job = job.clone();
    let id = job.id;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run(&app, job, request, dest, work_dir, delivery, control, true).await;
    });

    Ok(Started {
        id,
        dest: dest_text,
    })
}

/// The extension of the file the user gets.
/// The panel's word for a quality: the resolution the manifest stated.
/// Whether this is a YouTube job, read from its page or manifest host.
pub(crate) fn is_youtube_job(job: &MediaJob) -> bool {
    let host = |url: &str| {
        reqwest::Url::parse(url)
            .ok()
            .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
            .unwrap_or_default()
    };
    let page = host(&job.page_url);
    let manifest = host(&job.manifest_url);
    page.contains("youtube.")
        || page == "youtu.be"
        || manifest.contains("googlevideo")
        || manifest.contains("youtube.")
}

fn quality_token(format: &Format) -> String {
    match format.height {
        Some(height) => format!("{height}p"),
        None => format.label.clone(),
    }
}

fn output_container(format: &Format) -> String {
    match format.container.as_str() {
        "ts" => "ts".to_string(),
        "webm" => "webm".to_string(),
        "mkv" => "mkv".to_string(),
        "m4a" | "mp3" | "aac" | "flac" | "opus" | "ogg" => format.container.clone(),
        // An fMP4 track (init + m4s) is playable under `.mp4`; the remux step
        // rewrites it into a proper MP4 later.
        _ => "mp4".to_string(),
    }
}

/// Add the container extension, unless the name already ends with it.
fn with_extension(name: &str, extension: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(&format!(".{extension}")) {
        return name.to_string();
    }
    let known = [
        "mp4", "mkv", "webm", "ts", "m4a", "m4s", "mp3", "aac", "flac", "opus", "ogg",
    ];
    let stem = match name.rfind('.') {
        Some(index) if index > 0 && known.iter().any(|known| &lower[index + 1..] == *known) => {
            &name[..index]
        }
        _ => name,
    };
    format!("{stem}.{extension}")
}

fn safe_component(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '"' | '<' | '>' | '|' | '?' => '_',
            other => other,
        })
        .collect()
}

mod captions;
mod deliver;
mod run;
mod tracks;

use run::run;

#[cfg(test)]
mod tests;
