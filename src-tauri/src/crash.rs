//! The crash monitor and the freeze detector of the application.
//!
//! The application starts a second copy of itself with the monitor flag. That
//! copy waits for the application to end and writes a report when it ends
//! badly. The application itself only touches its heartbeat, so a program
//! that is already falling over is never asked to write anything.

use std::path::PathBuf;

use tauri::Manager;

/// The crate the monitor is built from.
pub use idm_crash::{Heartbeat, BEAT};

/// The module that watches the application.
pub use idm_crash::monitor::{Monitor, FLAG};

/// The folder the monitor watches, when this copy of the application is the
/// monitor.
///
/// The folder is the one the flag carries, so the monitor knows where the
/// heartbeat and the log are.
pub fn monitor_folder() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == FLAG {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

/// Start the monitor and the heartbeat of the application.
///
/// The monitor is a second copy of the application: it waits outside and
/// writes the report, so a crash in the application cannot take the report
/// with it.
pub fn start(folder: PathBuf) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    // The monitor is a second copy of the application: it is told what to
    // watch and what to do, and then it is left alone.
    let Ok(_child) = std::process::Command::new(exe)
        .arg(FLAG)
        .arg(&folder)
        .spawn()
    else {
        return;
    };
    let heartbeat = Heartbeat::at(&folder);
    heartbeat.beat();
    std::thread::spawn(move || loop {
        heartbeat.beat();
        std::thread::sleep(BEAT);
    });
}

/// The file the application writes when it ends in an orderly way, so the
/// monitor knows that the end was not a crash.
pub fn say_goodbye(folder: &std::path::Path) {
    let _ = std::fs::write(folder.join("goodbye.txt"), "");
}

/// Watch the application from outside, and write a report when it ends badly.
pub fn watch(folder: PathBuf) {
    let monitor = Monitor::new(&folder, env!("CARGO_PKG_VERSION"));
    monitor.watch(std::process::id());
}

/// The report of the last time the window stopped answering.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreezeReport {
    /// Where the report and the stacks of the stopped program are.
    pub folder: String,
    /// The report as it was written.
    pub text: String,
    /// When the report was written, in seconds since the epoch.
    pub written_at: u64,
}

/// Read the report of the last freeze, when there was one.
///
/// The monitor writes it beside the log, so the reader is told on the next
/// start that the window stopped answering, and where the stacks are.
#[tauri::command]
pub fn last_freeze_report(app: tauri::AppHandle) -> Option<FreezeReport> {
    let folder = app.path().app_log_dir().ok()?;
    let path = folder.join("freeze-report.md");
    let text = std::fs::read_to_string(&path).ok()?;
    let written_at = std::fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|moment| moment.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_secs())
        .unwrap_or(0);
    Some(FreezeReport {
        folder: folder.to_string_lossy().to_string(),
        text,
        written_at,
    })
}
