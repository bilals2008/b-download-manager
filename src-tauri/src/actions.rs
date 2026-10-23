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
pub fn start(
    app: &AppHandle,
    state: &AppState,
    url: String,
    dest: PathBuf,
    options: StartOptions,
) -> Result<u64, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("a URL is required".to_string());
    }
    if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("ftp://")) {
        return Err("the URL must start with http://, https:// or ftp://.".to_string());
    }
    if dest.as_os_str().is_empty() {
        return Err("a destination file is required".to_string());
    }
    crate::dest::validate(&dest)?;

    let download = Download::new(
        state.next_id(),
        url,
        dest,
        options.connections.unwrap_or(DEFAULT_CONNECTIONS).clamp(1, 32),
        state.limits().proxy().as_deref(),
        options.referer.as_deref(),
    )?;
    download.set_meta(options.category, options.description);

    let id = download.id;
    state.insert(download.clone());
    state.save_index();

    // A download that was added to the queue waits for Start Queue. The row
    // is there at once, in the state the queue expects, and no task is
    // spawned: the queue starts it the same way a paused one is started.
    if options.queue {
        let _ = app.emit(UPDATE_EVENT, download.info());
        return Ok(id);
    }

    let limits: Limits = state.limits().clone();
    let request = download.request_run();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        idm_core::downloader::run_claimed(Arc::new(AppReporter(app)), limits, download, request)
            .await;
    });

    Ok(id)
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
