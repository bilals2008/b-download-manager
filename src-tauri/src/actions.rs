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
        options.connections.unwrap_or(DEFAULT_CONNECTIONS).clamp(1, 32),
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
pub fn retry(
    app: &AppHandle,
    state: &AppState,
    id: u64,
    restart: bool,
) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    match download.status() {
        Status::Failed | Status::Cancelled | Status::Paused => {}
        Status::Running | Status::Queued => {
            return Err("the download is still running".to_string())
        }
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
    let old = state
        .get(id)
        .ok_or_else(|| refused("invalid", "unknown download".to_string(), std::path::Path::new("")))?;
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

/// Move a download that has no task to another folder.
///
/// The part files and the manifest travel with it, so a server that supports
/// ranges continues from the bytes already received. The new name is chosen
/// to be free: a move never replaces a file in the target folder.
pub fn relocate(
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

    let moved = Download::new(
        old.id,
        old.url.clone(),
        new_dest.clone(),
        old.max_connections,
        state.limits().proxy().as_deref(),
        old.referer.as_deref(),
    )?;
    moved.set_speed_limit(old.speed_limit());

    let old_segments = old
        .segments
        .lock()
        .expect("segments lock poisoned")
        .clone();
    let mut new_segments = Vec::with_capacity(old_segments.len());
    let mut moved_files: Vec<(PathBuf, PathBuf)> = Vec::new();
    for mut segment in old_segments {
        let target = part_path(&moved.dir, &moved.stem, segment.id);
        if segment.part.exists() {
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

    moved.total.store(old.total.load(std::sync::atomic::Ordering::SeqCst), std::sync::atomic::Ordering::SeqCst);
    moved
        .next_segment_id
        .store(old.next_segment_id.load(std::sync::atomic::Ordering::SeqCst), std::sync::atomic::Ordering::SeqCst);
    *moved.segments.lock().expect("segments lock poisoned") = new_segments;
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

    state.replace(moved);
    state.save_index();
    let _ = app.emit(
        UPDATE_EVENT,
        state.get(id).expect("the moved download").info(),
    );
    resume(app, state, id)
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
