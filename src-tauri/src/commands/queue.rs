//! The queue: order, the simultaneous limit, and starting or holding it.

use tauri::{AppHandle, State};

use crate::actions;
use crate::state::AppState;

/// Park a download in the queue, keeping the bytes it already received.
#[tauri::command]
pub(crate) fn queue_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    actions::queue(&app, &state, id)
}

/// Take a download out of the queue and leave it paused.
#[tauri::command]
pub(crate) fn dequeue_download(app: AppHandle, state: State<'_, AppState>, id: u64) -> Result<(), String> {
    actions::dequeue(&app, &state, id)
}

/// Activate the queue and start queued downloads in order, up to the
/// simultaneous limit.
#[tauri::command]
pub(crate) fn start_queue(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    actions::start_queue(&app, &state)
}

/// Stop the queue and pause every download it has running.
#[tauri::command]
pub(crate) fn stop_queue(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    actions::stop_queue(&app, &state)
}

/// Move a queued download to a new position (0 = first).
#[tauri::command]
pub(crate) fn set_queue_position(state: State<'_, AppState>, id: u64, position: u32) -> Result<(), String> {
    actions::set_queue_position(&state, id, position)
}

/// Set the maximum number of simultaneous downloads the queue may start.
#[tauri::command]
pub(crate) fn set_max_simultaneous(state: State<'_, AppState>, max: u32) -> u32 {
    actions::set_max_simultaneous(&state, max)
}

/// The queue's current state: order, running set, limit, active flag.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QueueState {
    order: Vec<u64>,
    running: Vec<u64>,
    max_simultaneous: u32,
    active: bool,
}

/// Return the queue's current state for the UI.
#[tauri::command]
pub(crate) fn get_queue(state: State<'_, AppState>) -> QueueState {
    let snapshot = state.queue().snapshot();
    QueueState {
        order: snapshot.order,
        running: snapshot.running,
        max_simultaneous: snapshot.max_simultaneous,
        active: snapshot.active,
    }
}
