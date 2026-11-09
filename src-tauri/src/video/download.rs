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
#[derive(Debug, Clone)]
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
}

/// Start the chosen track(s). The work is done on a task of its own; the
/// answer carries the destination the panel shows at once.
pub fn start(
    app: &AppHandle,
    job: &MediaJob,
    request: &DownloadRequest,
    folder: PathBuf,
    name: String,
) -> Result<Started, String> {
    if job.state != MediaState::Ready {
        return Err("the manifest has not been read yet".to_string());
    }
    if job.downloading {
        return Err("this video is already being downloaded".to_string());
    }
    if job.done {
        return Err("this video has already been downloaded".to_string());
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
        Some(id) => Some(
            manifest
                .formats
                .iter()
                .find(|format| format.id == id)
                .ok_or_else(|| "that audio track is not in the manifest".to_string())?,
        ),
        None => None,
    };

    // The choice belongs to the page's host, so the next video there opens on
    // the same quality instead of starting from the default.
    let host = super::host_of(&job.page_url, &job.manifest_url);
    app.state::<AppState>()
        .remember_video_quality(&host, &quality_token(format));

    // An audio-only row is saved with its own container. A plain file that is
    // already in a final container and has no second track is already the
    // deliverable, so it is placed as it is. Everything else goes through the
    // remuxer: a fragmented stream or a pair needs one file.
    let direct = audio_format.is_none()
        && matches!(job.source, Some(SourcePlan::Files { .. }))
        && matches!(format.kind, MediaKind::Muxed | MediaKind::Video)
        && matches!(format.container.as_str(), "mp4" | "webm" | "mkv");
    let audio_only = format.kind == MediaKind::Audio;
    let (extension, container) = if audio_only {
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
        video: !audio_only,
        audio: audio_only || audio_format.is_some() || matches!(format.kind, MediaKind::Muxed),
        audio_selected: audio_only || audio_format.is_some(),
        extension: extension.clone(),
        container,
    };
    let file_name = with_extension(&name, &extension);
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
    let app_state = app.state::<AppState>();
    app_state.update_media_job(job.id, |job| {
        job.downloading = true;
        job.done = false;
        job.error = None;
        job.dest = Some(dest_text.clone());
        job.stage = Some("Starting".to_string());
        job.progress = TrackProgress::default();
        job.control = Some(control.clone());
        job.work_dir = Some(work_dir.to_string_lossy().to_string());
    });

    let job = job.clone();
    let job_id = job.id;
    let request = request.clone();
    let app = app.clone();
    let dest_clone = dest.clone();
    tauri::async_runtime::spawn(async move {
        run(&app, job, request, dest_clone, work_dir, delivery, control).await;
    });

    Ok(Started {
        id: job_id,
        dest: dest.to_string_lossy().to_string(),
    })
}

async fn run(
    app: &AppHandle,
    mut job: MediaJob,
    request: DownloadRequest,
    dest: PathBuf,
    work_dir: PathBuf,
    delivery: Delivery,
    control: Arc<TrackControl>,
) {
    let state = app.state::<AppState>();
    let limits = state.limits().clone();
    let connections = state.browser_connections().clamp(1, 16);

    // A YouTube address is bound to the moment it was resolved: the client
    // identity it was signed for, and a deadline the provider chose. The plan
    // the panel parsed can be minutes old by the time the user picks a
    // quality, so the page is asked once more before the first byte.
    if super::yt::is_youtube_page(&job.page_url) {
        // A resolve that fails quietly here would send the download against a
        // dead address, so what it said is written down.
        match super::ytdlp::resolve_page(app, &job.page_url, &job.title).await {
            Ok((manifest, source)) => {
                job.manifest = Some(manifest);
                job.source = Some(source);
            }
            Err(error) => {
                idm_core::log::write("warn", &format!("resolving the page again failed: {error}"));
            }
        }
    }

    // At most one renewal. A provider that refuses the second address of the
    // same representation is not going to accept a third, and a loop here
    // would spend the user's time instead of telling them.
    let mut refreshed = false;
    let result = loop {
        let outcome = match job.source.as_ref() {
            Some(SourcePlan::Hls {
                master,
                media,
                base,
            }) => {
                run_hls(
                    app,
                    &job,
                    &request,
                    master.as_ref(),
                    media.as_ref(),
                    base,
                    &work_dir,
                    &limits,
                    connections,
                    &control,
                )
                .await
            }
            Some(SourcePlan::Dash { tracks, .. }) => {
                run_dash(
                    app,
                    &job,
                    &request,
                    tracks,
                    &work_dir,
                    &limits,
                    connections,
                    &control,
                )
                .await
            }
            Some(SourcePlan::Files { tracks }) => {
                run_files(
                    app,
                    &job,
                    &request,
                    tracks,
                    &work_dir,
                    &limits,
                    connections,
                    &control,
                )
                .await
            }
            None => Err(Outcome::media("this stream cannot be downloaded")),
        };
        match outcome {
            // The engine says the address was refused, or that the bytes behind
            // it changed. Both are decisions, not sentences, so they are
            // answered with a new address from the page rather than with a
            // search for "403" in the words of the error.
            Err(Outcome::Failed(error))
                if !refreshed
                    && worth_another_address(&error)
                    && super::yt::is_youtube_page(&job.page_url) =>
            {
                refreshed = true;
                idm_core::log::write(
                    "info",
                    &format!(
                        "a track address stopped working ({}); asking the page for a new one",
                        error.problem.name()
                    ),
                );
                match super::ytdlp::renew_page(app, &job.page_url, &job.title).await {
                    Ok((manifest, source)) => {
                        let before = chosen_lease(job.source.as_ref(), &request.video).cloned();
                        let after = chosen_lease(Some(&source), &request.video).cloned();
                        match (&before, &after) {
                            (Some(before), Some(after)) if before.same_bytes(after) => {
                                idm_core::log::write(
                                    "info",
                                    "the new address serves the same representation; the bytes on disk are kept",
                                );
                            }
                            (Some(_), Some(_)) => {
                                // The guard in the engine drops the bytes of
                                // the old representation before the first
                                // fetch, so nothing is mixed. Saying so is the
                                // difference between a slower download and a
                                // corruption nobody notices.
                                idm_core::log::write(
                                    "warn",
                                    "the new address serves another representation; the bytes on disk are dropped",
                                );
                            }
                            _ => {}
                        }
                        job.manifest = Some(manifest);
                        job.source = Some(source);
                        continue;
                    }
                    Err(renew_error) => {
                        break Err(Outcome::Failed(TrackError::new(
                            error.problem,
                            format!(
                                "the address was refused and the page did not offer a new one: {renew_error}"
                            ),
                        )));
                    }
                }
            }
            other => break other,
        }
    };

    match result {
        Ok(()) => {
            let mut inputs = vec![video_track_path(&work_dir)];
            if request.audio.is_some() {
                let audio = audio_track_path(&work_dir);
                if !audio.is_file() {
                    finish_failed(
                        &state,
                        job.id,
                        "the audio track was not written".to_string(),
                    );
                    return;
                }
                inputs.push(audio);
            }

            let stage = stage_sink(app, job.id);
            let destination = dest.clone();
            let plan = delivery.clone();
            let staging = work_dir.clone();
            let finished = tokio::task::spawn_blocking(move || {
                deliver(&destination, inputs, &staging, &plan, stage.as_ref())
            })
            .await;

            match finished {
                Ok(Ok(path)) => {
                    // The finished file is in place and checked, so the pieces
                    // can go.
                    let _ = std::fs::remove_dir_all(&work_dir);
                    finish_saved(&state, job.id, path);
                }
                Ok(Err(DeliverError::Cancelled)) => {
                    state.update_media_job(job.id, |job| {
                        job.downloading = false;
                        job.dest = None;
                        job.error = None;
                        job.stage = Some("Stopped".to_string());
                    });
                }
                Ok(Err(DeliverError::Failed(message))) => finish_failed(&state, job.id, message),
                Err(error) => finish_failed(&state, job.id, error.to_string()),
            }
        }
        Err(Outcome::Cancelled) => {
            state.update_media_job(job.id, |job| {
                job.downloading = false;
                job.dest = None;
                job.error = None;
                job.stage = Some("Stopped".to_string());
            });
        }
        Err(Outcome::Failed(error)) => finish_failed(&state, job.id, error.to_string()),
    }
}

enum Outcome {
    Cancelled,
    Failed(TrackError),
}

impl Outcome {
    /// A failure that no new address can fix: a manifest that does not hold
    /// the quality, a playlist that is gone, a job with no source at all.
    fn media(message: impl Into<String>) -> Self {
        Outcome::Failed(TrackError::new(TrackProblem::Media, message))
    }

    /// A failure that may pass when the same address is asked again.
    fn transient(message: impl Into<String>) -> Self {
        Outcome::Failed(TrackError::new(TrackProblem::Transient, message))
    }
}

/// Why the finished file could not be delivered. As visible as [`deliver`],
/// which answers with it: a type a caller can receive cannot be more private
/// than the function that returns it.
#[derive(Debug)]
pub(super) enum DeliverError {
    Cancelled,
    Failed(String),
}

/// Whether a failure is the address's fault rather than the network's, and so
/// whether the page should be asked for another one.
///
/// A refusal says the address, or the identity it was signed for, is no longer
/// accepted. A change says the bytes behind the address are not the bytes this
/// run started with, which the same address cannot fix either; the guard in the
/// engine has already dropped what no longer matches, so one new address
/// answers both. A short read or a busy server is neither, and is retried by
/// the engine itself rather than here.
fn worth_another_address(error: &TrackError) -> bool {
    matches!(error.problem, TrackProblem::Denied | TrackProblem::Changed)
}

/// The lease of the track a job is downloading, when its plan carries leases.
///
/// It is what a renewal is compared against: a new address for the same bytes
/// keeps the work already done, and an address for another encode must not.
fn chosen_lease<'a>(source: Option<&'a SourcePlan>, id: &str) -> Option<&'a RequestLease> {
    match source? {
        SourcePlan::Files { tracks } => tracks
            .iter()
            .find(|track| track.format.id == id)
            .map(|track| &track.lease),
        _ => None,
    }
}

/// A stable name for the playlist a set of segments came from.
///
/// The addresses inside a playlist are re-signed on every read, so the query
/// is dropped; what is left — the number of segments and the paths of the
/// first and the last — is the same for the same playlist, and different for
/// a live window that has slid on.
fn hls_fingerprint(playlist: &hls::MediaPlaylist) -> Option<String> {
    let first = playlist.segments.first()?;
    let last = playlist.segments.last()?;
    Some(format!(
        "hls|{}|{}|{}",
        playlist.segments.len(),
        path_of(&first.url),
        path_of(&last.url)
    ))
}

/// The same for one representation of a DASH manifest.
fn dash_fingerprint(track: &idm_core::media::dash::DashTrack) -> Option<String> {
    let first = track.segments.first()?;
    Some(format!(
        "dash|{}|{}|{}|{}",
        track.format.id,
        track.format.container,
        track.segments.len(),
        path_of(&first.url)
    ))
}

/// The path of an address, without the query a provider signs.
fn path_of(url: &str) -> String {
    reqwest::Url::parse(url)
        .map(|parsed| parsed.path().to_string())
        .unwrap_or_default()
}

/// Put the downloaded tracks in place, then read the result back.
///
/// A track that already is the file the name promises (a progressive MP4, a
/// WebM, a fragmented `.m4a`) is checked where it lies; anything else is
/// joined and checked inside the job's staging folder. The destination is
/// touched by one step only — the move of a finished, checked file — and a
/// file that was already there is replaced, never deleted: a failed download
/// must not take away what the user had.
pub(super) fn deliver(
    dest: &Path,
    inputs: Vec<PathBuf>,
    work_dir: &Path,
    delivery: &Delivery,
    stage: &dyn Fn(&str),
) -> Result<PathBuf, DeliverError> {
    let primary = inputs
        .first()
        .cloned()
        .ok_or_else(|| DeliverError::Failed("there is nothing to save".to_string()))?;
    let expect = || Expect {
        extension: &delivery.extension,
        video: delivery.video,
        audio: delivery.audio,
        audio_selected: delivery.audio_selected,
    };
    let already = inputs.len() == 1
        && verify::inspect(&primary)
            .ok()
            .is_some_and(|info| info.container.fits_extension(&delivery.extension));
    let container = delivery
        .container
        .or_else(|| Container::from_extension(&delivery.extension));

    let staged = if already {
        stage("Checking");
        verify::check_finished(&primary, expect()).map_err(DeliverError::Failed)?;
        primary
    } else if let Some(container) = container {
        stage("Joining");
        let joined = work_dir.join(format!("final.{}", delivery.extension));
        let _ = std::fs::remove_file(&joined);
        mux::join(&inputs, &joined, work_dir, container).map_err(|error| match error {
            mux::MuxError::Cancelled => DeliverError::Cancelled,
            other => DeliverError::Failed(other.to_string()),
        })?;
        stage("Checking");
        if let Err(error) = verify::check_finished(&joined, expect()) {
            let _ = std::fs::remove_file(&joined);
            return Err(DeliverError::Failed(error));
        }
        joined
    } else {
        // Nothing can be joined into this name and the track is not in it:
        // saving the bytes under it would be a lie about the file.
        return Err(DeliverError::Failed(format!(
            "the download is not the {} it should be",
            delivery.extension
        )));
    };

    stage("Saving");
    // A single track that already is the file its name promises can be sitting
    // at the destination itself. Renaming a file onto itself would move the
    // user's file out from under the name they are looking at.
    if staged == dest {
        return Ok(dest.to_path_buf());
    }
    disk::replace_file(&staged, dest).map_err(|error| {
        DeliverError::Failed(format!(
            "the finished file could not be put in place: {error}"
        ))
    })?;
    Ok(dest.to_path_buf())
}

/// A callback that writes the job's stage, for the blocking delivery step.
fn stage_sink(app: &AppHandle, id: u64) -> Arc<dyn Fn(&str) + Send + Sync> {
    let app = app.clone();
    Arc::new(move |text: &str| {
        app.state::<AppState>().update_media_job(id, |job| {
            job.stage = Some(text.to_string());
        });
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_hls(
    app: &AppHandle,
    job: &MediaJob,
    request: &DownloadRequest,
    master: Option<&hls::Master>,
    stored: Option<&hls::MediaPlaylist>,
    base: &str,
    work_dir: &Path,
    limits: &idm_core::limits::Limits,
    connections: u32,
    control: &Arc<TrackControl>,
) -> Result<(), Outcome> {
    let playlist = if request.video == "media" {
        stored
            .cloned()
            .ok_or_else(|| Outcome::media("the playlist is not there any more"))?
    } else {
        // A variant, or an audio-only rendition chosen on its own row.
        let uri = match master {
            Some(master) => match master.variant(&request.video) {
                Some(variant) => Some(variant.uri.clone()),
                None => master
                    .audio_rendition(&request.video)
                    .map(|audio| audio.uri.clone()),
            },
            None => None,
        }
        .ok_or_else(|| Outcome::media("that quality is not in the manifest"))?;
        let fetched = super::probe::fetch(&uri, &job.referer, &job.headers, job.header_origin())
            .await
            .map_err(Outcome::transient)?;
        hls::parse_media(&fetched.text, &fetched.url)
            .map_err(|error| Outcome::media(error.to_string()))?
    };
    let _ = base;
    if playlist.discontinuities > 0 {
        // The segments are joined in the order the playlist lists them, and
        // each plain segment carries its own timestamps. A stream whose
        // timeline restarts is worth a note in the log rather than a silent
        // assumption that it does not matter.
        idm_core::log::write(
            "info",
            &format!(
                "the playlist restarts its timeline {} time(s); the segments are joined in the order it lists them",
                playlist.discontinuities
            ),
        );
    }

    // The sound playlist is read first, only to know how many segments the
    // whole job has. That count is what keeps the row's percentage stable
    // while the picture is downloading and the sound has not started.
    let audio_playlist = if let Some(audio_id) = request.audio.as_deref() {
        let rendition = master
            .and_then(|master| master.audio_rendition(audio_id))
            .ok_or_else(|| Outcome::media("that audio track is not in the manifest"))?;
        let fetched = super::probe::fetch(
            &rendition.uri,
            &job.referer,
            &job.headers,
            job.header_origin(),
        )
        .await
        .map_err(Outcome::transient)?;
        Some(
            hls::parse_media(&fetched.text, &fetched.url)
                .map_err(|error| Outcome::media(error.to_string()))?,
        )
    } else {
        None
    };
    let video_segments = playlist.segments.len();
    let audio_segments = audio_playlist
        .as_ref()
        .map(|playlist| playlist.segments.len())
        .unwrap_or(0);
    let total_segments = video_segments + audio_segments;
    let last_video = Arc::new(Mutex::new(TrackProgress::default()));

    let track = TrackSource {
        init: playlist.init.as_ref().map(segment),
        segments: playlist.segments.iter().map(segment).collect(),
        dest: video_track_path(work_dir),
        work_dir: work_dir.to_path_buf(),
        stem: "video".to_string(),
        connections,
        proxy: app.state::<AppState>().limits().proxy(),
        referer: Some(job.referer.clone()).filter(|referer| !referer.is_empty()),
        headers: job.headers.clone(),
        header_origin: Some(job.header_origin.clone()).filter(|origin| !origin.is_empty()),
        // A playlist is re-read every run, but the segments already on disk
        // belong to the playlist of the run that wrote them. When a live
        // window has slid on, the index files no longer mean the same bytes.
        fingerprint: hls_fingerprint(&playlist),
    };

    run_track(
        limits,
        track,
        control,
        progress_sink(
            app.clone(),
            job.id,
            "Video",
            ProgressBase::default(),
            0,
            total_segments,
            Some(last_video.clone()),
        ),
    )
    .await?;

    if let Some(audio_playlist) = audio_playlist {
        let video = *last_video
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let base = ProgressBase {
            downloaded: video.downloaded,
            total: video.total,
        };
        let audio = TrackSource {
            init: audio_playlist.init.as_ref().map(segment),
            segments: audio_playlist.segments.iter().map(segment).collect(),
            dest: audio_track_path(work_dir),
            work_dir: work_dir.to_path_buf(),
            stem: "audio".to_string(),
            connections,
            proxy: app.state::<AppState>().limits().proxy(),
            referer: Some(job.referer.clone()).filter(|referer| !referer.is_empty()),
            headers: job.headers.clone(),
            header_origin: Some(job.header_origin.clone()).filter(|origin| !origin.is_empty()),
            fingerprint: hls_fingerprint(&audio_playlist),
        };
        run_track(
            limits,
            audio,
            control,
            progress_sink(
                app.clone(),
                job.id,
                "Audio",
                base,
                video_segments,
                total_segments,
                None,
            ),
        )
        .await?;
    }

    Ok(())
}

async fn run_track(
    limits: &idm_core::limits::Limits,
    source: TrackSource,
    control: &Arc<TrackControl>,
    sink: ProgressSink,
) -> Result<(), Outcome> {
    match download::download_track_typed(limits.clone(), source, Some(control.clone()), Some(sink))
        .await
    {
        Ok(TrackOutcome::Done) => Ok(()),
        Ok(TrackOutcome::Cancelled) => Err(Outcome::Cancelled),
        Err(error) => Err(Outcome::Failed(error)),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_dash(
    app: &AppHandle,
    job: &MediaJob,
    request: &DownloadRequest,
    tracks: &[idm_core::media::dash::DashTrack],
    work_dir: &Path,
    limits: &idm_core::limits::Limits,
    connections: u32,
    control: &Arc<TrackControl>,
) -> Result<(), Outcome> {
    let track = tracks
        .iter()
        .find(|track| track.format.id == request.video)
        .ok_or_else(|| Outcome::media("that quality is not in the manifest"))?;
    let audio_track = if let Some(audio_id) = request.audio.as_deref() {
        Some(
            tracks
                .iter()
                .find(|track| track.format.id == audio_id)
                .ok_or_else(|| Outcome::media("that audio track is not in the manifest"))?,
        )
    } else {
        None
    };
    let video_segments = track.segments.len();
    let audio_segments = audio_track.map(|track| track.segments.len()).unwrap_or(0);
    let total_segments = video_segments + audio_segments;
    let last_video = Arc::new(Mutex::new(TrackProgress::default()));

    let source = TrackSource {
        init: track.init.clone(),
        segments: track.segments.clone(),
        dest: video_track_path(work_dir),
        work_dir: work_dir.to_path_buf(),
        stem: "video".to_string(),
        connections,
        proxy: app.state::<AppState>().limits().proxy(),
        referer: Some(job.referer.clone()).filter(|referer| !referer.is_empty()),
        headers: job.headers.clone(),
        header_origin: Some(job.header_origin.clone()).filter(|origin| !origin.is_empty()),
        fingerprint: dash_fingerprint(track),
    };
    run_track(
        limits,
        source,
        control,
        progress_sink(
            app.clone(),
            job.id,
            "Video",
            ProgressBase::default(),
            0,
            total_segments,
            Some(last_video.clone()),
        ),
    )
    .await?;

    if let Some(audio) = audio_track {
        let video = *last_video
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let base = ProgressBase {
            downloaded: video.downloaded,
            total: video.total,
        };
        let source = TrackSource {
            init: audio.init.clone(),
            segments: audio.segments.clone(),
            dest: audio_track_path(work_dir),
            work_dir: work_dir.to_path_buf(),
            stem: "audio".to_string(),
            connections,
            proxy: app.state::<AppState>().limits().proxy(),
            referer: Some(job.referer.clone()).filter(|referer| !referer.is_empty()),
            headers: job.headers.clone(),
            header_origin: Some(job.header_origin.clone()).filter(|origin| !origin.is_empty()),
            fingerprint: dash_fingerprint(audio),
        };
        run_track(
            limits,
            source,
            control,
            progress_sink(
                app.clone(),
                job.id,
                "Audio",
                base,
                video_segments,
                total_segments,
                None,
            ),
        )
        .await?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_files(
    app: &AppHandle,
    job: &MediaJob,
    request: &DownloadRequest,
    tracks: &[super::FileTrack],
    work_dir: &Path,
    limits: &idm_core::limits::Limits,
    connections: u32,
    control: &Arc<TrackControl>,
) -> Result<(), Outcome> {
    let chosen = tracks
        .iter()
        .find(|track| track.format.id == request.video)
        .ok_or_else(|| Outcome::media("that quality is not in the manifest"))?;
    let audio_track = if let Some(audio_id) = request.audio.as_deref() {
        Some(
            tracks
                .iter()
                .find(|track| track.format.id == audio_id)
                .ok_or_else(|| Outcome::media("that audio track is not in the manifest"))?,
        )
    } else {
        None
    };
    let total_segments = 1 + usize::from(audio_track.is_some());
    let last_video = Arc::new(Mutex::new(TrackProgress::default()));
    let track = TrackSource {
        init: None,
        segments: vec![TrackSegment {
            url: chosen.url().to_string(),
            range: None,
        }],
        dest: video_track_path(work_dir),
        work_dir: work_dir.to_path_buf(),
        stem: "video".to_string(),
        connections,
        proxy: app.state::<AppState>().limits().proxy(),
        referer: Some(job.referer.clone()).filter(|referer| !referer.is_empty()),
        headers: track_headers(job, &chosen.lease),
        header_origin: Some(job.header_origin.clone()).filter(|origin| !origin.is_empty()),
        fingerprint: Some(chosen.lease.representation.key()),
    };
    run_track(
        limits,
        track,
        control,
        progress_sink(
            app.clone(),
            job.id,
            "Video",
            ProgressBase::default(),
            0,
            total_segments,
            Some(last_video.clone()),
        ),
    )
    .await?;

    if let Some(audio) = audio_track {
        let video = *last_video
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let base = ProgressBase {
            downloaded: video.downloaded,
            total: video.total,
        };
        let source = TrackSource {
            init: None,
            segments: vec![TrackSegment {
                url: audio.url().to_string(),
                range: None,
            }],
            dest: audio_track_path(work_dir),
            work_dir: work_dir.to_path_buf(),
            stem: "audio".to_string(),
            connections,
            proxy: app.state::<AppState>().limits().proxy(),
            referer: Some(job.referer.clone()).filter(|referer| !referer.is_empty()),
            headers: track_headers(job, &audio.lease),
            header_origin: Some(job.header_origin.clone()).filter(|origin| !origin.is_empty()),
            fingerprint: Some(audio.lease.representation.key()),
        };
        run_track(
            limits,
            source,
            control,
            progress_sink(app.clone(), job.id, "Audio", base, 1, total_segments, None),
        )
        .await?;
    }

    Ok(())
}

#[derive(Clone, Copy, Default)]
struct ProgressBase {
    downloaded: u64,
    total: u64,
}

/// A sink that adds the finished tracks to this one and keeps the segment
/// count over the whole job, so the row's percentage never walks backwards.
fn progress_sink(
    app: AppHandle,
    id: u64,
    stage: &'static str,
    base: ProgressBase,
    segments_before: usize,
    segments_total: usize,
    capture: Option<Arc<Mutex<TrackProgress>>>,
) -> ProgressSink {
    Arc::new(move |progress| {
        if let Some(capture) = &capture {
            if let Ok(mut last) = capture.lock() {
                *last = progress;
            }
        }
        let combined = TrackProgress {
            downloaded: base.downloaded + progress.downloaded,
            total: base.total + progress.total,
            segments_done: segments_before + progress.segments_done,
            segments_total,
        };
        let state = app.state::<AppState>();
        state.update_media_job(id, |job| {
            job.stage = Some(stage.to_string());
            job.progress = combined;
        });
    })
}

fn finish_saved(state: &AppState, id: u64, dest: PathBuf) {
    state.update_media_job(id, |job| {
        job.downloading = false;
        job.done = true;
        job.error = None;
        job.stage = Some("Saved".to_string());
        job.dest = Some(dest.to_string_lossy().to_string());
    });
}

fn finish_failed(state: &AppState, id: u64, error: String) {
    state.update_media_job(id, |job| {
        job.downloading = false;
        job.dest = None;
        job.error = Some(error);
        job.stage = Some("Failed".to_string());
    });
}

/// The headers a track request carries: the page's, scoped to the origin
/// they were collected from, plus the identity the address was signed for.
///
/// The lease decides once more rather than trusting the job: a redirect can
/// leave the origin, and a `user-agent` from the page must never be sent
/// beside the one the provider signed the address for — a server that sees
/// two identities refuses the request.
fn track_headers(job: &MediaJob, lease: &RequestLease) -> Vec<(String, String)> {
    lease.headers_for(&lease.url, &job.headers, job.header_origin())
}

fn segment(segment: &hls::HlsSegment) -> TrackSegment {
    TrackSegment {
        url: segment.url.clone(),
        range: segment.range,
    }
}

fn video_track_path(work_dir: &Path) -> PathBuf {
    work_dir.join("video.track")
}

fn audio_track_path(work_dir: &Path) -> PathBuf {
    work_dir.join("audio.track")
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

#[cfg(test)]
mod tests {
    use super::*;
    use idm_core::media::MediaKind;

    #[test]
    fn the_extension_follows_the_container() {
        let mut format = Format::new("v", MediaKind::Video, "mp4");
        assert_eq!(output_container(&format), "mp4");
        format.container = "ts".to_string();
        assert_eq!(output_container(&format), "ts");
        format.container = "webm".to_string();
        assert_eq!(output_container(&format), "webm");
        format.container = "m4a".to_string();
        assert_eq!(output_container(&format), "m4a");
    }

    #[test]
    fn a_name_gets_one_extension() {
        assert_eq!(with_extension("Film", "mp4"), "Film.mp4");
        assert_eq!(with_extension("Film.mp4", "mp4"), "Film.mp4");
        assert_eq!(with_extension("Film.old.mp4", "mp4"), "Film.old.mp4");
        assert_eq!(with_extension("Film.old", "mp4"), "Film.old.mp4");
        assert_eq!(with_extension("Film.ts", "mp4"), "Film.mp4");
        assert_eq!(with_extension("Dr. No", "mp4"), "Dr. No.mp4");
    }

    #[test]
    fn a_component_cannot_leave_the_folder() {
        assert_eq!(safe_component("a/b\\c:d"), "a_b_c_d");
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bdm-deliver-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        dir
    }

    fn delivery(extension: &str) -> Delivery {
        Delivery {
            extension: extension.to_string(),
            container: None,
            video: true,
            audio: false,
            audio_selected: false,
        }
    }

    #[test]
    fn a_file_that_is_already_the_destination_is_left_where_it_is() {
        let dir = scratch("in-place");
        let dest = dir.join("Film.webm");
        std::fs::write(&dest, super::super::tools::tiny_webm()).expect("the file");

        // The track and the destination are the same path: a download resumed
        // straight into the folder it belongs to.
        let saved = deliver(&dest, vec![dest.clone()], &dir, &delivery("webm"), &|_| {})
            .expect("a file that checks out");

        assert_eq!(saved, dest);
        assert!(dest.is_file(), "the destination is still there");
        assert_eq!(
            std::fs::read(&dest).expect("the file"),
            super::super::tools::tiny_webm()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_does_not_check_out_never_replaces_the_destination() {
        let dir = scratch("check");
        let dest = dir.join("Film.webm");
        let old = b"the file that was already there";
        std::fs::write(&dest, old).expect("the old file");

        // A WebM header with no segment and no track: it is the container the
        // name promises, and it holds nothing a person could watch.
        let track = dir.join("video.track");
        std::fs::write(
            &track,
            super::super::tools::ebml(
                &[0x1A, 0x45, 0xDF, 0xA3],
                &super::super::tools::ebml(&[0x42, 0x82], b"webm"),
            ),
        )
        .expect("the track");

        let failed = match deliver(&dest, vec![track], &dir, &delivery("webm"), &|_| {}) {
            Ok(_) => panic!("a file with no picture cannot be saved"),
            Err(error) => error,
        };
        assert!(matches!(failed, DeliverError::Failed(_)));
        assert_eq!(
            std::fs::read(&dest).expect("the old file"),
            old,
            "the destination the user had is untouched"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_join_that_fails_never_replaces_the_destination() {
        let dir = scratch("join");
        let dest = dir.join("Film.mp4");
        let old = b"the file that was already there";
        std::fs::write(&dest, old).expect("the old file");

        // Text where an MP4 should be: the join refuses it before anything is
        // moved into the destination.
        let track = dir.join("video.track");
        std::fs::write(&track, b"<!doctype html><title>Not found</title>").expect("the track");

        let failed = match deliver(&dest, vec![track], &dir, &delivery("mp4"), &|_| {}) {
            Ok(_) => panic!("an HTML page is not a video"),
            Err(error) => error,
        };
        assert!(matches!(failed, DeliverError::Failed(_)));
        assert_eq!(
            std::fs::read(&dest).expect("the old file"),
            old,
            "the destination the user had is untouched"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
