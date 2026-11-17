//! Driving one run: probe, plan, fetch every range, assemble.

use super::assembly::assemble;
use super::probe::{measure_location, probe_with_retries};
use super::ranges::{downgrade_to_one_connection, maybe_grow, maybe_redirect, segment_task};
use super::resume::restore;
use super::*;

/// Check that the volume the bytes land on has room for the download.
///
/// The parts go to the unfinished folder. When the finished file is moved to
/// another volume, that volume needs the room too.
fn check_free_space(download: &Arc<Download>, total: Option<u64>) -> Result<(), String> {
    let final_folder = download
        .dest
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    check_volume(&download.dir, total)?;
    if !disk::same_volume(&download.dir, final_folder) {
        check_volume(final_folder, total)?;
    }
    Ok(())
}

fn check_volume(folder: &Path, total: Option<u64>) -> Result<(), String> {
    let Some(free) = disk::free_space(folder) else {
        return Ok(());
    };
    if free < DISK_MARGIN {
        return Err("the disk is full".to_string());
    }
    if let Some(total) = total {
        if free < total + DISK_MARGIN {
            return Err(format!(
                "not enough space on the disk: {free} bytes free, {} needed",
                total + DISK_MARGIN
            ));
        }
    }
    Ok(())
}

pub(super) async fn drive(
    reporter: &Arc<dyn Reporter>,
    limits: &Limits,
    download: &Arc<Download>,
    epoch: u64,
) -> Result<(), String> {
    // A pause that arrived between the command that started this run and the
    // run itself holds the connections back; the status says so from the
    // first event instead of claiming traffic that is not moving.
    download.set_status(if download.control.is_paused() {
        Status::Paused
    } else {
        Status::Running
    });
    emit(reporter, download);
    log::write(
        "info",
        &format!(
            "starting {} -> {} with {} connections",
            download.url,
            download.dest.display(),
            download.max_connections
        ),
    );

    let probe = probe_with_retries(download, epoch).await?;

    // The validator of the probe travels with every range as `If-Range`, so
    // a file that changes between the probe and a range is refused instead of
    // being mixed into the bytes already on disk.
    download.set_validator(probe.validator());

    // A file can be resumed when the server honours ranges and either knows
    // how big the file is or gives a validator that proves the bytes on disk
    // are still the first bytes of the file it serves now.
    download.set_resumable(probe.ranges && (probe.total.is_some() || probe.validator().is_some()));

    // A server that answers a range request with 200 cannot be segmented.
    let host = crate::limits::host_of(&download.url);
    let site_max = host
        .as_deref()
        .map(|host| limits.sites.resolve(host, download.max_connections))
        .unwrap_or(download.max_connections);
    // What this host allowed the last time is the starting point, so a host
    // that refused eight of ten ranges is not asked for ten again.
    let allowed = match host.as_deref() {
        Some(host) => {
            download.set_learned_connections(limits.learned.get(host));
            limits
                .learned
                .resolve(host, site_max.min(download.max_connections))
        }
        None => site_max.min(download.max_connections),
    };
    // The number of connections a file deserves: one per 10 KB that is left,
    // the way the reference binary counts them.
    let max_connections = if probe.ranges {
        match probe.total {
            Some(total) => segmenter::connections_for(total, allowed),
            None => 1,
        }
    } else {
        1
    };
    // A server that does not honour ranges must not be split: a range that
    // starts past the first byte is answered with the whole file, and a new
    // connection opened on the fly would poison the bytes on disk. Marking the
    // download as degraded keeps `maybe_grow` from splitting it. This is the
    // GitHub zip case: the probe is answered with `200` and no `Content-Range`,
    // and splitting the plan turned a working download into "the file changed".
    if !probe.ranges {
        download.degraded.store(true, Ordering::SeqCst);
        // A host that answered a range request with the whole file allows one
        // connection; that is worth remembering.
        if let Some(host) = host.as_deref() {
            limits.learned.observe(host, download.max_connections, 1);
        }
    }

    check_free_space(download, probe.total)?;

    download
        .total
        .store(probe.total.unwrap_or(0), Ordering::SeqCst);

    let segments = restore(download, &probe, max_connections);
    save_manifest(download);
    emit(reporter, download);
    log::write(
        "info",
        &format!(
            "the file is {} bytes in {} ranges, the server honours ranges: {}",
            probe
                .total
                .map(|total| total.to_string())
                .unwrap_or_else(|| "of an unknown size".to_string()),
            segments.len(),
            probe.ranges
        ),
    );

    // A new run starts from a clean slate: the ranges it hands over and the
    // rates it measures belong to this run, not to the last one.
    download.pace.lock().expect("pace lock poisoned").clear();
    download
        .redirected
        .lock()
        .expect("redirected lock poisoned")
        .clear();
    measure_location(download, host.clone());

    let mut set = tokio::task::JoinSet::new();
    for segment in &segments {
        set.spawn(segment_task(
            reporter.clone(),
            limits.clone(),
            download.clone(),
            segment.id,
            epoch,
        ));
    }

    let mut failure: Option<String> = None;
    // Set when the plan on disk was thrown away because the server serves a
    // different file now. Only one restart is worth it: a server that changes
    // on every request is not downloadable in pieces.
    let mut restarted = false;
    let mut last_space_check = Instant::now();
    let mut last_manifest_save = Instant::now();
    loop {
        tokio::select! {
            joined = set.join_next() => {
                match joined {
                    Some(Ok(Ok(()))) => {}
                    Some(Ok(Err(error))) => {
                        if error.contains(CORRUPT_MESSAGE) || error.contains(RANGE_IGNORED_MESSAGE) {
                            // The server sent bytes that are not the range
                            // that was asked for. The download goes on with a
                            // single connection, which asks for the whole
                            // file and cannot be given the wrong part of it.
                            let already_degraded =
                                download.degraded.load(Ordering::SeqCst);
                            if !already_degraded {
                                if let Some(segment) =
                                    downgrade_to_one_connection(download, probe.total)
                                {
                                    set.spawn(segment_task(
                                        reporter.clone(),
                                        limits.clone(),
                                        download.clone(),
                                        segment.id,
                                        epoch,
                                    ));
                                    continue;
                                }
                            }
                        }
                        // A resume that asked for a range the server no longer
                        // serves from the copy on disk: the server answered the
                        // whole file instead. Nothing on disk can be kept, so
                        // the file is fetched again from the first byte, the
                        // way the reference manager restarts a changed file
                        // instead of failing it.
                        if error.contains(CHANGED_PREFIX) && !restarted {
                            restarted = true;
                            if let Ok(fresh) = download.source.probe().await {
                                download
                                    .total
                                    .store(fresh.total.unwrap_or(0), Ordering::SeqCst);
                                download.set_validator(fresh.validator());
                                if let Some(segment) =
                                    downgrade_to_one_connection(download, fresh.total)
                                {
                                    log::write(
                                        "warn",
                                        "the file changed on the server, the download starts over",
                                    );
                                    set.spawn(segment_task(
                                        reporter.clone(),
                                        limits.clone(),
                                        download.clone(),
                                        segment.id,
                                        epoch,
                                    ));
                                    continue;
                                }
                            }
                        }
                        // A task of the plan that was given up reports the
                        // rollback of its range, not a failure of the
                        // download.
                        if error.contains(ROLLED_BACK_MESSAGE) {
                            continue;
                        }
                        failure = Some(error);
                        download.stop();
                    }
                    Some(Err(error)) => {
                        failure = Some(error.to_string());
                        download.stop();
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(300)) => {
                // A paused download writes nothing, so it has no new resume
                // state to record, no new connection to open, and no new
                // reason to look at the free space of the disk.
                if !download.control.is_paused() {
                    maybe_grow(reporter, limits, download, &mut set, epoch);
                    maybe_redirect(reporter, limits, download, &mut set, epoch);
                    // The manifest is the resume state. Writing it on every
                    // tick would rewrite the same JSON several times a second
                    // for the length of the download; a couple of seconds of
                    // lost progress after a crash is the cheaper trade.
                    if last_manifest_save.elapsed() >= MANIFEST_INTERVAL {
                        last_manifest_save = Instant::now();
                        save_manifest(download);
                    }
                    if last_space_check.elapsed() >= SPACE_CHECK_INTERVAL {
                        last_space_check = Instant::now();
                        if let Err(error) = check_free_space(download, probe.total) {
                            failure = Some(error);
                            download.stop();
                        }
                    }
                }
                // The bytes that were in flight when the pause arrived still
                // land, so the count keeps being reported while paused.
                emit_throttled(reporter, download, false);
            }
        }

        if failure.is_some() || set.is_empty() {
            break;
        }
    }

    if let Some(error) = failure {
        return Err(error);
    }
    if download.is_cancelled(epoch) {
        return Err("cancelled".to_string());
    }

    assemble(download).await?;
    // Every range of the plan finished: the host honoured what it was asked
    // for, so the next download from it starts at that number.
    if let Some(host) = host.as_deref() {
        if !download.degraded.load(Ordering::SeqCst) {
            let planned = segments.len() as u32;
            limits.learned.observe(host, planned, planned);
        }
    }
    Ok(())
}
