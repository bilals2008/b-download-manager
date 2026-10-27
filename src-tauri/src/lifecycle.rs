//! What the close and minimize buttons mean, and how the application leaves.
//!
//! A close request is a question, not an order: with the window in the tray
//! the application keeps downloading, and with active downloads it must not
//! disappear because a title-bar button was pressed. This module owns that
//! decision, the one confirmation the window shows, and the single shutdown
//! path every deliberate exit goes through.
//!
//! Shutdown holds active transfers before saving: a paused download writes the
//! exact byte ranges it has, so the next start resumes where this one stopped
//! instead of trusting a manifest that is a couple of seconds old.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Window};

use idm_core::downloader::Status;
use idm_core::log;

use crate::actions;
use crate::bridge;
use crate::progress;
use crate::state::AppState;
use crate::tray;

/// The event that asks the window whether to keep running or exit.
pub const CLOSE_EVENT: &str = "app://close-requested";

/// What a close request should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Hide the window; downloads and the tray keep running.
    Hide,
    /// Ask the window, because exiting would stop active downloads.
    Ask,
    /// Save everything and leave.
    Exit,
}

/// Decide what closing the main window means.
///
/// `unfinished` counts running, queued and paused rows. The tray must exist
/// for hiding to be reversible; without it, the application exits rather than
/// leaving a process with no window and no icon.
pub fn close_action(close_to_tray: bool, unfinished: usize, tray_available: bool) -> CloseAction {
    if !tray_available {
        return CloseAction::Exit;
    }
    if close_to_tray {
        CloseAction::Hide
    } else if unfinished > 0 {
        CloseAction::Ask
    } else {
        CloseAction::Exit
    }
}

/// The downloads that are not finished.
fn unfinished(state: &AppState) -> usize {
    state
        .list()
        .iter()
        .filter(|download| {
            matches!(
                download.status(),
                Status::Running | Status::Queued | Status::Paused
            )
        })
        .count()
}

/// The answer the window sends back: it will stay in the tray.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseRequest {
    active: usize,
}

/// Handle the main window's close button.
///
/// The caller has already prevented the native close. Every branch ends here:
/// hide, ask, or leave, never a webview destroyed behind a running download.
pub fn close_requested(window: &Window) {
    let app = window.app_handle();
    let state = app.state::<AppState>();
    let active = unfinished(&state);
    let action = close_action(
        state.preferences().close_to_tray,
        active,
        state.tray_available(),
    );

    match action {
        CloseAction::Hide => {
            if let Err(error) = window.hide() {
                log::write("warn", &format!("the window did not hide: {error}"));
            }
            tray::refresh(app);
        }
        CloseAction::Ask => {
            // The question belongs in front of the user, even if the window
            // was minimized when the close arrived.
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            if let Err(error) = app.emit(CLOSE_EVENT, CloseRequest { active }) {
                log::write(
                    "warn",
                    &format!("the close question was not asked: {error}"),
                );
            }
        }
        CloseAction::Exit => shutdown(app),
    }
}

/// Hide the window when minimizing is configured as going to the tray.
///
/// Returns whether the window was hidden, so the caller can tell the two
/// meanings apart when it has no preference of its own.
pub fn minimize_to_tray(window: &Window) -> bool {
    if window.label() != "main" {
        return false;
    }
    let app = window.app_handle();
    let state = app.state::<AppState>();
    let preferences = state.preferences();
    if !preferences.minimize_to_tray || !state.tray_available() {
        return false;
    }
    if let Err(error) = window.hide() {
        log::write("warn", &format!("the window did not hide: {error}"));
        return false;
    }
    tray::refresh(app);
    true
}

/// The minimize button of a window of this application.
///
/// Only the main window has a tray meaning; the smaller windows simply
/// minimize, because they are not the application itself.
#[tauri::command]
pub fn minimize_window(window: Window) {
    if !minimize_to_tray(&window) {
        let _ = window.minimize();
    }
}

/// Hide the main window on request, without touching its downloads.
#[tauri::command]
pub fn hide_main_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("the main window is not there")?;
    window.hide().map_err(|error| error.to_string())?;
    tray::refresh(&app);
    Ok(())
}

/// The one genuine exit.
///
/// Every deliberate way out calls this: the tray, the menu, the close question
/// and the quit command. It holds running transfers, writes their resumes,
/// stops the bridge from taking new work, removes the notification icons and
/// only then exits. A second call is a no-op, so the several exit paths cannot
/// pause or save twice.
pub fn shutdown(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.begin_shutdown() {
        return;
    }
    log::write("info", "the application is shutting down");

    for download in state.list() {
        if download.status() == Status::Running {
            // A pause records exact byte ranges and tells the task to stop.
            // A row that finished between the list and the pause is left as
            // it is; the error only says there was nothing to hold.
            let _ = actions::pause(app, &state, download.id);
        }
    }

    state.save_manifests();
    state.save_index();
    state.save_preferences();
    bridge::stop(&state);
    tray::remove(app);
    for download in state.list() {
        progress::remove_tray(app, download.id);
    }

    log::write("info", "the downloads are saved; exiting");
    app.exit(0);
}

/// Save the state on an exit this module did not initiate.
///
/// The operating system can end the process without a close request (logoff,
/// a system shutdown). This is the last moment to keep the resume files; it
/// does not pause anything, because the process may already be going.
pub fn save_on_exit(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.shutting_down() {
        return;
    }
    state.save_manifests();
    state.save_index();
    state.save_preferences();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_to_the_tray_never_asks_and_never_exits() {
        assert_eq!(close_action(true, 0, true), CloseAction::Hide);
        assert_eq!(close_action(true, 4, true), CloseAction::Hide);
    }

    #[test]
    fn a_quiet_close_with_no_tray_exits_instead_of_stranding_the_app() {
        assert_eq!(close_action(true, 0, false), CloseAction::Exit);
        assert_eq!(close_action(false, 3, false), CloseAction::Exit);
    }

    #[test]
    fn an_idle_close_exits_when_the_tray_is_not_the_destination() {
        assert_eq!(close_action(false, 0, true), CloseAction::Exit);
    }

    #[test]
    fn active_downloads_make_exiting_a_question() {
        assert_eq!(close_action(false, 1, true), CloseAction::Ask);
        assert_eq!(close_action(false, 9, true), CloseAction::Ask);
    }

    #[test]
    fn the_question_carries_only_the_count_the_window_shows() {
        let body = serde_json::to_value(CloseRequest { active: 3 }).expect("a body");
        assert_eq!(body["active"], 3);
    }
}
