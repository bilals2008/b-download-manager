//! Fetching ranges: one connection per range, growing and handing over.

use super::*;

/// Hand the tail of the slowest segment to a new connection.
///
/// This is the "add an optional connection" behaviour of the reference
/// download manager: the running connection keeps the bytes it already wrote,
/// and a fresh connection continues from the point it reached.
///
/// The segment that is chosen is the one that is furthest behind in
/// *progress*, not the one with the most bytes left. Splitting by progress
/// is what keeps the connections balanced: a segment that is short but
/// nearly finished is not worth a new connection, while a segment that is
/// slow keeps holding the whole file back.
pub(super) fn maybe_grow(
    reporter: &Arc<dyn Reporter>,
    limits: &Limits,
    download: &Arc<Download>,
    set: &mut tokio::task::JoinSet<Result<(), String>>,
    epoch: u64,
) {
    let mut segments = download.segments.lock().expect("segments lock poisoned");
    if segments.len() as u32 >= download.max_connections {
        return;
    }
    // A server that was seen to ignore a range request does not get any
    // more connections.
    if download.degraded.load(Ordering::SeqCst) {
        return;
    }

    // The slowest unfinished segment, by the share of its range that has
    // arrived. The fastest one sets the bar the slowest one has to fall
    // behind before another connection is worth its keep.
    let mut slowest: Option<usize> = None;
    let mut slowest_ratio = f64::MAX;
    let mut fastest_ratio = 0.0f64;
    let mut unfinished = 0usize;

    for (index, segment) in segments.iter().enumerate() {
        if segment.done() {
            continue;
        }
        let len = segment.len();
        if len == 0 || len == UNKNOWN_END {
            continue;
        }
        unfinished += 1;
        let ratio = segment.downloaded as f64 / len as f64;
        if ratio > fastest_ratio {
            fastest_ratio = ratio;
        }
        if ratio < slowest_ratio {
            slowest_ratio = ratio;
            slowest = Some(index);
        }
    }

    let Some(index) = slowest else { return };
    let segment = &segments[index];
    let left = segment.len() - segment.downloaded;
    if left < MIN_SPLIT {
        return;
    }
    // A single unfinished range has nothing to lag behind: the next
    // alternative connection is what gives the file another one. With more
    // than one, a new connection is only worth it when the slowest really
    // lags the fastest, or every range would be split in half for no gain.
    if unfinished > 1 && slowest_ratio > fastest_ratio * 0.8 {
        return;
    }

    let source_id = segments[index].id;
    let new_id = download.next_segment_id.fetch_add(1, Ordering::SeqCst);
    let Some(new_segment) =
        segmenter::split(&mut segments[index], new_id, &download.dir, &download.stem)
    else {
        return;
    };
    segments.push(new_segment.clone());
    drop(segments);
    // The range was handed over here too, so the pacing rules do not hand the
    // same range over a second time on the next tick.
    download
        .redirected
        .lock()
        .expect("redirected lock poisoned")
        .push(source_id);

    // The new range starts empty, even when an earlier run left a file with
    // the same name behind.
    let _ = std::fs::remove_file(&new_segment.part);

    set.spawn(segment_task(
        reporter.clone(),
        limits.clone(),
        download.clone(),
        new_segment.id,
        epoch,
    ));
    emit(reporter, download);
}

/// Hand the tail of one range to a new connection.
///
/// The range keeps the bytes it has and stops at the boundary, so the
/// connection that is still on it has nothing left to fetch when its next
/// chunk arrives; the new range starts at that boundary and reaches to the
/// old end. This is the shed of a stalled range and the rebalance of a lagging
/// one, which are the same move made for two reasons.
fn hand_over(
    reporter: &Arc<dyn Reporter>,
    limits: &Limits,
    download: &Arc<Download>,
    set: &mut tokio::task::JoinSet<Result<(), String>>,
    index: usize,
    epoch: u64,
) -> bool {
    let new_id = download.next_segment_id.fetch_add(1, Ordering::SeqCst);
    let new_segment = {
        let mut segments = download.segments.lock().expect("segments lock poisoned");
        let Some(segment) = segments.get_mut(index) else {
            return false;
        };
        let Some(new_segment) =
            segmenter::split(segment, new_id, &download.dir, &download.stem)
        else {
            return false;
        };
        segment.status = SegmentStatus::Stopped;
        segments.push(new_segment.clone());
        new_segment
    };

    // The new range starts empty, even when an earlier run left a file with
    // the same name behind.
    let _ = std::fs::remove_file(&new_segment.part);
    set.spawn(segment_task(
        reporter.clone(),
        limits.clone(),
        download.clone(),
        new_segment.id,
        epoch,
    ));
    emit(reporter, download);
    true
}

/// Give a stalled or lagging range's tail to a new connection.
///
/// A range that stops arriving holds the file back: its bytes are handed to a
/// connection that is moving. A range that merely lags gets the same treatment
/// when a connection slot is free, because the bytes left on it are bytes the
/// other connections could be fetching.
pub(super) fn maybe_redirect(
    reporter: &Arc<dyn Reporter>,
    limits: &Limits,
    download: &Arc<Download>,
    set: &mut tokio::task::JoinSet<Result<(), String>>,
    epoch: u64,
) {
    if download.degraded.load(Ordering::SeqCst) {
        return;
    }
    let segments = download
        .segments
        .lock()
        .expect("segments lock poisoned")
        .clone();
    // Two moving ranges say nothing about a third: one of them is always the
    // slower one.
    let active = segments.iter().filter(|segment| !segment.done()).count();
    if active < 3 {
        return;
    }

    let mut redirected = download
        .redirected
        .lock()
        .expect("redirected lock poisoned");
    if redirected.len() >= REDIRECT_LIMIT {
        return;
    }
    let rates = download
        .pace
        .lock()
        .expect("pace lock poisoned")
        .rates(Instant::now());

    // A free slot is what a rebalance needs; a stalled range is handed over
    // even when every slot is busy, because its connection is holding one
    // without using it.
    let free = (set.len() as u32) < download.max_connections;
    let candidate = crate::pace::stalled(&segments, &rates).or_else(|| {
        if free {
            crate::pace::lagging(&segments, &rates)
        } else {
            None
        }
    });
    let Some(id) = candidate.filter(|id| !redirected.contains(id)) else {
        return;
    };
    let Some(index) = segments.iter().position(|segment| segment.id == id) else {
        return;
    };
    // The range is noted before the hand-over, so a tick that arrives while
    // the new task is starting does not hand the same range over again.
    redirected.push(id);
    drop(redirected);
    drop(rates);

    if hand_over(reporter, limits, download, set, index, epoch) {
        log::write(
            "info",
            &format!("the tail of range {id} was given to another connection"),
        );
    }
}

/// Request one byte range and write it into the segment's part file.
pub(super) async fn fetch_range(
    download: &Arc<Download>,
    limits: &Limits,
    seg_id: u32,
    seg_start: u64,
    from: u64,
    to: Option<u64>,
    epoch: u64,
) -> Result<(), TransferError> {
    // One connection slot of the whole application. The slot is held for the
    // length of the request and given back when the response is done.
    let _permit = limits
        .budget
        .acquire()
        .await
        .ok_or_else(|| TransferError::msg("the connection budget is closed"))?;

    download.set_seg_status(seg_id, SegmentStatus::Connecting);

    // The whole file in one range, or a file whose size is unknown: the
    // server may answer with the whole file instead of the range. A range
    // that starts after the first byte is never the whole file, whatever its
    // end says.
    let total = download.total.load(Ordering::SeqCst);
    let covers_all = from == 0
        && match to {
            None => true,
            Some(to) => total > 0 && to + 1 == total && seg_start == 0,
        };

    // The validator proves that every range is answered from the same version
    // of the file. A server whose copy changed answers the whole file
    // instead, which `open` turns into an error.
    let validator = download.validator();
    let expected_total = (total > 0).then_some(total);
    let opened = download
        .source
        .open(from, to, covers_all, validator.as_deref(), expected_total)
        .await;
    let mut stream = match opened {
        Ok(stream) => stream,
        Err(error) => {
            // The status is the one fact a caller needs to tell "the address
            // was refused" from "the network blinked", and it is gone once
            // the refusal has been rendered into a message.
            download.note_status(error.status);
            return Err(error);
        }
    };
    download.set_seg_status(seg_id, SegmentStatus::Receiving);

    let Some(part) = download.seg_part(seg_id) else {
        // The range was given up while the connection was opening: another
        // connection owns the file now, and this one has nothing to write.
        return Ok(());
    };
    let mut file = OpenOptions::new()
        .create(true)
        // The part file holds the bytes of one range, from the first byte of
        // the range. It is never truncated here: a resumed download writes
        // over the bytes it has and keeps the rest.
        .truncate(false)
        .write(true)
        .open(&part)
        .map_err(|e| TransferError::permanent(e.to_string()))?;

    let base = from - seg_start;
    file.seek(SeekFrom::Start(base))
        .map_err(|e| TransferError::permanent(e.to_string()))?;

    let mut written = 0u64;
    // Set when the response was dropped because the user paused or stopped
    // the download. Such a response is not the end of a file whose size the
    // server never told, so the range must not be closed as if it were.
    let mut interrupted = false;

    while let Some(item) = stream.next().await {
        // A pause or a cancel reaches a range that is streaming: the response
        // is dropped, which closes its connection, and the task waits for the
        // resume or ends. A resume asks for the range again from the byte the
        // part file ended on.
        if download.control.is_paused() || download.is_cancelled(epoch) {
            interrupted = true;
            if download.control.is_paused() {
                download.set_seg_status(seg_id, SegmentStatus::Paused);
            }
            break;
        }

        let bytes = item.map_err(TransferError::msg)?;
        if bytes.is_empty() {
            continue;
        }

        // The segment may have been shortened while this request was running,
        // because a new connection took over the tail of the range. A range
        // that is gone was given up as a whole: the other connection fetches
        // the file.
        let Some((_, seg_end, _)) = download.seg_snapshot(seg_id) else {
            return Ok(());
        };
        let position = from + written;
        if position >= seg_end {
            break;
        }

        let take = ((seg_end - position).min(bytes.len() as u64)) as usize;

        // The bandwidth caps are applied before the bytes hit the disk.
        limits.global_speed.reserve(take as u64).await;
        download.limiter.reserve(take as u64).await;

        file.write_all(&bytes[..take])
            .map_err(|e| TransferError::permanent(e.to_string()))?;
        written += take as u64;

        download.record_progress(seg_id, base + written, SegmentStatus::Receiving);

        // The head of the file is the data of the first range. It is kept
        // while it streams in, so the other ranges can be compared with it
        // as soon as it is there.
        if seg_start == 0 {
            let mut head = download.head.lock().expect("head lock poisoned");
            if head.len() < verify::HEAD_BUFFER {
                let take = (verify::HEAD_BUFFER - head.len()).min(take);
                head.extend_from_slice(&bytes[..take]);
            }
        }

        // The range may have been rolled back while this request was
        // running, because the data was not the data of this range.
        let Some((_, _, current)) = download.seg_snapshot(seg_id) else {
            // The plan was given up (the download fell back to one
            // connection) while this chunk was being written.
            return Ok(());
        };
        if current < base + written {
            return Err(TransferError::msg(ROLLED_BACK_MESSAGE));
        }
    }

    file.flush()
        .map_err(|e| TransferError::permanent(e.to_string()))?;

    // A segment whose length the server never told us ends with the response.
    // An interrupted response is not an end: the rest is asked for again after
    // the resume, from the byte the part file stopped on.
    let Some((_, end, downloaded)) = download.seg_snapshot(seg_id) else {
        return Ok(());
    };
    if end == UNKNOWN_END {
        if !interrupted {
            download.finish_unknown_segment(seg_id);
        }
        return Ok(());
    }

    let expected = end.saturating_sub(from);
    if !interrupted && written < expected {
        // The connection closed before the end of the range, and the rest of
        // the range is still to come. Asking again from the byte the part
        // file reached is what a resumed download does anyway.
        return Err(TransferError::msg(format!(
            "the response ended after {written} of {expected} bytes"
        )));
    }
    if downloaded >= end - seg_start {
        // A known range that has every byte is done even if the pause landed
        // exactly on the last one.
        download.set_seg_status(seg_id, SegmentStatus::Done);
    }

    Ok(())
}

/// Give up on the other connections and fetch the file with one.
///
/// The reference download manager does this when the data of several
/// connections does not match the data that was downloaded earlier. Its own
/// message says that the file was taken over by one connection because of
/// data corruption, when newly received data in several connections did not
/// match the same data downloaded earlier.
pub(super) fn downgrade_to_one_connection(download: &Arc<Download>, total: Option<u64>) -> Option<Segment> {
    let new_id = download.next_segment_id.fetch_add(1, Ordering::SeqCst);
    let part = segmenter::part_path(&download.dir, &download.stem, new_id);

    let mut segments = download.segments.lock().expect("segments lock poisoned");

    let segment = Segment {
        id: new_id,
        start: 0,
        end: total.unwrap_or(UNKNOWN_END),
        downloaded: 0,
        part,
        status: SegmentStatus::Pending,
    };
    // The bytes that were fetched cannot be trusted, so the file is fetched
    // again from the start. Parts another connection is still writing go too;
    // a file that is open on Windows refuses the delete, and the sweep after
    // the download catches that one.
    remove_orphan_parts(
        &download.dir,
        &download.stem,
        std::slice::from_ref(&segment),
    );
    let _ = std::fs::remove_file(&segment.part);
    segments.clear();
    segments.push(segment.clone());

    download.degraded.store(true, Ordering::SeqCst);
    log::write(
        "warn",
        "the data of the ranges did not match, the download goes on with one connection",
    );
    Some(segment)
}

/// Check every range that has data against the head of the file.
fn verify_ranges(download: &Arc<Download>) {
    let segments: Vec<u32> = download
        .segments
        .lock()
        .expect("segments lock poisoned")
        .iter()
        .filter(|s| s.start > 0 && s.downloaded > 0)
        .map(|s| s.id)
        .collect();

    for id in segments {
        let _ = verify_segment(download, id);
    }
}

/// Check the data of a segment against the head of the file.
///
/// A segment that starts with the head of the file holds data that belongs
/// to the first range: the server sent the whole file instead of the range
/// that was asked for. The segment is rolled back and fetched again, the way
/// the reference binary gives the bytes back and starts the chunk over.
fn verify_segment(download: &Arc<Download>, seg_id: u32) -> Result<(), String> {
    let Some((start, _, downloaded)) = download.seg_snapshot(seg_id) else {
        return Ok(());
    };
    if downloaded == 0 {
        return Ok(());
    }
    // The first range is the head of the file.
    if start == 0 {
        return Ok(());
    }

    let head = download.head.lock().expect("head lock poisoned").clone();
    if head.is_empty() {
        return Ok(());
    }

    let Some(part) = download.seg_part(seg_id) else {
        return Ok(());
    };
    let chunk = read_head(&part, 0, verify::HEAD_BYTES);

    match verify::check(&head, &chunk, false) {
        verify::Verdict::Good => Ok(()),
        verify::Verdict::Repeated => {
            // The bytes of this range are not the bytes of this range. Give
            // them back and fetch the range again.
            download.set_seg_downloaded(seg_id, 0);
            log::write(
                "warn",
                "a range held the head of the file, the range was given back",
            );
            Err(CORRUPT_MESSAGE.to_string())
        }
    }
}

/// Download one segment, retrying on transient errors.
pub(super) async fn segment_task(
    reporter: Arc<dyn Reporter>,
    limits: Limits,
    download: Arc<Download>,
    seg_id: u32,
    epoch: u64,
) -> Result<(), String> {
    let mut attempt = 0u32;

    loop {
        if download.is_cancelled(epoch) {
            return Ok(());
        }
        download.control.wait_while_paused(epoch, &download).await;
        if download.is_cancelled(epoch) {
            return Ok(());
        }

        let Some((start, end, downloaded)) = download.seg_snapshot(seg_id) else {
            return Ok(());
        };
        let unknown = end == UNKNOWN_END;
        if !unknown && downloaded >= end - start {
            download.set_seg_status(seg_id, SegmentStatus::Done);
            return Ok(());
        }

        let from = start + downloaded;
        let to = if unknown { None } else { Some(end - 1) };

        match fetch_range(&download, &limits, seg_id, start, from, to, epoch).await {
            Ok(()) => {
                // The first range of the download is the head of the file.
                if start == 0 {
                    verify_ranges(&download);
                }
                // A range that holds the head of the file is not the range
                // that was asked for. The range is given back and the
                // download goes on with a single connection.
                verify_segment(&download, seg_id)?;
                attempt = 0;
                emit_throttled(&reporter, &download, false);
            }
            Err(error) => {
                // A failure that asking again cannot fix is not asked again:
                // a 404 stays a 404, a changed file stays changed, and a disk
                // that cannot be written stays unwritable.
                if error.kind != FailureKind::Transient {
                    download.set_seg_status(seg_id, SegmentStatus::Failed);
                    return Err(format!("segment {seg_id} failed: {}", error.message));
                }
                attempt += 1;
                if attempt >= download.max_attempts.load(Ordering::SeqCst) {
                    download.set_seg_status(seg_id, SegmentStatus::Failed);
                    return Err(format!(
                        "segment {seg_id} failed after {attempt} attempts: {}",
                        error.message
                    ));
                }
                // The server may tell us how long to wait; otherwise the wait
                // grows with every attempt.
                download.set_seg_status(seg_id, SegmentStatus::Retrying);
                emit_throttled(&reporter, &download, true);
                let wait = error.retry_after.unwrap_or_else(|| http::backoff(attempt));
                if !wait_while_active(&download, epoch, wait).await {
                    return Ok(());
                }
            }
        }
    }
}
