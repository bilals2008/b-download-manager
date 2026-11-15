//! The resume state: the manifest on disk and the plan it describes.

use super::*;

/// Whether the manifest on disk describes the file the server just showed.
fn can_resume(manifest: &Manifest, probe: &Probe) -> bool {
    // The probe only proves the server honours ranges if it answered `206`.
    if !probe.ranges {
        return false;
    }
    let current = probe.validator();
    match probe.total {
        Some(total) => {
            if manifest.total != total {
                return false;
            }
            // When both sides name a version, they have to agree. A server
            // that stopped sending a validator leaves the size as the only
            // proof, and an older manifest has no validator at all.
            match (&manifest.validator, current) {
                (Some(old), Some(new)) => *old == new,
                _ => true,
            }
        }
        // Without a size, only a matching validator proves that the bytes on
        // disk are the first bytes of the file the server still serves.
        None => {
            manifest.total == 0
                && manifest.segments.len() == 1
                && manifest.segments[0].is_unknown()
                && matches!((&manifest.validator, current), (Some(old), Some(new)) if *old == new)
        }
    }
}

/// Rebuild the segment list, resuming from the manifest when possible.
pub(super) fn restore(download: &Arc<Download>, probe: &Probe, max_connections: u32) -> Vec<Segment> {
    // The head is rebuilt from the range that starts at the first byte: from
    // the part on disk when the download resumes, from the stream when it
    // starts over. An old run's head belongs to none of them.
    download.head.lock().expect("head lock poisoned").clear();

    if let Some(manifest) = read_manifest(download) {
        if can_resume(&manifest, probe) {
            let mut segments = manifest.segments;
            // A crash can leave extra bytes behind. Cut the part file back
            // to the recorded length, the way the reference binary truncates
            // the file when it resumes ("Trucate at %I64d").
            for segment in &mut segments {
                // The part file of this range is gone. The range starts over,
                // the other ranges keep the bytes they have: one lost part
                // costs one range, not the whole file.
                if segment.downloaded > 0 && !segment.part.exists() {
                    segment.downloaded = 0;
                }
                // The part file can be longer than the manifest says (the
                // bytes reached the disk, the manifest did not) or shorter
                // (the manifest was written, the bytes were not flushed).
                // Only a longer file is cut back: `set_len` would pad a
                // shorter one with zeros.
                if let Ok(file) = OpenOptions::new().write(true).open(&segment.part) {
                    if let Ok(length) = file.metadata().map(|meta| meta.len()) {
                        if length > segment.downloaded {
                            let _ = file.set_len(segment.downloaded);
                        } else if length < segment.downloaded {
                            segment.downloaded = length;
                        }
                    }
                }
                // The connections of a manifest are not running now.
                segment.status = if segment.done() {
                    SegmentStatus::Done
                } else {
                    SegmentStatus::Pending
                };
            }
            // The other ranges are checked against the head of the file. A
            // resumed download already has the head in its first part.
            if let Some(first) = segments.iter().find(|segment| segment.start == 0) {
                let len = first.downloaded.min(verify::HEAD_BUFFER as u64) as usize;
                let head = read_head(&first.part, 0, len);
                *download.head.lock().expect("head lock poisoned") = head;
            }
            let next_id = segments.iter().map(|s| s.id).max().unwrap_or(0) + 1;
            download.next_segment_id.store(next_id, Ordering::SeqCst);
            *download.segments.lock().expect("segments lock poisoned") = segments.clone();
            return segments;
        }

        // The plan on disk belongs to another version of the file, or the
        // server no longer honours ranges. Its bytes are not used again, and
        // every part goes, not only the ones whose ids the new plan reuses.
        remove_orphan_parts(&download.dir, &download.stem, &[]);
        let _ = std::fs::remove_file(&download.manifest);
        log::write(
            "info",
            "the resume state did not match the server, the file starts over",
        );
    }

    let segments = match probe.total {
        Some(total) => segmenter::plan(total, max_connections, &download.dir, &download.stem),
        // The server did not tell us the size, so the file is fetched by a
        // single connection, from the first byte to the end of the response.
        None => segmenter::plan_unknown(&download.dir, &download.stem),
    };
    // A fresh plan starts from scratch: every part file of an earlier run is
    // stale, including the ones whose ids the new plan does not reuse.
    remove_orphan_parts(&download.dir, &download.stem, &[]);
    download
        .next_segment_id
        .store(segments.len() as u32, Ordering::SeqCst);
    *download.segments.lock().expect("segments lock poisoned") = segments.clone();
    segments
}

/// Read the manifest of a download, when it is there and still matches.
pub(super) fn read_manifest(download: &Arc<Download>) -> Option<Manifest> {
    let text = std::fs::read_to_string(&download.manifest).ok()?;
    let manifest: Manifest = serde_json::from_str(&text).ok()?;
    if manifest.url != crate::transfer::redacted(&download.url)
        || manifest.segments.is_empty()
        || !manifest.is_consistent()
    {
        return None;
    }
    Some(manifest)
}

pub fn save_manifest(download: &Download) {
    let mut segments = download
        .segments
        .lock()
        .expect("segments lock poisoned")
        .clone();
    // The list is not in file order: a split appends the tail of a range to
    // the end. The manifest on disk is kept in file order, the order the
    // ranges cover the file in.
    segments.sort_by_key(|segment| segment.start);
    let manifest = Manifest {
        // The manifest lies next to the file, which may be in a folder that
        // is shared, so the password of the URL is not in it.
        url: crate::transfer::redacted(&download.url),
        total: download.total.load(Ordering::SeqCst),
        segments,
        validator: download.validator(),
    };
    let Ok(text) = serde_json::to_string_pretty(&manifest) else {
        return;
    };

    // The manifest is written to a temporary file and renamed into place: a
    // crash in the middle of the write must not destroy the resume state.
    let temp = manifest_temp_path(&download.manifest);
    if std::fs::write(&temp, text).is_ok() {
        let _ = std::fs::rename(&temp, &download.manifest);
    }
}

/// What a relocation of a download's resume state produced.
#[derive(Debug, Clone)]
pub struct Relocated {
    /// The manifest that now describes the state, in its new folder.
    pub manifest: PathBuf,
    /// The files of the old folder that were copied. They may be removed
    /// once the index points at the new folder.
    pub old_files: Vec<PathBuf>,
}

/// Copy a download's whole resume state into `to_dir`, rewriting the part
/// paths the manifest records.
///
/// This is how an unfinished download survives a change of the application's
/// storage layout. The bytes are **copied**, not moved, and every copy is
/// measured in its new folder before it counts: a part that recorded bytes
/// must hold at least as many there. The old files stay where they are until
/// the caller has pointed its index at the new folder, so a crash anywhere in
/// between leaves one complete copy of the state and nothing is ever deleted
/// before its copy is verified. Files the old folder holds under this
/// download's own names — the ranges of an earlier plan, a join temporary, a
/// manifest temporary — travel with it rather than being dropped.
pub fn relocate_resume_state(manifest: &Path, to_dir: &Path) -> Result<Relocated, String> {
    let from_dir = manifest
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if from_dir == to_dir {
        return Ok(Relocated {
            manifest: manifest.to_path_buf(),
            old_files: Vec::new(),
        });
    }

    let text = std::fs::read_to_string(manifest)
        .map_err(|error| format!("{}: {error}", manifest.display()))?;
    let mut plan: Manifest =
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", manifest.display()))?;

    std::fs::create_dir_all(to_dir).map_err(|error| format!("{}: {error}", to_dir.display()))?;

    let mut old_files = vec![manifest.to_path_buf()];

    // A range that recorded bytes must still have them; a range that recorded
    // none has no part yet and needs no copy.
    for segment in &mut plan.segments {
        if segment.part.parent() == Some(to_dir) {
            continue;
        }
        if !segment.part.exists() {
            if segment.downloaded > 0 {
                return Err(format!(
                    "the part {} of {} is missing",
                    segment.part.display(),
                    manifest.display()
                ));
            }
            continue;
        }
        let name = segment
            .part
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("part{}", segment.id));
        let target = to_dir.join(name);
        std::fs::copy(&segment.part, &target)
            .map_err(|error| format!("cannot copy {}: {error}", segment.part.display()))?;
        let stored = target
            .metadata()
            .map_err(|error| format!("cannot check {}: {error}", target.display()))?
            .len();
        if stored < segment.downloaded {
            return Err(format!(
                "{} holds {stored} bytes, the manifest recorded {}",
                target.display(),
                segment.downloaded
            ));
        }
        old_files.push(segment.part.clone());
        segment.part = target;
    }

    // Anything else the engine wrote for this file name travels too: the
    // bytes of a range an older plan dropped, a join that died, a manifest
    // write that never finished. They are not trusted, but they are not
    // deleted on the way out of the user's folder either.
    let stem = manifest
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .and_then(|name| name.strip_suffix(".idm.json").map(str::to_string))
        .unwrap_or_default();
    if !stem.is_empty() {
        if let Ok(entries) = std::fs::read_dir(&from_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() || path == manifest {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                let ours = name == format!("{stem}.idmtmp")
                    || name == format!("{stem}.idm.json.tmp")
                    || name
                        .strip_prefix(&format!("{stem}.part"))
                        .is_some_and(|id| {
                            !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit())
                        });
                if ours {
                    let target = to_dir.join(&name);
                    if std::fs::copy(&path, &target).is_ok() {
                        old_files.push(path);
                    }
                }
            }
        }
    }

    // The rewritten manifest lands atomically and is read back: the entry the
    // index will point at must parse.
    let landing = manifest
        .file_name()
        .map(|name| to_dir.join(name))
        .ok_or("the manifest has no file name")?;
    let patched = serde_json::to_string_pretty(&plan)
        .map_err(|error| format!("the manifest could not be written: {error}"))?;
    let temp = manifest_temp_path(&landing);
    std::fs::write(&temp, patched).map_err(|error| format!("{}: {error}", temp.display()))?;
    std::fs::rename(&temp, &landing).map_err(|error| format!("{}: {error}", landing.display()))?;
    let reread = std::fs::read_to_string(&landing)
        .map_err(|error| format!("{}: {error}", landing.display()))?;
    serde_json::from_str::<Manifest>(&reread)
        .map_err(|_| format!("{} does not read back", landing.display()))?;

    Ok(Relocated {
        manifest: landing,
        old_files,
    })
}

/// Remove the files a relocation copied, once the index points at the new
/// folder. A file that is already gone is not an error: the state it
/// described lives in the new folder either way.
pub fn discard_relocated(state: &Relocated) {
    for file in &state.old_files {
        let _ = std::fs::remove_file(file);
    }
}
