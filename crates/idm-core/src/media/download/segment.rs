//! Fetching one segment: the whole-file download or the ranged transfer.

use super::*;

pub(super) enum FetchFailure {
    Cancelled,
    Failed(TrackError),
}

impl FetchFailure {
    /// A failure that no address can fix: the local disk, a malformed
    /// manifest, a body that is not the media that was asked for.
    fn media(message: impl Into<String>) -> Self {
        FetchFailure::Failed(TrackError::media(message))
    }

    /// A failure that a later attempt may pass.
    fn transient(message: impl Into<String>) -> Self {
        FetchFailure::Failed(TrackError::transient(message))
    }

    /// A failure of the disk rather than of the server.
    fn storage(message: impl Into<String>) -> Self {
        FetchFailure::Failed(TrackError::storage(message))
    }
}

/// Turn an engine failure into the reason a caller can act on.
fn classify(error: &TransferError) -> TrackError {
    match error.kind {
        FailureKind::Changed => TrackError::changed(error.message.clone()),
        FailureKind::RangeIgnored => TrackError::changed(error.message.clone()),
        FailureKind::Transient => TrackError::transient(error.message.clone()),
        FailureKind::Permanent => match error.status {
            Some(status) => TrackError::refused(status, error.message.clone()),
            None => TrackError::media(error.message.clone()),
        },
    }
}

/// Fetch one segment, by the engine's whole-file download or its ranged
/// transfer, depending on what the manifest said.
#[allow(clippy::too_many_arguments)]
pub(super) async fn fetch_segment(
    limits: &Limits,
    segment: &TrackSegment,
    file: &Path,
    index: usize,
    state: &Arc<Mutex<ProgressState>>,
    progress: &Option<ProgressSink>,
    control: Option<&TrackControl>,
    proxy: Option<&str>,
    referer: Option<&str>,
    connections: u32,
    headers: &[(String, String)],
    header_origin: Option<&str>,
) -> Result<(), FetchFailure> {
    let headers = headers_for(&segment.url, headers, header_origin);
    match segment.range {
        Some(range) => {
            fetch_window(
                segment, file, range, index, state, progress, control, proxy, referer, &headers,
            )
            .await
        }
        None => {
            fetch_whole(
                limits,
                segment,
                file,
                index,
                state,
                progress,
                control,
                proxy,
                referer,
                connections,
                &headers,
            )
            .await
        }
    }
}

/// A segment whose playlist gave no byte range: an ordinary download.
#[allow(clippy::too_many_arguments)]
async fn fetch_whole(
    limits: &Limits,
    segment: &TrackSegment,
    file: &Path,
    index: usize,
    state: &Arc<Mutex<ProgressState>>,
    progress: &Option<ProgressSink>,
    control: Option<&TrackControl>,
    proxy: Option<&str>,
    referer: Option<&str>,
    connections: u32,
    headers: &[(String, String)],
) -> Result<(), FetchFailure> {
    let download = Download::new_in_with_headers(
        index as u64,
        segment.url.clone(),
        file.to_path_buf(),
        file.parent(),
        connections.clamp(1, 32),
        proxy,
        referer,
        headers,
    )
    .map_err(FetchFailure::media)?;

    if let Some(control) = control {
        if !control.attach(&download) {
            return Err(FetchFailure::Cancelled);
        }
    }

    let reporter: Arc<dyn Reporter> = Arc::new(SegmentReporter {
        state: state.clone(),
        sink: progress.clone(),
    });
    downloader::run(reporter, limits.clone(), download.clone()).await;

    if let Some(control) = control {
        control.detach();
    }
    if download.status() == Status::Cancelled || is_cancelled(control) {
        return Err(FetchFailure::Cancelled);
    }
    if download.status() != Status::Completed {
        let message = download
            .info()
            .error
            .unwrap_or_else(|| "the segment did not finish".to_string());
        // The status the engine last saw is the difference between "this
        // address is no longer accepted" and "the network blinked".
        return Err(FetchFailure::Failed(match download.last_status() {
            Some(status) => TrackError::refused(status, message),
            None => TrackError::transient(message),
        }));
    }

    mark_done(state, index, download.info().total);
    Ok(())
}

/// A segment with a byte range into a larger resource.
///
/// The engine's transfer is used directly: there is no whole file to plan, and
/// the part file is the segment file, so its length is the resume point.
#[allow(clippy::too_many_arguments)]
async fn fetch_window(
    segment: &TrackSegment,
    file: &Path,
    range: (u64, u64),
    index: usize,
    state: &Arc<Mutex<ProgressState>>,
    progress: &Option<ProgressSink>,
    control: Option<&TrackControl>,
    proxy: Option<&str>,
    referer: Option<&str>,
    headers: &[(String, String)],
) -> Result<(), FetchFailure> {
    let (start, end) = range;
    if end < start {
        return Err(FetchFailure::media("the byte range ends before it starts"));
    }
    let expected = end - start + 1;
    let mut written = file_len(file).unwrap_or(0);
    if written == expected {
        mark_done(state, index, expected);
        return Ok(());
    }
    if written > expected {
        let _ = std::fs::remove_file(file);
        written = 0;
    }

    let source = Source::parse_with_headers(&segment.url, proxy, referer, headers)
        .map_err(FetchFailure::media)?;

    let mut attempt = 0u32;
    loop {
        if is_cancelled(control) {
            return Err(FetchFailure::Cancelled);
        }
        let from = start + written;
        if from > end {
            break;
        }
        match source.open(from, Some(end), false, None, None).await {
            Ok(mut stream) => {
                let mut out = std::fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(file)
                    .map_err(|error| FetchFailure::storage(error.to_string()))?;
                use std::io::{Seek, SeekFrom, Write};
                out.seek(SeekFrom::Start(written))
                    .map_err(|error| FetchFailure::storage(error.to_string()))?;

                while let Some(item) = stream.next().await {
                    if is_cancelled(control) {
                        return Err(FetchFailure::Cancelled);
                    }
                    let bytes = item.map_err(FetchFailure::transient)?;
                    if bytes.is_empty() {
                        continue;
                    }
                    out.write_all(&bytes)
                        .map_err(|error| FetchFailure::storage(error.to_string()))?;
                    written += bytes.len() as u64;
                    update(state, index, written.min(expected), expected);
                    report(state, progress);
                    if written >= expected {
                        break;
                    }
                }
                out.flush()
                    .map_err(|error| FetchFailure::storage(error.to_string()))?;
                if written >= expected {
                    break;
                }
                // The response ended early; ask for the rest from where the
                // file stops, after a short wait.
                attempt += 1;
                if attempt >= 8 {
                    return Err(FetchFailure::transient(format!(
                        "the response ended after {written} of {expected} bytes"
                    )));
                }
                tokio::time::sleep(http::backoff(attempt)).await;
            }
            Err(error) => {
                if !error.is_transient() {
                    return Err(FetchFailure::Failed(classify(&error)));
                }
                attempt += 1;
                if attempt >= 8 {
                    let message = format!(
                        "the range failed after {attempt} attempts: {}",
                        error.message
                    );
                    return Err(FetchFailure::Failed(classify(&error).wrapped(message)));
                }
                tokio::time::sleep(error.retry_after.unwrap_or_else(|| http::backoff(attempt)))
                    .await;
            }
        }
    }

    if file_len(file) != Some(expected) {
        return Err(FetchFailure::storage(
            "the segment file is shorter than the range it should hold",
        ));
    }
    mark_done(state, index, expected);
    Ok(())
}

/// Join the files in order into `dest`, through a temporary file.
pub(super) fn join(files: &[PathBuf], dest: &Path) -> Result<(), String> {
    use std::io::{Read, Write};

    if let Some(parent) = dest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("the unfinished folder could not be made: {error}"))?;
    }
    let temporary = join_temporary(dest);
    {
        let mut out = std::fs::File::create(&temporary)
            .map_err(|error| format!("the track could not be written: {error}"))?;
        let mut buffer = vec![0u8; 1024 * 1024];
        for file in files {
            let mut input = std::fs::File::open(file)
                .map_err(|error| format!("{} could not be read: {error}", file.display()))?;
            loop {
                let read = input
                    .read(&mut buffer)
                    .map_err(|error| format!("{}: {error}", file.display()))?;
                if read == 0 {
                    break;
                }
                out.write_all(&buffer[..read])
                    .map_err(|error| format!("the track could not be written: {error}"))?;
            }
        }
        out.flush()
            .map_err(|error| format!("the track could not be written: {error}"))?;
        out.sync_all()
            .map_err(|error| format!("the track could not be written: {error}"))?;
    }
    disk::replace_file(&temporary, dest).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("the track could not be put in place: {error}")
    })
}

/// The name the join is assembled under, beside the track it becomes.
pub(super) fn join_temporary(dest: &Path) -> PathBuf {
    dest.with_file_name(format!(
        "{}.idmtrack",
        dest.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "track".to_string())
    ))
}
