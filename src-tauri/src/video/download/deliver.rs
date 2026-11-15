//! Putting the finished tracks in place and reading the result back.

use super::*;

/// Why the finished file could not be delivered. As visible as [`deliver`],
/// which answers with it: a type a caller can receive cannot be more private
/// than the function that returns it.
#[derive(Debug)]
pub(super) enum DeliverError {
    Cancelled,
    Failed(String),
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
    // A conversion is a sound-only outcome: the picture is not written at all,
    // so the join machinery is not reached.
    if let Some(selector) = &delivery.convert {
        return deliver_conversion(dest, inputs, work_dir, selector, delivery, stage);
    }
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
        match verify::check_finished(&primary, expect()) {
            Ok(_) => primary,
            Err(error) => match (container, delivery.ffmpeg.as_deref()) {
                // The bytes are the container the name promises, but their
                // index does not describe them. ffmpeg reads the samples and
                // writes a fresh index while copying them.
                (Some(container), Some(ffmpeg)) => {
                    stage("Joining");
                    let joined = work_dir.join(format!("final.{}", delivery.extension));
                    let _ = std::fs::remove_file(&joined);
                    mux::join_ffmpeg(&inputs, &joined, container, ffmpeg)
                        .map_err(delivery_error)?;
                    stage("Checking");
                    if let Err(second) = verify::check_finished(&joined, expect()) {
                        let _ = std::fs::remove_file(&joined);
                        return Err(DeliverError::Failed(second));
                    }
                    joined
                }
                _ => return Err(DeliverError::Failed(error)),
            },
        }
    } else if let Some(container) = container {
        stage("Joining");
        let joined = work_dir.join(format!("final.{}", delivery.extension));
        let _ = std::fs::remove_file(&joined);
        mux::join(
            &inputs,
            &joined,
            work_dir,
            container,
            delivery.ffmpeg.as_deref(),
        )
        .map_err(delivery_error)?;
        stage("Checking");
        if let Err(error) = verify::check_finished(&joined, expect()) {
            let _ = std::fs::remove_file(&joined);
            let Some(ffmpeg) = delivery.ffmpeg.as_deref() else {
                return Err(DeliverError::Failed(error));
            };
            // The join produced a file its own index cannot describe; ffmpeg
            // writes the samples and the index again.
            stage("Joining");
            mux::join_ffmpeg(&inputs, &joined, container, ffmpeg).map_err(delivery_error)?;
            stage("Checking");
            if let Err(second) = verify::check_finished(&joined, expect()) {
                let _ = std::fs::remove_file(&joined);
                return Err(DeliverError::Failed(second));
            }
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

    // The captions go in while the file is still one step from the name the
    // reader asked for: the picture and the sound are copied as they are and
    // the text is written into the container the name promises.
    let staged = match &delivery.captions {
        None => staged,
        Some(captions) => {
            stage("Joining");
            let Some(ffmpeg) = delivery.ffmpeg.as_deref() else {
                return Err(DeliverError::Failed(
                    "the captions need FFmpeg; install it from the video settings page"
                        .to_string(),
                ));
            };
            let codec = caption_codec(&delivery.extension).ok_or_else(|| {
                DeliverError::Failed(format!(
                    "captions cannot go into .{}",
                    delivery.extension
                ))
            })?;
            let with_captions = work_dir.join(format!("final-captions.{}", delivery.extension));
            let _ = std::fs::remove_file(&with_captions);
            let answer = std::process::Command::new(ffmpeg)
                .arg("-y")
                .arg("-i")
                .arg(&staged)
                .arg("-i")
                .arg(captions)
                .args(["-c", "copy", "-c:s"])
                .arg(codec)
                .arg(&with_captions)
                .output();
            match answer {
                Ok(output) if output.status.success() => {}
                Ok(output) => {
                    let tail = String::from_utf8_lossy(&output.stderr)
                        .lines()
                        .rev()
                        .find(|line| !line.trim().is_empty())
                        .unwrap_or("unknown")
                        .to_string();
                    let _ = std::fs::remove_file(&with_captions);
                    return Err(DeliverError::Failed(format!(
                        "the captions could not be written: {tail}"
                    )));
                }
                Err(error) => {
                    return Err(DeliverError::Failed(format!(
                        "FFmpeg could not be started: {error}"
                    )));
                }
            }
            stage("Checking");
            if let Err(error) = verify::check_finished(&with_captions, expect()) {
                let _ = std::fs::remove_file(&with_captions);
                return Err(DeliverError::Failed(error));
            }
            // The intermediate is superseded by the captioned one; it does
            // not belong on disk next to the file that was asked for.
            let _ = std::fs::remove_file(&staged);
            with_captions
        }
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

/// The FFmpeg codec a caption track goes to for this container.
fn caption_codec(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "mp4" => Some("mov_text"),
        // An SRT track goes into Matroska as S_TEXT/UTF8, as it is.
        "mkv" => Some("copy"),
        // WebVTT is written as WebVTT; text to text, never a transcode of the
        // picture.
        "webm" => Some("webvtt"),
        _ => None,
    }
}

/// A conversion, from the downloaded track to the format the reader asked
/// for, with the name of the video and not the temporary track file.
fn deliver_conversion(
    dest: &Path,
    inputs: Vec<PathBuf>,
    work_dir: &Path,
    selector: &idm_core::media::audio::AudioSelector,
    delivery: &Delivery,
    stage: &dyn Fn(&str),
) -> Result<PathBuf, DeliverError> {
    stage("Converting");
    // The sound is the thing that is wanted; the picture is not written.
    let input = inputs
        .get(1)
        .cloned()
        .or_else(|| inputs.first().cloned())
        .ok_or_else(|| DeliverError::Failed("there is nothing to convert".to_string()))?;
    let ffmpeg = delivery.ffmpeg.as_deref().ok_or_else(|| {
        DeliverError::Failed(
            "saving as this format needs FFmpeg; install it from the video settings page"
                .to_string(),
        )
    })?;
    let output = work_dir.join(format!("converted.{}", selector.extension()));
    let _ = std::fs::remove_file(&output);
    let mut command = std::process::Command::new(ffmpeg);
    command
        .arg("-y")
        .arg("-i")
        .arg(&input)
        .arg("-vn")
        .arg("-c:a")
        .arg(
            selector
                .ffmpeg_codec()
                .expect("a conversion has an ffmpeg codec"),
        );
    if let idm_core::media::audio::AudioSelector::Mp3(
        idm_core::media::audio::Mp3Quality::Constant(rate),
    ) = selector
    {
        command.arg("-b:a").arg(format!("{rate}k"));
    }
    command.arg(&output);
    let answer = command
        .output()
        .map_err(|error| DeliverError::Failed(format!("FFmpeg could not be started: {error}")))?;
    if !answer.status.success() {
        // The reason FFmpeg died, from its own words. A URL never appears in
        // ffmpeg's stderr.
        let tail = String::from_utf8_lossy(&answer.stderr)
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("unknown")
            .to_string();
        let _ = std::fs::remove_file(&output);
        return Err(DeliverError::Failed(format!(
            "the sound could not be written: {tail}"
        )));
    }
    let written = std::fs::metadata(&output)
        .map(|meta| meta.len())
        .unwrap_or(0);
    if written == 0 {
        let _ = std::fs::remove_file(&output);
        return Err(DeliverError::Failed(
            "the sound could not be written: the file is empty".to_string(),
        ));
    }
    stage("Saving");
    disk::replace_file(&output, dest).map_err(|error| {
        DeliverError::Failed(format!(
            "the finished file could not be put in place: {error}"
        ))
    })?;
    Ok(dest.to_path_buf())
}

/// A failed join, in the form the delivery step answers with.
fn delivery_error(error: mux::MuxError) -> DeliverError {
    match error {
        mux::MuxError::Cancelled => DeliverError::Cancelled,
        other => DeliverError::Failed(other.to_string()),
    }
}

/// A callback that writes the job's stage, for the blocking delivery step.
pub(super) fn stage_sink(app: &AppHandle, id: u64) -> Arc<dyn Fn(&str) + Send + Sync> {
    let app = app.clone();
    Arc::new(move |text: &str| {
        app.state::<AppState>().update_media_job(id, |job| {
            job.stage = Some(text.to_string());
        });
    })
}
