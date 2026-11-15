//! Parking downloads in the queue, ordering them, and starting or holding it.

use tauri::AppHandle;

use idm_core::downloader::Status;

use super::announce;
use crate::state::AppState;

/// Put a download in the queue, parking its task but keeping its bytes.
///
/// The row then waits for Start Queue, which starts it the same way a paused
/// one is started. A finished row has nothing left to queue.
pub fn queue(app: &AppHandle, state: &AppState, id: u64) -> Result<(), String> {
    queue_row(state, id)?;
    state.queue().enqueue(id);
    state.save_queue();
    if let Some(download) = state.get(id) {
        announce(app, &download);
    }
    crate::tray::refresh(app);
    Ok(())
}

pub(super) fn queue_row(state: &AppState, id: u64) -> Result<(), String> {
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
    state.queue().dequeue(id);
    state.save_queue();
    if let Some(download) = state.get(id) {
        announce(app, &download);
    }
    crate::tray::refresh(app);
    Ok(())
}

pub(super) fn dequeue_row(state: &AppState, id: u64) -> Result<(), String> {
    let download = state.get(id).ok_or("unknown download")?;
    if download.status() != Status::Queued {
        return Err("this download is not in the queue".to_string());
    }
    download.set_status(Status::Paused);
    state.save_index();
    Ok(())
}

/// Start the queue: activate the scheduler and begin starting queued
/// downloads in order, up to `max_simultaneous` at a time.
pub fn start_queue(app: &AppHandle, state: &AppState) -> Result<(), String> {
    state.queue().start_queue(app, state);
    state.save_queue();
    state.save_index();
    Ok(())
}

/// Stop the queue: deactivate the scheduler and pause every download it
/// has running.
pub fn stop_queue(app: &AppHandle, state: &AppState) -> Result<(), String> {
    state.queue().stop_queue(app, state);
    state.save_queue();
    state.save_index();
    Ok(())
}

/// Move a queued download to a new position (0 = first).
pub fn set_queue_position(state: &AppState, id: u64, position: u32) -> Result<(), String> {
    state
        .queue()
        .reorder(id, position as usize)
        .map_err(|error| error.to_string())?;
    state.save_queue();
    state.save_index();
    Ok(())
}

/// Set the maximum number of simultaneous downloads the queue may start.
pub fn set_max_simultaneous(state: &AppState, max: u32) -> u32 {
    state.queue().set_max_simultaneous(max);
    state.save_queue();
    state.save_index();
    state.queue().max_simultaneous()
}
