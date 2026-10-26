//! The download actions the window and the browser bridge share.
//!
//! Every way of starting, holding, resuming or stopping a download goes
//! through this module: a toolbar button, the add dialog, and a request from
//! the Firefox extension all end up in the same place, so the two front ends
//! cannot drift apart in what a download may do.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use idm_core::downloader::{Download, DownloadInfo, Reporter, Status, DEFAULT_CONNECTIONS};
use idm_core::limits::Limits;
use idm_core::segmenter::part_path;

use crate::conflict::{self, Choice, Refusal, Started};
use crate::state::AppState;

/// Event carrying the state of one download to the UI.
pub const UPDATE_EVENT: &str = "download://update";

/// What a new download carries besides its address and destination.
#[derive(Default)]
pub struct StartOptions {
    /// The number of ranges, or the engine default.
    pub connections: Option<u32>,
    /// The page the link was on, when the browser knew it.
    pub referer: Option<String>,
    /// Start the rows in the queue instead of starting the transfer now.
    pub queue: bool,
    /// The category the add dialog showed.
    pub category: Option<String>,
    /// The note the user typed in the add dialog.
    pub description: Option<String>,
    /// What the File Exists dialog answered.
    pub conflict: Choice,
}

/// The reporter the desktop app hands to the engine: it pushes the state of
/// a download into the user interface.
struct AppReporter(AppHandle);

impl Reporter for AppReporter {
    fn report(&self, info: &DownloadInfo) {
        let _ = self.0.emit(UPDATE_EVENT, info.clone());

        // A finished file gets the window IDM opens for it, unless the user
        // turned that window off in Options. The window is built on the main
        // thread, the one the window toolkit runs on.
        if info.status == Status::Completed && self.0.state::<AppState>().show_complete_dialog() {
            let app = self.0.clone();
            let info = info.clone();
            let _ = self
                .0
                .run_on_main_thread(move || crate::complete::show(&app, &info));
        }
    }
}

/// Start a download and return its id.
///
/// The run is claimed before its task is spawned, so a stop that arrives while
/// the task still waits to be polled covers it too.
fn refused(kind: &str, message: String, dest: &std::path::Path) -> Refusal {
    Refusal {
        kind: kind.to_string(),
        message,
        path: dest.to_string_lossy().to_string(),
        suggested: dest.to_string_lossy().to_string(),
    }
}

/// Hand a download to a task of its own.
///
/// The task claims the run itself, so a stop that lands right after the spawn
/// still covers it.
fn spawn_run(app: &AppHandle, state: &AppState, download: Arc<Download>) {
    let limits: Limits = state.limits().clone();
    let request = download.request_run();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        idm_core::downloader::run_claimed(Arc::new(AppReporter(app)), limits, download, request)
            .await;
    });
}

pub fn start(
    app: &AppHandle,
    state: &AppState,
    url: String,
    dest: PathBuf,
    options: StartOptions,
) -> Result<Started, Refusal> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err(refused("invalid", "a URL is required".to_string(), &dest));
    }
    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ftp://")) {
        return Err(refused(
            "invalid",
            "the URL must start with http://, https:// or ftp://.".to_string(),
            &dest,
        ));
    }
    if dest.as_os_str().is_empty() {
        return Err(refused(
            "invalid",
            "a destination file is required".to_string(),
            &dest,
        ));
    }

    // The check and the insert are one step: two starts that both found the
    // name free would otherwise write over each other's part files.
    let _gate = state.start_gate();
    let (dest, replace) = match conflict::resolve(state, &dest, options.conflict, None)? {
        conflict::Resolved::Skip => {
            return Ok(Started {
                id: None,
                dest: dest.to_string_lossy().to_string(),
                skipped: true,
            })
        }
        conflict::Resolved::Start { dest, replace } => (dest, replace),
    };

    let download = Download::new(
        state.next_id(),
        url,
        dest.clone(),
        options
            .connections
            .unwrap_or(DEFAULT_CONNECTIONS)
            .clamp(1, 32),
        state.limits().proxy().as_deref(),
        options.referer.as_deref(),
    )
    .map_err(|message| refused("invalid", message, &dest))?;
    download.set_meta(options.category, options.description);
    download.set_allow_replace(replace);

    let id = download.id;
    state.insert(download.clone());
    state.save_index();

    // A download that was added to the queue waits for Start Queue. The row
    // is there at once, in the state the queue expects, and no task is
    // spawned: the queue starts it the same way a paused one is started.
    if options.queue {
        let _ = app.emit(UPDATE_EVENT, download.info());
        return Ok(Started {
            id: Some(id),
            dest: dest.to_string_lossy().to_string(),
            skipped: false,
        });
    }

    spawn_run(app, state, download);

    Ok(Started {
        id: Some(id),
        dest: dest.to_string_lossy().to_string(),
        skipped: false,
    })
}

/// Start a failed download again.
///
/// `restart` forgets the bytes of the attempt that could not continue, which
/// is the safe answer when the server does not support ranges. Without it the
/// engine picks up from the parts and the manifest it still has.
pub fn retry(app: &AppHandle, state: &AppState, id: u64, restart: bool) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    match download.status() {
        Status::Failed | Status::Cancelled | Status::Paused => {}
        Status::Running | Status::Queued => return Err("the download is still running".to_string()),
        Status::Completed => return Err("this download has already finished".to_string()),
    }
    if restart {
        download.discard_progress()?;
        download.set_status(Status::Queued);
    }
    resume(app, state, id)
}

/// Start a download over, from its first byte.
///
/// The address, the category and the note stay; the bytes of the old attempt
/// do not. The row is replaced only once the File Exists question is settled,
/// so a cancelled question leaves it exactly as it was, and its own file is
/// not a conflict with the download that replaces it.
pub fn redownload(
    app: &AppHandle,
    state: &AppState,
    id: u64,
    choice: Choice,
) -> Result<Started, Refusal> {
    let old = state.get(id).ok_or_else(|| {
        refused(
            "invalid",
            "unknown download".to_string(),
            std::path::Path::new(""),
        )
    })?;
    if old.is_running() || matches!(old.status(), Status::Running | Status::Queued) {
        // Not a destination question: the row itself is busy, which no answer
        // in the File Exists window can settle.
        return Err(refused(
            "busy",
            "stop the download before starting it again".to_string(),
            &old.dest,
        ));
    }

    let _gate = state.start_gate();
    let (dest, replace) = match conflict::resolve(state, &old.dest, choice, Some(id))? {
        conflict::Resolved::Skip => {
            return Ok(Started {
                id: None,
                dest: old.dest.to_string_lossy().to_string(),
                skipped: true,
            })
        }
        conflict::Resolved::Start { dest, replace } => (dest, replace),
    };

    let previous = old.info();
    let fresh = Download::new(
        state.next_id(),
        previous.url.clone(),
        dest.clone(),
        old.max_connections,
        state.limits().proxy().as_deref(),
        old.referer.as_deref(),
    )
    .map_err(|message| refused("invalid", message, &dest))?;
    fresh.set_meta(previous.category, previous.description);
    fresh.set_speed_limit(previous.speed_limit);
    fresh.set_allow_replace(replace);

    // Part files carry the name of the file, not the id of the row, so the
    // old bytes would look like the start of this transfer. They go first.
    let _ = old.discard_progress();

    let new_id = fresh.id;
    state.remove(id);
    state.insert(fresh.clone());
    state.save_index();
    // The progress window of the row that was replaced has nothing to show.
    crate::progress::close(app, id);
    crate::complete::close(app, id);
    // The new row takes the place of the old one at once, instead of waiting
    // for the first bytes of the transfer.
    let _ = app.emit(UPDATE_EVENT, fresh.info());
    spawn_run(app, state, fresh);

    Ok(Started {
        id: Some(new_id),
        dest: dest.to_string_lossy().to_string(),
        skipped: false,
    })
}

/// Put a download in another category.
///
/// The category is metadata: the destination, the file on disk and the resume
/// state are left alone. The update event reaches the list, so a rename in the
/// categories pane shows at once.
pub fn set_category(
    app: &AppHandle,
    state: &AppState,
    id: u64,
    category: Option<String>,
) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    download.set_category(category);
    let _ = app.emit(UPDATE_EVENT, download.info());
    state.save_index();
    Ok(())
}

/// Record the category and the note of a download together.
///
/// Both are metadata the user edits in the properties window, so the file on
/// disk and the resume state stay exactly as they are.
pub fn set_meta(
    app: &AppHandle,
    state: &AppState,
    id: u64,
    category: Option<String>,
    description: Option<String>,
) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    download.set_meta(category, description);
    let _ = app.emit(UPDATE_EVENT, download.info());
    state.save_index();
    Ok(())
}

/// Point a download at another file, taking what it already wrote along.
///
/// A finished file is moved itself; an unfinished row takes its part files and
/// manifest, so a resume continues from the bytes already received. A row with
/// a task cannot move, because the transfer would keep writing to the old
/// file. A taken name is a question, never a silent replacement.
pub fn set_destination(
    app: &AppHandle,
    state: &AppState,
    id: u64,
    dest: PathBuf,
    choice: Choice,
) -> Result<String, Refusal> {
    let target = rewrite_destination(state, id, &dest, choice)?;
    if let Some(download) = state.get(id) {
        let _ = app.emit(UPDATE_EVENT, download.info());
    }
    Ok(target.to_string_lossy().to_string())
}

fn rewrite_destination(
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

    let _gate = state.start_gate();
    let (target, replace) = match conflict::resolve(state, dest, answer, Some(id))? {
        conflict::Resolved::Start { dest, replace } => (dest, replace),
        conflict::Resolved::Skip => return Ok(old.dest.clone()),
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
        move_file(&old.dest, &target).map_err(|error| {
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
fn moved_download(
    state: &AppState,
    old: &Download,
    dest: PathBuf,
    replace: bool,
) -> Result<Arc<Download>, String> {
    let previous = old.info();
    let moved = Download::new(
        old.id,
        old.url.clone(),
        dest,
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
        if segment.part.exists() {
            // A file already at the target name belongs to the row that is
            // moving; its own part file is not a conflict with itself.
            if let Err(error) = move_file(&segment.part, &target) {
                for (from, to) in moved_files.iter().rev() {
                    let _ = move_file(to, from);
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
pub fn relocate(app: &AppHandle, state: &AppState, id: u64, folder: PathBuf) -> Result<(), String> {
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
        return resume(app, state, id);
    }
    let wanted = folder.join(name);

    let _gate = state.start_gate();
    let new_dest = match conflict::resolve(state, &wanted, Choice::Rename, Some(id))
        .map_err(|refusal| refusal.message)?
    {
        conflict::Resolved::Start { dest, .. } => dest,
        conflict::Resolved::Skip => return Err("the download was not moved".to_string()),
    };

    let moved = moved_download(state, &old, new_dest, false)?;
    state.replace(moved);
    state.save_index();
    let _ = app.emit(
        UPDATE_EVENT,
        state.get(id).expect("the moved download").info(),
    );
    resume(app, state, id)
}

/// Put a download in the queue, parking its task but keeping its bytes.
///
/// The row then waits for Start Queue, which starts it the same way a paused
/// one is started. A finished row has nothing left to queue.
pub fn queue(app: &AppHandle, state: &AppState, id: u64) -> Result<(), String> {
    queue_row(state, id)?;
    if let Some(download) = state.get(id) {
        let _ = app.emit(UPDATE_EVENT, download.info());
    }
    Ok(())
}

fn queue_row(state: &AppState, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    match download.status() {
        Status::Completed => return Err("a finished download cannot be queued".to_string()),
        // The state the user asked for is the state it is in.
        Status::Queued => return Ok(()),
        _ => {}
    }
    // Stopping keeps the parts and the manifest; the queue starts the row
    // again from the bytes already received.
    // A task that is still closing would otherwise finish as cancelled and
    // overwrite the queue state it was just given; a new epoch makes its last
    // write a no-op.
    if download.is_running() {
        download.next_epoch();
    }
    download.cancel();
    download.set_status(Status::Queued);
    download.save_manifest();
    state.save_index();
    Ok(())
}

/// Take a queued download back out of the queue and leave it paused.
pub fn dequeue(app: &AppHandle, state: &AppState, id: u64) -> Result<(), String> {
    dequeue_row(state, id)?;
    if let Some(download) = state.get(id) {
        let _ = app.emit(UPDATE_EVENT, download.info());
    }
    Ok(())
}

fn dequeue_row(state: &AppState, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    if download.status() != Status::Queued {
        return Err("this download is not in the queue".to_string());
    }
    download.set_status(Status::Paused);
    state.save_index();
    Ok(())
}

/// Delete the file of a finished download and forget the row.
///
/// The caller asks first: this is the one command that removes something from
/// the disk. An unfinished row is refused because its file does not exist yet
/// and its parts are the resume state.
pub fn delete_file(app: &AppHandle, state: &AppState, id: u64) -> Result<(), String> {
    delete_file_of(state, id)?;
    // The progress and completion windows of a row that is gone have nothing
    // to show.
    crate::progress::close(app, id);
    crate::complete::close(app, id);
    Ok(())
}

fn delete_file_of(state: &AppState, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    match download.status() {
        Status::Completed => {}
        Status::Running | Status::Queued => {
            return Err("stop the download before deleting its file".to_string())
        }
        _ => return Err("only a finished file can be deleted".to_string()),
    }
    if !download.dest.is_file() {
        return Err(format!("{} is not there any more", download.dest.display()));
    }
    std::fs::remove_file(&download.dest)
        .map_err(|error| format!("the file could not be deleted: {error}"))?;
    // A finished download has no parts left; remove a manifest that survived
    // an earlier version instead of leaving it beside a file that is gone.
    let _ = std::fs::remove_file(&download.manifest);
    state.remove(id);
    state.save_index();
    Ok(())
}

/// Put `from` at `to`, across volumes as well.
fn move_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(from, to)?;
            std::fs::remove_file(from)
        }
    }
}

/// Hold a running download back. The bytes stay where they are.
pub fn pause(app: &AppHandle, state: &AppState, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    // A row that is finished or stopped has no task to hold back; saying so
    // is better than leaving a row that claims to be paused.
    match download.status() {
        // A second click that lands after the first command answered is not
        // an error: the download is already where the user asked it to be.
        Status::Paused => return Ok(()),
        Status::Running | Status::Queued => {}
        _ => return Err("this download is not running".to_string()),
    }
    download.pause();
    download.set_status(Status::Paused);
    // The pause is a good moment to make the resume state exact: the bytes
    // that arrived since the last periodic write are recorded now.
    download.save_manifest();
    let _ = app.emit(UPDATE_EVENT, download.info());
    state.save_index();
    Ok(())
}

/// Let a paused or stopped download run again.
pub fn resume(app: &AppHandle, state: &AppState, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    let info = download.info();
    if info.status == Status::Completed {
        return Err("this download has already finished".to_string());
    }

    download.control.resume();
    if info.status == Status::Failed {
        // The message of the old failure belongs to the attempt that just
        // ended, not to the one the user is starting now.
        download.clear_error();
    }

    // A download whose task is gone — it was cancelled, or it failed — is
    // started again, from the parts that are on disk. A cancel that has been
    // asked for but not answered yet also asks for a new run: the engine
    // serialises the two, so the new task starts where the old one stopped.
    let restarting = !download.is_running() || download.is_cancelled(download.current_epoch());
    // Say that it runs before the task starts. The task may have to wait for a
    // cancelled run to let go, and the window should not keep offering Resume
    // while the run it just asked for is on its way.
    if info.status != Status::Running {
        download.set_status(Status::Running);
        let _ = app.emit(UPDATE_EVENT, download.info());
    }
    if restarting {
        let limits = state.limits().clone();
        let reporter = Arc::new(AppReporter(app.clone()));
        let running = download.clone();
        // Claim the run now. The task may wait for a cancelled run to finish,
        // and a stop that lands in that window has to cover it too.
        let request = running.request_run();
        tauri::async_runtime::spawn(async move {
            idm_core::downloader::run_claimed(reporter, limits, running, request).await;
        });
    }

    state.save_index();
    Ok(())
}

/// Stop a download. Its parts are kept, so Resume continues from them.
pub fn cancel(app: &AppHandle, state: &AppState, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    match download.status() {
        // Stopping a download that is already stopped is not a failure: the
        // state the user asked for is the state it is in.
        Status::Cancelled => return Ok(()),
        Status::Completed => return Err("a finished download cannot be cancelled".to_string()),
        _ => {}
    }
    download.cancel();
    // A stopped download is not a failed one: the message of the attempt the
    // user just stopped would read as a failure of a row that is now stopped.
    download.clear_error();
    download.set_status(Status::Cancelled);
    // The ranges hold the bytes they received; record them before the task
    // winds down, so a resume continues from the right place.
    download.save_manifest();
    let _ = app.emit(UPDATE_EVENT, download.info());
    state.save_index();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use idm_core::segmenter::{part_path, Segment, SegmentStatus};
    use std::path::Path;
    use std::sync::atomic::Ordering;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("my-idm-clone-actions-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn row(id: u64, dest: &Path) -> Arc<Download> {
        Download::new(
            id,
            "https://example.org/film.mp4".to_string(),
            dest.to_path_buf(),
            4,
            None,
            None,
        )
        .expect("a download")
    }

    #[test]
    fn a_finished_file_moves_with_its_metadata() {
        let dir = temp_dir("completed");
        let source = dir.join("old").join("film.mp4");
        std::fs::create_dir_all(source.parent().expect("a folder")).expect("folder");
        std::fs::write(&source, b"done").expect("write");

        let state = AppState::default();
        let download = row(1, &source);
        download.set_meta(Some("Video".to_string()), Some("a note".to_string()));
        download.set_status(Status::Completed);
        state.insert(download);

        let target = dir.join("new").join("film.mp4");
        let written = rewrite_destination(&state, 1, &target, Choice::Ask).expect("a free name");
        assert_eq!(written, target);
        assert_eq!(std::fs::read(&target).expect("the moved file"), b"done");
        assert!(!source.exists(), "the old file is gone");

        let info = state.get(1).expect("the row").info();
        assert_eq!(info.status, Status::Completed);
        assert_eq!(info.dest, target.to_string_lossy());
        assert_eq!(info.category.as_deref(), Some("Video"));
        assert_eq!(info.description.as_deref(), Some("a note"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_file_is_not_a_move() {
        let dir = temp_dir("same");
        let dest = dir.join("film.mp4");
        std::fs::write(&dest, b"done").expect("write");

        let state = AppState::default();
        let download = row(1, &dest);
        download.set_status(Status::Completed);
        state.insert(download);

        let written = rewrite_destination(&state, 1, &dest, Choice::Ask)
            .expect("its own file is no conflict");
        assert_eq!(written, dest);
        assert_eq!(std::fs::read(&dest).expect("the file"), b"done");
        assert_eq!(state.get(1).expect("the row").dest, dest);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_taken_name_asks_before_a_finished_file_moves() {
        let dir = temp_dir("taken");
        let source = dir.join("film.mp4");
        std::fs::write(&source, b"new").expect("write");
        let taken = dir.join("taken").join("film.mp4");
        std::fs::create_dir_all(taken.parent().expect("a folder")).expect("folder");
        std::fs::write(&taken, b"old").expect("write");

        let state = AppState::default();
        let download = row(1, &source);
        download.set_status(Status::Completed);
        state.insert(download);

        let refusal = rewrite_destination(&state, 1, &taken, Choice::Ask).expect_err("a question");
        assert_eq!(refusal.kind, "exists");
        assert_eq!(
            std::fs::read(&taken).expect("the file that was there"),
            b"old"
        );

        let moved = rewrite_destination(&state, 1, &taken, Choice::Rename).expect("a free name");
        assert_eq!(moved, dir.join("taken").join("film (2).mp4"));
        assert_eq!(
            std::fs::read(&taken).expect("the file that was there"),
            b"old"
        );
        assert_eq!(std::fs::read(&moved).expect("the moved file"), b"new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_replaced_file_only_goes_after_the_answer() {
        let dir = temp_dir("replace");
        let source = dir.join("film.mp4");
        std::fs::write(&source, b"new").expect("write");
        let taken = dir.join("taken").join("film.mp4");
        std::fs::create_dir_all(taken.parent().expect("a folder")).expect("folder");
        std::fs::write(&taken, b"old").expect("write");

        let state = AppState::default();
        let download = row(1, &source);
        download.set_status(Status::Completed);
        state.insert(download);

        let written = rewrite_destination(&state, 1, &taken, Choice::Replace).expect("replace");
        assert_eq!(written, taken);
        assert_eq!(std::fs::read(&taken).expect("the replaced file"), b"new");
        assert!(!source.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unfinished_download_takes_its_parts_along() {
        let dir = temp_dir("parts");
        let source = dir.join("film.mp4");
        let state = AppState::default();
        let download = row(1, &source);
        download.set_status(Status::Paused);
        let part = part_path(&download.dir, &download.stem, 0);
        std::fs::write(&part, b"half").expect("write");
        {
            let mut segments = download.segments.lock().expect("segments");
            segments.push(Segment {
                id: 0,
                start: 0,
                end: 10,
                downloaded: 4,
                part: part.clone(),
                status: SegmentStatus::Paused,
            });
        }
        download.total.store(10, Ordering::SeqCst);
        state.insert(download);

        let target = dir.join("new").join("film.mp4");
        rewrite_destination(&state, 1, &target, Choice::Ask).expect("a free name");

        let info = state.get(1).expect("the row").info();
        assert_eq!(info.status, Status::Paused, "a move is not a resume");
        assert_eq!(info.downloaded, 4, "the bytes travel with the row");
        let new_part = part_path(target.parent().expect("a folder"), "film.mp4", 0);
        assert_eq!(std::fs::read(&new_part).expect("the moved part"), b"half");
        assert!(!part.exists());
        assert!(
            target.with_file_name("film.mp4.idm.json").exists(),
            "the manifest goes with the parts"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_row_with_a_task_cannot_change_its_file() {
        let dir = temp_dir("busy");
        let state = AppState::default();
        let download = row(1, &dir.join("film.mp4"));
        download.set_status(Status::Running);
        state.insert(download);

        let refusal = rewrite_destination(&state, 1, &dir.join("other.mp4"), Choice::Ask)
            .expect_err("a running row");
        assert_eq!(refusal.kind, "busy");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleting_a_finished_file_removes_the_file_and_the_row() {
        let dir = temp_dir("delete");
        let dest = dir.join("film.mp4");
        std::fs::write(&dest, b"done").expect("write");

        let state = AppState::default();
        let download = row(1, &dest);
        download.set_status(Status::Completed);
        state.insert(download);

        delete_file_of(&state, 1).expect("the file is deleted");
        assert!(!dest.exists());
        assert!(state.get(1).is_none(), "the row is gone too");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_not_deleted_silently() {
        let dir = temp_dir("missing");
        let state = AppState::default();
        let download = row(1, &dir.join("film.mp4"));
        download.set_status(Status::Completed);
        state.insert(download);

        let error = delete_file_of(&state, 1).expect_err("nothing to delete");
        assert!(error.contains("not there"), "the error was: {error}");
        assert!(state.get(1).is_some(), "the record stays");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_running_download_keeps_its_file() {
        let dir = temp_dir("delete-busy");
        let dest = dir.join("film.mp4");
        std::fs::write(&dest, b"part").expect("write");

        let state = AppState::default();
        let download = row(1, &dest);
        download.set_status(Status::Running);
        state.insert(download);

        let error = delete_file_of(&state, 1).expect_err("a running row");
        assert!(
            error.contains("stop the download"),
            "the error was: {error}"
        );
        assert!(dest.exists(), "the file is left alone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_queue_parks_a_row_and_lets_it_out_again() {
        let dir = temp_dir("queue");
        let state = AppState::default();
        let download = row(1, &dir.join("film.mp4"));
        download.set_status(Status::Paused);
        state.insert(download);

        queue_row(&state, 1).expect("the row is queued");
        assert_eq!(state.get(1).expect("the row").status(), Status::Queued);
        // A second click is not a failure.
        queue_row(&state, 1).expect("already queued");

        dequeue_row(&state, 1).expect("the row leaves the queue");
        assert_eq!(state.get(1).expect("the row").status(), Status::Paused);

        state.get(1).expect("the row").set_status(Status::Completed);
        assert!(
            queue_row(&state, 1).is_err(),
            "a finished row has nothing to queue"
        );
        assert!(dequeue_row(&state, 1).is_err(), "it is not in the queue");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
