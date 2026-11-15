//! Moving a download's file, or its unfinished parts, to another name.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter};

use idm_core::downloader::{Download, Status};
use idm_core::segmenter::part_path;

use super::{announce, refused, resume, UPDATE_EVENT};
use crate::conflict::{self, Choice, Refusal};
use crate::state::AppState;

/// Point a download at another file, taking what it already wrote along.
///
/// A finished file is moved itself; an unfinished row takes its part files and
/// manifest, so a resume continues from the bytes already received. A row with
/// a task cannot move, because the transfer would keep writing to the old
/// file. A taken name is a question, never a silent replacement.
pub async fn set_destination(
    app: &AppHandle,
    state: &AppState,
    id: u64,
    dest: PathBuf,
    choice: Choice,
) -> Result<String, Refusal> {
    let target = rewrite_destination(state, id, &dest, choice).await?;
    if let Some(download) = state.get(id) {
        announce(app, &download);
    }
    Ok(target.to_string_lossy().to_string())
}

pub(super) async fn rewrite_destination(
    state: &AppState,
    id: u64,
    dest: &std::path::Path,
    choice: Choice,
) -> Result<PathBuf, Refusal> {
    let old = state
        .get(id)
        .ok_or_else(|| refused("invalid", "unknown download".to_string(), dest))?;
    if old.is_running() || matches!(old.status(), Status::Running | Status::Queued) {
        return Err(refused(
            "busy",
            "stop the download before changing where it is saved".to_string(),
            &old.dest,
        ));
    }
    // The same file is not a move: asking about it would report the row's own
    // file as a conflict, and renaming it would move the parts for no reason.
    if conflict::same_path(&old.dest, dest) {
        return Ok(old.dest.clone());
    }

    let completed = old.status() == Status::Completed;

    // A finished file is the only copy there is, so a taken name is asked
    // about. An unfinished row only has parts, and putting those over a file
    // in the target folder would not save anything; a free name is chosen.
    let answer = if completed { choice } else { Choice::Rename };

    // The gate is held for the name that is chosen, not for the bytes that
    // move: a copy across volumes takes a while, and the gate is a lock the
    // whole application waits for.
    let (target, replace) = {
        let _gate = state.start_gate();
        match conflict::resolve(state, dest, answer, Some(id))? {
            conflict::Resolved::Start { dest, replace } => (dest, replace),
            conflict::Resolved::Skip => return Ok(old.dest.clone()),
        }
    };

    if completed && !old.dest.is_file() {
        return Err(refused(
            "missing",
            format!(
                "{} is not there any more, so it cannot be moved",
                old.dest.display()
            ),
            &old.dest,
        ));
    }

    // Build the row while the old file is still there, so a failure here
    // leaves both the file and the row untouched.
    let moved = moved_download(state, &old, target.clone(), replace)
        .await
        .map_err(|message| refused("invalid", message, &target))?;
    if completed && replace && target.exists() {
        // The user confirmed replacing the file at the new path. A part file
        // holds bytes, not the finished file, which is why this only applies
        // to a completed download.
        std::fs::remove_file(&target).map_err(|error| {
            refused(
                "invalid",
                format!("the existing file could not be replaced: {error}"),
                &target,
            )
        })?;
    }
    if completed {
        move_file(&old.dest, &target).await.map_err(|error| {
            refused(
                "invalid",
                format!("the file could not be moved: {error}"),
                &target,
            )
        })?;
    }

    state.replace(moved);
    state.save_index();
    Ok(target)
}

/// Build the row that writes `dest` in place of `old`.
///
/// Everything that describes the download travels with it: the metadata, the
/// speed cap, the byte counters and the ranges. The part files are moved to
/// the new name, and a failure puts them back before the old row is touched.
async fn moved_download(
    state: &AppState,
    old: &Download,
    dest: PathBuf,
    replace: bool,
) -> Result<Arc<Download>, String> {
    let previous = old.info();
    // The row keeps its id, and its id is the name of its staging folder: a
    // move changes the file, not where the application keeps the bytes. A row
    // of the older layout is taken into the tree as it moves.
    let work_dir = state.work_dir(old.id, &dest);
    let moved = Download::new_in(
        old.id,
        old.url.clone(),
        dest,
        work_dir.as_deref(),
        old.max_connections,
        state.limits().proxy().as_deref(),
        old.referer.as_deref(),
    )?;
    moved.set_meta(previous.category, previous.description);
    moved.set_speed_limit(previous.speed_limit);
    moved.set_allow_replace(replace);

    let old_segments = old.segments.lock().expect("segments lock poisoned").clone();
    let mut new_segments = Vec::with_capacity(old_segments.len());
    let mut moved_files: Vec<(PathBuf, PathBuf)> = Vec::new();
    for mut segment in old_segments {
        let target = part_path(&moved.dir, &moved.stem, segment.id);
        if segment.part.exists() && !conflict::same_path(&segment.part, &target) {
            // A file already at the target name belongs to the row that is
            // moving; its own part file is not a conflict with itself.
            if let Err(error) = move_file(&segment.part, &target).await {
                for (from, to) in moved_files.iter().rev() {
                    let _ = move_file(to, from).await;
                }
                return Err(format!("cannot move the partial file: {error}"));
            }
            moved_files.push((segment.part.clone(), target.clone()));
        }
        segment.part = target;
        new_segments.push(segment);
    }

    let order = std::sync::atomic::Ordering::SeqCst;
    moved.total.store(old.total.load(order), order);
    moved
        .next_segment_id
        .store(old.next_segment_id.load(order), order);
    *moved.segments.lock().expect("segments lock poisoned") = new_segments;
    // A move is neither a resume nor a failure: the row keeps the state it
    // had, including the message of an attempt that could not finish.
    moved.set_status(previous.status);
    *moved.error.lock().expect("error lock poisoned") = previous.error;
    if let Some(resumable) = previous.resumable {
        moved.set_resumable(resumable);
    }

    // A download that failed before it planned its ranges has nothing to
    // resume, and an empty manifest would only sit beside the file.
    if !moved
        .segments
        .lock()
        .expect("segments lock poisoned")
        .is_empty()
    {
        moved.save_manifest();
    }
    let _ = std::fs::remove_file(&old.manifest);
    Ok(moved)
}

/// Move a download that has no task to another folder.
///
/// The part files and the manifest travel with it, so a server that supports
/// ranges continues from the bytes already received. The new name is chosen
/// to be free: a move never replaces a file in the target folder.
pub async fn relocate(
    app: &AppHandle,
    state: &AppState,
    id: u64,
    folder: PathBuf,
) -> Result<(), String> {
    let old = state.get(id).ok_or("unknown download")?;
    if old.is_running() || matches!(old.status(), Status::Running | Status::Queued) {
        return Err("stop the download before moving it".to_string());
    }
    if old.status() == Status::Completed {
        return Err("a finished download cannot be moved".to_string());
    }
    if !folder.is_dir() {
        return Err(format!("{} is not a folder", folder.display()));
    }
    let name = old
        .dest
        .file_name()
        .ok_or("the download has no file name")?;
    // The same folder is not a move: the question was about the bytes, so
    // continue where the download stopped instead of renaming the file.
    if old
        .dest
        .parent()
        .is_some_and(|parent| conflict::same_path(parent, &folder))
    {
        return resume(app, state, id, false);
    }
    let wanted = folder.join(name);

    // The gate is held for the name that is chosen, not for the bytes that
    // move: a copy across volumes takes a while, and the gate is a lock the
    // whole application waits for.
    let new_dest = {
        let _gate = state.start_gate();
        match conflict::resolve(state, &wanted, Choice::Rename, Some(id))
            .map_err(|refusal| refusal.message)?
        {
            conflict::Resolved::Start { dest, .. } => dest,
            conflict::Resolved::Skip => return Err("the download was not moved".to_string()),
        }
    };

    let moved = moved_download(state, &old, new_dest, false).await?;
    state.replace(moved);
    state.save_index();
    let _ = app.emit(
        UPDATE_EVENT,
        state.get(id).expect("the moved download").info(),
    );
    resume(app, state, id, false)
}

/// Put `from` at `to`, across volumes as well.
///
/// A rename cannot cross a volume, so the bytes are copied when it fails.
/// The copy of a large file takes a while, so it runs on the blocking pool
/// of the runtime: the window must stay alive while the bytes move.
async fn move_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    match tokio::fs::rename(from, to).await {
        Ok(()) => Ok(()),
        Err(_) => {
            tokio::fs::copy(from, to).await?;
            tokio::fs::remove_file(from).await
        }
    }
}
