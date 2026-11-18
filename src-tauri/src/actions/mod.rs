//! The download actions the window and the browser bridge share.
//!
//! Every way of starting, holding, resuming or stopping a download goes
//! through this module: a toolbar button, the add dialog, and a request from
//! the Firefox extension all end up in the same place, so the two front ends
//! cannot drift apart in what a download may do.
//!
//! Starting a row and moving its files live here, because they share the
//! staging tree and the conflict gate. The queue and the running controls are
//! [`queue`] and [`control`]; moving a file is [`destination`].

mod control;
mod destination;
mod queue;

pub use control::{cancel, delete_file, pause, resume};
pub use destination::{relocate, set_destination};
pub use queue::{
    dequeue, queue, set_max_simultaneous, set_queue_position, start_queue, stop_queue,
};

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use idm_core::downloader::{Download, DownloadInfo, Reporter, Status, DEFAULT_CONNECTIONS};
use idm_core::limits::Limits;

use crate::conflict::{self, Choice, Refusal, Started};
use crate::debrid;
use crate::state::AppState;

/// Event carrying the state of one download to the UI.
pub const UPDATE_EVENT: &str = "download://update";

/// The sentence a start answers with when the application's own storage for
/// unfinished bytes could not be created. The alternative — parts in the
/// user's download folder — is never taken, so this reaches the window as a
/// refusal the reader can act on.
const THE_STORAGE_IS_NOT_READY: &str = "the application's storage folder is not available. \
                                        Free some space on the disk where the application keeps \
                                        its data, and start again.";

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
    /// Put the file in the folder its category saves to. The add dialog clears
    /// this when the user picked a folder themselves.
    pub use_category_folder: bool,
    /// The note the user typed in the add dialog.
    pub description: Option<String>,
    /// What the File Exists dialog answered.
    pub conflict: Choice,
    /// The export format the reader picked for a Drive editor link.
    pub drive_export: Option<String>,
}

/// The reporter the desktop app hands to the engine: it pushes the state of
/// a download into the user interface.
struct AppReporter(AppHandle);

impl Reporter for AppReporter {
    fn report(&self, info: &DownloadInfo) {
        let _ = self.0.emit(UPDATE_EVENT, info.clone());
        // The tray follows the same reports as the window, so its tooltip is
        // current whether or not a window is watching. Both calls only write
        // when the line they would show has changed.
        crate::progress::refresh_tray(&self.0, info);
        crate::tray::refresh(&self.0);

        // A finished file gets the window the reference manager opens for it,
        // unless the user
        // turned that window off in Options. The window is built on the main
        // thread, the one the window toolkit runs on, so it is handed to the
        // shower that gives each build its own moment.
        //
        // The progress window of that download goes first: the completion
        // window says the same thing better, and two windows over one file
        // read as one that will not stay closed.
        if info.status == Status::Completed && self.0.state::<AppState>().show_complete_dialog() {
            crate::progress::close(&self.0, info.id);
            let app = self.0.clone();
            let info = info.clone();
            crate::shower::build(&app, move |app| crate::complete::show(app, &info));
        }

        // A finished archive is unpacked beside itself when the reader asked
        // for it. The unpacking runs off this thread: it is minutes of disk
        // work, and the report is what delivers the state of every download.
        if info.status == Status::Completed
            && self
                .0
                .state::<AppState>()
                .engine_settings()
                .extract_after_done
        {
            let app = self.0.clone();
            let id = info.id;
            tauri::async_runtime::spawn(async move {
                crate::extract::extract_automatically(app, id).await;
            });
        }
    }
}

/// Build the refusal a start answers with.
pub(super) fn refused(kind: &str, message: String, dest: &std::path::Path) -> Refusal {
    Refusal {
        kind: kind.to_string(),
        message,
        path: dest.to_string_lossy().to_string(),
        suggested: dest.to_string_lossy().to_string(),
    }
}

/// Tell every window and tray icon what one download is doing.
///
/// The engine reports on its own while bytes arrive; this is the same news
/// for the changes the commands make, so a pause or a queue move is reflected
/// in a progress window's tray tooltip without waiting for the next report.
pub fn announce(app: &AppHandle, download: &Download) {
    let info = download.info();
    let _ = app.emit(UPDATE_EVENT, info.clone());
    crate::progress::refresh_tray(app, &info);
}

/// Hand a download to a task of its own.
///
/// The task claims the run itself, so a stop that lands right after the spawn
/// still covers it. `queue_managed` marks a download the queue started, so the
/// scheduler can start the next one when this run ends.
pub(super) fn spawn_run(
    app: &AppHandle,
    state: &AppState,
    download: Arc<Download>,
    queue_managed: bool,
) {
    start_download_run(app, state, download, queue_managed);
}

/// Start one download run and, when it ends, tell the queue if it was
/// queue-managed so the scheduler can start the next waiting download.
pub(super) fn start_download_run(
    app: &AppHandle,
    state: &AppState,
    download: Arc<Download>,
    queue_managed: bool,
) {
    let limits: Limits = state.limits().clone();
    let reporter = Arc::new(AppReporter(app.clone()));
    let running = download.clone();
    let download_id = running.id;
    let request = running.request_run();
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        idm_core::downloader::run_claimed(reporter, limits, running, request).await;
        if queue_managed {
            let state = app_clone.state::<AppState>();
            state.queue_on_finished(&app_clone, download_id);
        }
    });
}

/// The destination with the name a service gave the file.
///
/// A name the reader chose is kept: only the placeholder of the add dialog
/// is replaced, so an unlock never moves a file the reader has named.
fn named(dest: PathBuf, name: &str) -> PathBuf {
    let current = dest
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    if !current.is_empty() && current != "download.bin" {
        return dest;
    }
    match dest.parent() {
        Some(parent) => parent.join(safe_name(name)),
        None => dest,
    }
}

/// A name a service gave, safe to put in a path.
///
/// The name comes from the outside, so it may hold a path of its own.
fn safe_name(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    let cleaned: String = name
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') && *c != '\0')
        .collect();
    let cleaned = cleaned.trim().trim_end_matches([' ', '.']);
    if cleaned.is_empty() {
        "download.bin".to_string()
    } else {
        cleaned.to_string()
    }
}

/// The connections a download starts with.
///
/// A service that says how many connections may fetch the file has the
/// last word: more than that is refused by the service itself.
fn connections(asked: Option<u32>, allowed: Option<u32>) -> u32 {
    let mut count = asked.unwrap_or(DEFAULT_CONNECTIONS).clamp(1, 32);
    if let Some(allowed) = allowed {
        count = count.min(allowed.max(1));
    }
    count
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
) -> Result<Started, Refusal> {
    let mut dest = dest;
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err(refused("invalid", "a URL is required".to_string(), &dest));
    }
    idm_core::log::write("info", "the download is starting");

    // The link is unlocked before the engine is asked to start it, so the
    // download fetches the direct link and takes the name the service gave.
    // A magnet link becomes a link of the service that takes it, so the
    // scheme is checked after this step and not before it.
    let prepared = match debrid::prepare(&url, state.engine_settings().debrid_auto_unlock) {
        Ok(prepared) => prepared,
        Err(message) => return Err(refused("debrid", message, &dest)),
    };
    // A Drive link is not a file; it points at one or at an editor. The
    // address the engine fetches is the one that carries the bytes.
    let url = match crate::drive::resolve(&url, options.drive_export.as_deref()) {
        Ok(url) => url,
        Err(message) => return Err(refused("drive", message, &dest)),
    };
    let (url, service_connections) = match prepared {
        Some(prepared) => {
            // The name the service gave replaces the placeholder, not a name
            // the reader chose.
            if let Some(name) = prepared.file_name.as_deref() {
                dest = named(dest, name);
            }
            (prepared.url, prepared.max_connections)
        }
        None => (url, None),
    };

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

    // A category with a folder of its own is applied before the destination is
    // checked, so the File Exists question is asked about the file that will
    // really be written.
    let (dest, category) =
        category_destination(state, dest, options.category, options.use_category_folder);

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

    // The application's staging tree is the only place a download may keep
    // its unfinished bytes. Without it the start is refused: the alternative
    // is part files in the folder the user chose for finished files.
    let id = state.next_id();
    let work_dir = state
        .work_dir(id, &dest)
        .ok_or_else(|| refused("storage", THE_STORAGE_IS_NOT_READY.to_string(), &dest))?;
    let download = Download::new_in(
        id,
        url,
        dest.clone(),
        Some(&work_dir),
        connections(options.connections, service_connections),
        state.limits().proxy().as_deref(),
        options.referer.as_deref(),
    )
    .map_err(|message| refused("invalid", message, &dest))?;
    download.set_meta(category, options.description);
    download.set_allow_replace(replace);

    state.insert(download.clone());
    state.save_index();

    // A download that was added to the queue waits for Start Queue. The row
    // is there at once, in the state the queue expects, and no task is
    // spawned: the queue starts it the same way a paused one is started.
    if options.queue {
        announce(app, &download);
        crate::tray::refresh(app);
        return Ok(Started {
            id: Some(id),
            dest: dest.to_string_lossy().to_string(),
            skipped: false,
        });
    }

    spawn_run(app, state, download, false);

    Ok(Started {
        id: Some(id),
        dest: dest.to_string_lossy().to_string(),
        skipped: false,
    })
}

/// The destination a category gives a start, and the category it belongs to.
///
/// Only a start that did not name a folder of its own is moved: the add dialog
/// clears the flag when the user picked one. A category without a folder of
/// its own saves in a folder of its name inside the folder the start came
/// with, which for a hand-over is the default save folder. When no category
/// was supplied, the file name decides, so a hand-over from the browser lands
/// with the rest of its type. The bridge asks this the same question before it
/// answers the extension, so a conflict is found on the file the start would
/// really write.
pub(crate) fn category_destination(
    state: &AppState,
    dest: PathBuf,
    category: Option<String>,
    apply: bool,
) -> (PathBuf, Option<String>) {
    if !apply {
        return (dest, category);
    }
    let category = category.or_else(|| state.detect_category(&dest));
    let Some(folder) = category
        .as_deref()
        .and_then(|name| state.category_folder_in(name, dest.parent()))
    else {
        return (dest, category);
    };
    match dest.file_name() {
        Some(name) => (folder.join(name), category),
        None => (dest, category),
    }
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
    announce(app, &download);
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
    announce(app, &download);
    state.save_index();
    Ok(())
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
    resume(app, state, id, false)
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
    let fresh_id = state.next_id();
    let work_dir = state
        .work_dir(fresh_id, &dest)
        .ok_or_else(|| refused("storage", THE_STORAGE_IS_NOT_READY.to_string(), &dest))?;
    let fresh = Download::new_in(
        fresh_id,
        previous.url.clone(),
        dest.clone(),
        Some(&work_dir),
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
    announce(app, &fresh);
    spawn_run(app, state, fresh, false);

    Ok(Started {
        id: Some(new_id),
        dest: dest.to_string_lossy().to_string(),
        skipped: false,
    })
}

#[cfg(test)]
mod tests;
