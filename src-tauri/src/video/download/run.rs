//! The download orchestration loop: resolve the page, run the tracks, renew a refused address once, then deliver.

use super::captions;
use super::deliver::{deliver, stage_sink, DeliverError};
use super::tracks::{
    audio_track_path, finish_failed, finish_saved, run_dash, run_files, run_hls, video_track_path,
};
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    app: &AppHandle,
    mut job: MediaJob,
    request: DownloadRequest,
    dest: PathBuf,
    work_dir: PathBuf,
    delivery: Delivery,
    control: Arc<TrackControl>,
    resume: bool,
) {
    let state = app.state::<AppState>();
    let limits = state.limits().clone();
    let connections = state.browser_connections().clamp(1, 16);

    // A YouTube address is bound to the moment it was resolved: the client
    // identity it was signed for, and a deadline the provider chose. The plan
    // the panel parsed can be minutes old by the time the user picks a
    // quality, so the page is asked once more before the first byte.
    // A resume continues the bytes already on disk, so the page is not read
    // again: a fresh answer can name another representation and the guard
    // would drop exactly the work the person asked to keep. A refused address
    // still renews below.
    if !resume && crate::video::yt::is_youtube_page(&job.page_url) {
        // A resolve that fails quietly here would send the download against a
        // dead address, so what it said is written down.
        match crate::video::ytdlp::resolve_page(app, &job.page_url, &job.title).await {
            Ok((manifest, source)) => {
                // The page is asked again seconds after the panel listed it,
                // and a second answer can be a shorter list (the site dropped
                // a rendition, or the helper fell back to the in-house
                // resolver). The quality the reader picked must survive:
                // keeping the list they chose from is better than refusing
                // before the first byte.
                if keeps_selection(&manifest, &request) {
                    job.manifest = Some(manifest);
                    job.source = Some(source);
                } else {
                    idm_core::log::write(
                        "warn",
                        "the page was read again without the chosen quality; the list the panel showed is kept",
                    );
                }
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
                    && crate::video::yt::is_youtube_page(&job.page_url) =>
            {
                refreshed = true;
                idm_core::log::write(
                    "info",
                    &format!(
                        "a track address stopped working ({}); asking the page for a new one",
                        error.problem.name()
                    ),
                );
                match crate::video::ytdlp::renew_page(app, &job.page_url, &job.title).await {
                    Ok((manifest, source)) => {
                        if !keeps_selection(&manifest, &request) {
                            idm_core::log::write(
                                "warn",
                                "the renewed page lost the chosen track; the old plan is kept",
                            );
                            break Err(Outcome::Failed(TrackError::new(
                                error.problem,
                                "the page no longer offers the chosen quality".to_string(),
                            )));
                        }
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
            // The captions go to their own file before anything else joins:
            // a caption page that cannot be read is better said now than
            // after a film's worth of bytes.
            let captions = match captions::prepare(app, &job, &request, &work_dir).await {
                Ok(captions) => captions,
                Err(message) => {
                    let _ = std::fs::remove_dir_all(&work_dir);
                    finish_failed(&state, job.id, message);
                    return;
                }
            };
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
            let mut plan = delivery.clone();
            plan.captions = captions.map(|captions| captions.path);
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
                        job.error = None;
                        job.stage = Some("Stopped".to_string());
                    });
                    state.save_media_index();
                }
                Ok(Err(DeliverError::Failed(message))) => {
                    // The tracks were downloaded once and could not be made
                    // into the file that was asked for. Keeping them only
                    // makes the next try fail in the same place; the bytes
                    // are fetched again instead.
                    let _ = std::fs::remove_dir_all(&work_dir);
                    finish_failed(&state, job.id, message);
                }
                Err(error) => {
                    let _ = std::fs::remove_dir_all(&work_dir);
                    finish_failed(&state, job.id, error.to_string());
                }
            }
        }
        Err(Outcome::Cancelled) => {
            state.update_media_job(job.id, |job| {
                job.downloading = false;
                job.error = None;
                job.stage = Some("Stopped".to_string());
            });
            state.save_media_index();
        }
        Err(Outcome::Failed(error)) => finish_failed(&state, job.id, error.to_string()),
    }
}

pub(super) enum Outcome {
    Cancelled,
    Failed(TrackError),
}

impl Outcome {
    /// A failure that no new address can fix: a manifest that does not hold
    /// the quality, a playlist that is gone, a job with no source at all.
    pub(super) fn media(message: impl Into<String>) -> Self {
        Outcome::Failed(TrackError::new(TrackProblem::Media, message))
    }

    /// A failure that may pass when the same address is asked again.
    pub(super) fn transient(message: impl Into<String>) -> Self {
        Outcome::Failed(TrackError::new(TrackProblem::Transient, message))
    }
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

/// Whether a freshly read page still names every track the reader chose.
fn keeps_selection(manifest: &idm_core::media::Manifest, request: &DownloadRequest) -> bool {
    let has = |id: &str| manifest.formats.iter().any(|format| format.id == id);
    has(&request.video) && request.audio.as_deref().map_or(true, has)
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
