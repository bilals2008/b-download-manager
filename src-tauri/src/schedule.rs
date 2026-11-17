//! The queue's schedule.
//!
//! The rule itself lives in the engine (`idm_core::schedule`), because a
//! browser hand-over has to obey it with no window open. This module is the
//! clock: a thread that watches the schedule, starts the queue when a window
//! opens, and does what the reader asked for when it closes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use idm_core::schedule::{EndAction, Schedule};

use tauri::Manager;

use crate::state::AppState;

/// How often the clock is read. A minute is the finest window a reader sets,
/// so a quarter of a minute is finer than the rule needs.
const TICK: Duration = Duration::from_secs(15);

/// What the window shows about the schedule.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleStatus {
    /// The rule itself, as the window edits it.
    pub schedule: Schedule,
    /// Whether a schedule is in force at all.
    pub enabled: bool,
    /// Whether now is inside a window.
    pub open: bool,
    /// The rule in one sentence.
    pub summary: String,
    /// When the next window opens, as a reader reads it.
    pub next: Option<String>,
    /// What happens when the window closes.
    pub end_action: EndAction,
    /// Whether the computer may be powered off when the window closes.
    pub power_off: bool,
}

/// What a tick of the clock found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    /// Nothing changed.
    Nothing,
    /// A window opened: the queue starts.
    Opened,
    /// A window closed: the queue does what the schedule says.
    Closed,
}

/// What a tick of the clock means, given whether the window was open before.
pub fn decide(was_open: bool, open_now: bool) -> Tick {
    match (was_open, open_now) {
        (false, true) => Tick::Opened,
        (true, false) => Tick::Closed,
        _ => Tick::Nothing,
    }
}

/// The thread that watches the clock.
pub struct ScheduleRunner {
    /// Whether the window was open at the last tick.
    open: AtomicBool,
    /// Whether the thread should stop.
    stop: AtomicBool,
}

impl ScheduleRunner {
    /// A runner that has not seen a window yet.
    pub fn new() -> Self {
        Self {
            open: AtomicBool::new(false),
            stop: AtomicBool::new(false),
        }
    }

}

impl Default for ScheduleRunner {
    fn default() -> Self {
        Self::new()
    }
}

/// Start the clock. The first reading happens at once, so an application
/// that starts inside a window begins its queue without a click.
pub fn start(app: tauri::AppHandle) {
    let runner = ScheduleRunner::new();
    tick(&app, &runner);
    std::thread::Builder::new()
        .name("schedule".to_string())
        .spawn(move || {
            while !runner.stop.load(Ordering::SeqCst) {
                std::thread::sleep(TICK);
                tick(&app, &runner);
            }
        })
        .ok();
}

/// Read the clock once and act on what it says.
fn tick(app: &tauri::AppHandle, runner: &ScheduleRunner) {
    let state = app.state::<AppState>();
    let settings = state.engine_settings();
    let schedule = settings.schedule.clone();
    let open_now = schedule.is_open_at(idm_core::schedule::now_local());
    let was_open = runner.open.swap(open_now, Ordering::SeqCst);
    match decide(was_open, open_now) {
        Tick::Nothing => {}
        Tick::Opened => {
            idm_core::log::write(
                "info",
                &format!("the queue's window opened: {}", schedule.summary()),
            );
            state.queue().start_queue(app, &state);
        }
        Tick::Closed => {
            idm_core::log::write(
                "info",
                &format!("the queue's window closed: {}", schedule.summary()),
            );
            close_window(app, &state, schedule.end_action, settings.power_off_after_queue);
        }
    }
}

/// What happens when the window closes.
fn close_window(
    app: &tauri::AppHandle,
    state: &AppState,
    action: EndAction,
    power_off_allowed: bool,
) {
    match action {
        EndAction::KeepRunning => {}
        EndAction::PauseQueue => {
            state.queue().stop_queue(app, state);
        }
        EndAction::ShutDown => {
            state.queue().stop_queue(app, state);
            if !power_off_allowed {
                idm_core::log::write(
                    "warn",
                    "the schedule asked for a shutdown, but the reader has not allowed one",
                );
                return;
            }
            save_everything(app, state);
            power_off();
        }
    }
}

/// Pause the running downloads and write the index, so nothing is lost when
/// the machine goes down.
fn save_everything(app: &tauri::AppHandle, state: &AppState) {
    for download in state.list() {
        let _ = crate::actions::pause(app, state, download.info().id);
    }
    state.save_manifests();
    state.save_index();
    state.save_queue();
}

/// Ask the operating system to power the machine off.
///
/// The system's own timer is used, with a minute to cancel, so a mistaken
/// schedule is not a machine that cannot be reached.
#[cfg(windows)]
fn power_off() {
    let _ = std::process::Command::new("shutdown")
        .args([
            "/s",
            "/t",
            "60",
            "/c",
            "The download queue has finished.",
        ])
        .spawn();
}

#[cfg(not(windows))]
fn power_off() {
    idm_core::log::write(
        "warn",
        "the schedule asked for a shutdown, which this system does not do here",
    );
}

/// The schedule as the window shows it.
pub fn status(state: &AppState) -> ScheduleStatus {
    let settings = state.engine_settings();
    let schedule = settings.schedule.clone();
    let now = idm_core::schedule::now_local();
    let open = schedule.is_open_at(now);
    let next = schedule
        .next_open_at(now)
        .map(|window| window.describe())
        .filter(|_| !open);
    ScheduleStatus {
        schedule: schedule.clone(),
        enabled: !schedule.is_empty(),
        open,
        summary: schedule.summary(),
        next,
        end_action: schedule.end_action,
        power_off: settings.power_off_after_queue,
    }
}

/// Store the schedule the reader chose.
pub fn store(state: &AppState, schedule: Schedule, power_off: bool) -> ScheduleStatus {
    let schedule = schedule.sanitized();
    state.modify_settings(|settings| {
        settings.schedule = schedule.clone();
        settings.power_off_after_queue = power_off;
    });
    state.save_engine_settings();
    status(state)
}

/// The schedule as the window shows it.
#[tauri::command]
pub fn queue_schedule(state: tauri::State<'_, AppState>) -> ScheduleStatus {
    status(&state)
}

/// Keep the schedule the reader chose.
#[tauri::command]
pub fn set_queue_schedule(
    state: tauri::State<'_, AppState>,
    schedule: Schedule,
    power_off: bool,
) -> ScheduleStatus {
    store(&state, schedule, power_off)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_that_opens_starts_the_queue() {
        assert_eq!(decide(false, true), Tick::Opened);
    }

    #[test]
    fn a_window_that_closes_ends_it() {
        assert_eq!(decide(true, false), Tick::Closed);
    }

    #[test]
    fn nothing_happens_when_the_window_did_not_change() {
        assert_eq!(decide(false, false), Tick::Nothing);
        assert_eq!(decide(true, true), Tick::Nothing);
    }
}
