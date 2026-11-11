//! The monitor process.
//!
//! The application starts a second copy of itself with the `--crash-monitor`
//! flag. That copy does not open a window: it waits for the application to
//! end, and when it ends badly it writes down what happened.
//!
//! The monitor never writes the application's own files. It writes its report
//! beside them, in its own folder.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{Finding, Heartbeat, Report, log_tail};

/// The flag that turns a copy of the application into the monitor.
pub const FLAG: &str = "--crash-monitor";

/// How often the monitor looks at the application.
const LOOK: Duration = Duration::from_secs(1);

/// The monitor of one application.
pub struct Monitor {
    /// The heartbeat of the application it watches.
    heartbeat: Heartbeat,
    /// The file the application writes when it ends in an orderly way.
    goodbye: PathBuf,
    /// The log of the application.
    log: PathBuf,
    /// Where the report is written.
    report: PathBuf,
    /// The version of the application.
    version: String,
}

impl Monitor {
    /// The monitor of the application whose data is in `folder`.
    pub fn new(folder: &Path, version: &str) -> Self {
        Monitor {
            heartbeat: Heartbeat::at(folder),
            goodbye: folder.join("goodbye.txt"),
            log: folder.join("idm-clone.log"),
            report: folder.join("crash-report.md"),
            version: version.to_owned(),
        }
    }

    /// Where the report is written.
    pub fn report_path(&self) -> &Path {
        &self.report
    }

    /// Watch the application until it ends, and write a report when it ends
    /// badly.
    ///
    /// The call returns when the application is gone.
    pub fn watch(&self, pid: u32) {
        let mut episodes = crate::hang::Episodes::new();
        loop {
            if !alive(pid) {
                break;
            }
            // A quiet heartbeat is asked one more question: does the window
            // still answer? A machine that went to sleep looks quiet and
            // answers at once; a stuck message loop does not answer at all.
            match episodes.observe(self.heartbeat.quiet_for(), crate::hang::ask(pid)) {
                crate::hang::Episode::Frozen { quiet_for } => {
                    self.write(Finding::Frozen { quiet_for });
                }
                crate::hang::Episode::Recovered | crate::hang::Episode::Quiet => {}
            }
            std::thread::sleep(LOOK);
        }
        // The application said goodbye when it ended in an orderly way.
        if self.goodbye.exists() {
            return;
        }
        self.write(Finding::Crashed { code: exit_code(pid) });
    }

    /// Write the report down.
    fn write(&self, finding: Finding) {
        let report = Report {
            finding,
            version: self.version.clone(),
            log_tail: log_tail(&self.log, 40),
        };
        let _ = std::fs::write(&self.report, report.text());
    }
}

/// Whether the process is still there.
#[cfg(target_os = "windows")]
fn alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    /// The right to wait for a process to end.
    const SYNCHRONIZE: u32 = 0x0010_0000;
    unsafe {
        // The handle must be able to wait: asking only to read the exit code
        // would leave the wait without the right to be done.
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
            0,
            pid,
        );
        if handle.is_null() {
            // The process is gone: OpenProcess could not open it.
            return false;
        }
        let waited = WaitForSingleObject(handle, 0);
        let mut code = 0u32;
        let _ = GetExitCodeProcess(handle, &mut code);
        let _ = CloseHandle(handle);
        waited == WAIT_TIMEOUT
    }
}

/// Whether the process is still there.
///
/// Without the platform's process API the heartbeat is the only sign of life,
/// so a process that has stopped beating is taken for one that is gone.
#[cfg(not(target_os = "windows"))]
fn alive(pid: u32) -> bool {
    let _ = pid;
    true
}

/// The code the process ended with.
#[cfg(target_os = "windows")]
fn exit_code(pid: u32) -> i32 {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return 0;
        }
        let mut code = 0u32;
        let _ = GetExitCodeProcess(handle, &mut code);
        let _ = CloseHandle(handle);
        code as i32
    }
}

/// The code the process ended with.
#[cfg(not(target_os = "windows"))]
fn exit_code(_pid: u32) -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_monitor_knows_where_its_report_goes() {
        let dir = std::env::temp_dir().join(format!("bdm-crash-monitor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        let monitor = Monitor::new(&dir, "1.1.1");
        assert!(monitor.report_path().ends_with("crash-report.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_process_that_runs_is_alive() {
        assert!(alive(std::process::id()));
    }
}
