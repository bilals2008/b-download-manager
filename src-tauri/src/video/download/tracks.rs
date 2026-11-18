//! Running the tracks: HLS, DASH and plain files, their progress, fingerprints and finish.

use super::run::Outcome;
use super::*;

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
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_hls(
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
        let fetched =
            crate::video::probe::fetch(&uri, &job.referer, &job.headers, job.header_origin())
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
        let fetched = crate::video::probe::fetch(
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
pub(super) async fn run_dash(
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
pub(super) async fn run_files(
    app: &AppHandle,
    job: &MediaJob,
    request: &DownloadRequest,
    tracks: &[crate::video::FileTrack],
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
        // The byte total of a track is only known once it has been probed, and
        // a picture that finishes before its sound would report 100% and then
        // drop when the sound's bytes arrive. The share of the track that is
        // done is the stable answer: the finished tracks plus the part of this
        // one, out of every track the job has.
        let track = if progress.total > 0 {
            (progress.downloaded as f64 / progress.total as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fraction = if segments_total > 0 {
            (segments_before as f64 + track) / segments_total as f64
        } else {
            0.0
        };
        let state = app.state::<AppState>();
        state.update_media_job(id, |job| {
            job.stage = Some(stage.to_string());
            job.progress = combined;
            // A resumed track can report the bytes it already had first; the
            // bar follows the highest point it has reached.
            job.fraction = job.fraction.max(fraction.clamp(0.0, 1.0));
        });
    })
}

pub(super) fn finish_saved(state: &AppState, id: u64, dest: PathBuf) {
    state.update_media_job(id, |job| {
        job.downloading = false;
        job.done = true;
        job.error = None;
        job.stage = Some("Saved".to_string());
        job.dest = Some(dest.to_string_lossy().to_string());
        job.fraction = 1.0;
    });
    state.save_media_index();
}

pub(super) fn finish_failed(state: &AppState, id: u64, error: String) {
    state.update_media_job(id, |job| {
        job.downloading = false;
        job.error = Some(error);
        job.stage = Some("Failed".to_string());
    });
    state.save_media_index();
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

pub(super) fn video_track_path(work_dir: &Path) -> PathBuf {
    work_dir.join("video.track")
}

pub(super) fn audio_track_path(work_dir: &Path) -> PathBuf {
    work_dir.join("audio.track")
}
