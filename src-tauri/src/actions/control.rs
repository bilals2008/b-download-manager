//! Holding a running download back, letting it go, and removing its file.

use tauri::AppHandle;

use idm_core::downloader::Status;

use super::{announce, start_download_run};
use crate::state::AppState;

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
    crate::tray::refresh(app);
    Ok(())
}

pub(super) fn delete_file_of(state: &AppState, id: u64) -> Result<(), String> {
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
    // A finished download has no parts left; forget the whole temporary set,
    // including a manifest that survived an earlier version and the staging
    // folder that goes with it, rather than leaving either beside a file
    // that is gone.
    download.forget_progress();
    state.remove(id);
    state.save_index();
    Ok(())
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
    announce(app, &download);
    state.save_index();
    crate::tray::refresh(app);
    Ok(())
}

/// Let a paused or stopped download run again.
///
/// `queue_managed` marks a download the queue started, so the scheduler can
/// start the next one when this run ends.
pub fn resume(
    app: &AppHandle,
    state: &AppState,
    id: u64,
    queue_managed: bool,
) -> Result<(), String> {
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
        announce(app, &download);
    }
    if restarting {
        start_download_run(app, state, download, queue_managed);
    }

    state.save_index();
    crate::tray::refresh(app);
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
    announce(app, &download);
    state.save_index();
    crate::tray::refresh(app);
    Ok(())
}
