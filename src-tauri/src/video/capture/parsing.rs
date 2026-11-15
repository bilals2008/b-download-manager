use idm_core::media::youtube;
use idm_core::media::{dash, hls, quality, Format, Manifest, MediaKind, Pair};
use tauri::{AppHandle, Manager};

use crate::state::AppState;

use super::helpers::{
    file_container, infer_kind, is_audio, looks_like_mpd, refused_content_type, title_of,
    youtube_params,
};
use crate::video::{lease, FileTrack, SourcePlan};
use super::{MediaJob, MediaKindHint, MediaState};


/* --------------------------------------------------------------- parsing */

pub(super) async fn parse_job(app: AppHandle, id: u64) {
    let job = match app.state::<AppState>().media_job(id) {
        Some(job) => job,
        None => return,
    };
    let result = parse_manifest(&app, &job).await;
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
                    let notice = crate::video::download::is_youtube_job(&job).then(|| {
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
                        crate::video::window::show(&app, id);
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
                crate::video::window::show(&app, id);
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

pub(super) async fn parse_manifest(app: &AppHandle, job: &MediaJob) -> Result<(Manifest, SourcePlan), String> {
    // A YouTube page is resolved through the video helper when it is
    // installed, and through the in-house player clients otherwise; the page's
    // own formats carry no usable `url`.
    if job.kind == MediaKindHint::Youtube || crate::video::yt::is_youtube_page(&job.page_url) {
        return resolve_youtube(app, job).await;
    }
    match infer_kind(job.kind, &job.manifest_url) {
        MediaKindHint::Youtube => resolve_youtube(app, job).await,
        MediaKindHint::Hls => parse_hls(job).await,
        MediaKindHint::Dash => parse_dash(job).await,
        MediaKindHint::File => parse_file(job).await,
        MediaKindHint::Auto => {
            // The address does not say what it is. Read it as text: a
            // manifest is small, and anything that is not one is refused
            // rather than downloaded as a page.
            let fetched = crate::video::probe::fetch(
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

/// The page resolver reports typed reasons (unplayable, members only, live,
/// SABR only); the panel shows the sentence, so the message is what travels.
pub(super) async fn resolve_youtube(
    app: &AppHandle,
    job: &MediaJob,
) -> Result<(Manifest, SourcePlan), String> {
    crate::video::ytdlp::resolve_page(app, &job.page_url, &job.title).await
}

pub(super) async fn parse_hls(job: &MediaJob) -> Result<(Manifest, SourcePlan), String> {
    let fetched = crate::video::probe::fetch(
        &job.manifest_url,
        &job.referer,
        &job.headers,
        job.header_origin(),
    )
    .await?;
    parse_hls_text(job, fetched)
}

pub(super) fn parse_hls_text(
    job: &MediaJob,
    fetched: crate::video::probe::Fetched,
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
            subtitles: Vec::new(),
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

pub(super) async fn parse_dash(job: &MediaJob) -> Result<(Manifest, SourcePlan), String> {
    let fetched = crate::video::probe::fetch(
        &job.manifest_url,
        &job.referer,
        &job.headers,
        job.header_origin(),
    )
    .await?;
    parse_dash_text(job, fetched)
}

pub(super) fn parse_dash_text(
    job: &MediaJob,
    fetched: crate::video::probe::Fetched,
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

pub(super) async fn parse_file(job: &MediaJob) -> Result<(Manifest, SourcePlan), String> {
    let probe = crate::video::probe::probe_file(
        &job.manifest_url,
        &job.referer,
        &job.headers,
        job.header_origin(),
    )
    .await?;
    if let Some(refusal) = probe.content_type.as_deref().and_then(refused_content_type) {
        return Err(refusal.to_string());
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

    // A plain file was pointed at by the page itself, so the page's own
    // headers are the identity it expects — but only back to the origin they
    // were collected from, and only when the job's consent covers them.
    let origin = lease::origin_of(&probe.url);
    let same_origin = job
        .header_origin()
        .is_some_and(|page| page.eq_ignore_ascii_case(&origin));
    let user_agent = if same_origin {
        job.headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
            .map(|(_, value)| value.clone())
    } else {
        None
    };
    let credentials = lease::Credentials {
        origin,
        cookies: job.cookie_consent && job.cookies_offered,
    };
    let lease = lease::RequestLease::new(
        probe.url,
        lease::Authority::Browser,
        "browser",
        None,
        user_agent,
        credentials,
        lease::Transport::Progressive,
        lease::RepresentationFingerprint {
            id: format.id.clone(),
            container: format.container.clone(),
            codec: format.codec.clone(),
            bytes: format.size,
        },
        None,
    );
    Ok((
        manifest,
        SourcePlan::Files {
            tracks: vec![FileTrack { format, lease }],
        },
    ))
}

/// The visible manifest of plain files: every picture paired with a sound
/// track in the same container, audio-only files left as their own row.
pub(super) fn file_manifest(title: String, mut formats: Vec<Format>) -> Manifest {
    let mut pairs = quality::pair_file_formats(&formats);
    quality::finish(&mut formats, &mut pairs, None);
    Manifest {
        title,
        duration: None,
        formats,
        pairs,
        container_hint: Some("file".to_string()),
        subtitles: Vec::new(),
    }
}
