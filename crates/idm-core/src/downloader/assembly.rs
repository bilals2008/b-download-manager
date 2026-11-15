//! Joining the parts into the finished file.

use super::*;

/// Join every part file into the destination file and rename it into place.
pub(super) async fn assemble(download: &Arc<Download>) -> Result<(), String> {
    let segments = download
        .segments
        .lock()
        .expect("segments lock poisoned")
        .clone();
    // A range split after a later one was added is appended to the list, so
    // the list is not in file order. The parts have to be joined by where
    // they start, not by when they were planned.
    let mut segments = segments;
    segments.sort_by_key(|segment| segment.start);
    let dest = download.dest.clone();
    let tmp = join_temp_path(&download.dir, &download.stem);

    // A file whose size the server never told us is held in one part file
    // from the first byte to the end, so there is nothing to join.
    if segments.iter().any(|s| s.is_unknown()) {
        let Some(segment) = segments.first() else {
            return Err("the download has no parts".to_string());
        };
        // The part is about to become the destination, so its bytes have to
        // reach the disk before its name does.
        OpenOptions::new()
            .write(true)
            .open(&segment.part)
            .and_then(|file| file.sync_all())
            .map_err(|e| format!("{}: {e}", segment.part.display()))?;
        guard_replace(download)?;
        disk::replace_file(&segment.part, &dest).map_err(|e| e.to_string())?;
        cleanup(download);
        return Ok(());
    }

    let join_target = tmp.clone();
    let join_segments = segments.clone();
    let total = download.total.load(Ordering::SeqCst);
    let joined = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut out = File::create(&join_target).map_err(|e| e.to_string())?;

        for segment in &join_segments {
            let mut file = File::open(&segment.part)
                .map_err(|e| format!("{}: {e}", segment.part.display()))?;
            let mut remaining = segment.len();
            let mut buffer = vec![0u8; COPY_BUF];

            while remaining > 0 {
                let want = remaining.min(COPY_BUF as u64) as usize;
                let read = file.read(&mut buffer[..want]).map_err(|e| e.to_string())?;
                if read == 0 {
                    return Err(format!(
                        "{} is shorter than the range it should hold",
                        segment.part.display()
                    ));
                }
                out.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
                remaining -= read as u64;
            }
        }

        out.flush().map_err(|e| e.to_string())?;
        // The rename makes the joined file the destination. The bytes behind
        // that name have to be on the disk before it is, or a crash leaves a
        // destination that is shorter than the file it claims to be.
        out.sync_all().map_err(|e| e.to_string())?;

        // The file that is about to replace the destination has to be the
        // size the server promised, or it is not the file that was asked for.
        if total > 0 {
            let length = out.metadata().map_err(|e| e.to_string())?.len();
            if length != total {
                return Err(format!(
                    "the assembled file is {length} bytes, the server promised {total}"
                ));
            }
        }

        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?;

    if let Err(error) = joined {
        // The destination is untouched: the half written file only ever
        // lived in the temporary file.
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }

    if let Err(error) = guard_replace(download) {
        // The half written file never became the destination.
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    if let Err(error) = disk::replace_file(&tmp, &dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error.to_string());
    }

    cleanup(download);
    Ok(())
}

/// Refuse to put the finished file over one that was not there when the
/// download started.
///
/// A Replace was confirmed before the transfer began, so it is the one case
/// where the destination may already hold a file. Everything else leaves a
/// file that appeared meanwhile alone, and reports why.
fn guard_replace(download: &Arc<Download>) -> Result<(), String> {
    if download.dest.exists() && !download.allow_replace.load(Ordering::SeqCst) {
        return Err(format!(
            "{} appeared while the download was running; it was not replaced",
            download.dest.display()
        ));
    }
    Ok(())
}
