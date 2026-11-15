//! The crash monitor and the freeze detector.
//!
//! A second process waits beside the application and writes down what
//! happened when it dies. The application never writes its own dump: a
//! program that is already falling over cannot be trusted to write one.
//!
//! The monitor watches two things: the process itself, and a heartbeat file
//! the application touches while it is alive. A process that is gone without
//! having said goodbye crashed; a process that is there but has stopped
//! touching its heartbeat is frozen.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub mod hang;
pub mod monitor;
pub mod sample;

pub use monitor::{FLAG, Monitor};

/// How long the application may be quiet before it is called frozen.
pub const FREEZE_AFTER: Duration = Duration::from_secs(20);

/// How often the application touches its heartbeat.
pub const BEAT: Duration = Duration::from_secs(2);

/// The heartbeat file: the moment the application was last alive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heartbeat {
    /// Where the file is.
    pub path: PathBuf,
}

impl Heartbeat {
    /// The heartbeat of the application, in the folder it keeps its data.
    pub fn at(folder: &Path) -> Self {
        Heartbeat {
            path: folder.join("alive.txt"),
        }
    }

    /// Say that the application is alive.
    pub fn beat(&self) {
        let _ = std::fs::write(&self.path, clock());
    }

    /// The moment the application was last alive, when the file says so.
    pub fn last(&self) -> Option<Instant> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        let seconds: u64 = text.trim().parse().ok()?;
        // The file holds the moment of the beat in seconds since the epoch, so
        // the quiet time is the difference between it and now.
        let quiet = now_seconds().saturating_sub(seconds);
        Some(Instant::now() - Duration::from_secs(quiet))
    }

    /// How long the application has been quiet, when it has beaten at all.
    pub fn quiet_for(&self) -> Option<Duration> {
        self.last().map(|last| last.elapsed())
    }

    /// Whether the application has been quiet for longer than it should.
    pub fn is_frozen(&self) -> bool {
        match self.last() {
            Some(last) => last.elapsed() > FREEZE_AFTER,
            None => false,
        }
    }

    /// Forget the beat, so a heartbeat that was never written does not look
    /// like one that is old.
    pub fn forget(&self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// What the monitor found out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// The application ended on its own.
    Exited {
        /// The code it ended with.
        code: i32,
    },
    /// The application stopped answering.
    Frozen {
        /// How long it had been quiet.
        quiet_for: Duration,
    },
    /// The application died without saying goodbye.
    Crashed {
        /// The code it died with.
        code: i32,
    },
}

/// What the monitor writes down when something happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// What happened.
    pub finding: Finding,
    /// The version of the application.
    pub version: String,
    /// The last lines the application wrote to its log.
    pub log_tail: Vec<String>,
    /// The threads of the process, when it was sampled.
    pub threads: Vec<String>,
    /// The file the stacks were written to, when they could be written.
    pub dump: Option<PathBuf>,
}

impl Report {
    /// A report with nothing extra in it.
    pub fn new(finding: Finding, version: String) -> Self {
        Self {
            finding,
            version,
            log_tail: Vec::new(),
            threads: Vec::new(),
            dump: None,
        }
    }

    /// The report as the text that is written to the file.
    pub fn text(&self) -> String {
        let mut text = format!("# B Download Manager crash report\n\n");
        text.push_str(&format!("Version: {}\n", self.version));
        text.push_str(&format!("What happened: {}\n\n", describe(&self.finding)));
        if let Some(dump) = &self.dump {
            text.push_str(&format!(
                "The stacks of the stopped program are in {}.\n\n",
                dump.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default()
            ));
        }
        if !self.threads.is_empty() {
            text.push_str("## The threads of the program\n\n```\n");
            for line in &self.threads {
                text.push_str(line);
                text.push('\n');
            }
            text.push_str("```\n\n");
        }
        if !self.log_tail.is_empty() {
            text.push_str("## The last lines of the log\n\n```\n");
            for line in &self.log_tail {
                text.push_str(line);
                text.push('\n');
            }
            text.push_str("```\n");
        }
        text
    }
}

/// The sentence that says what happened.
pub fn describe(finding: &Finding) -> String {
    match finding {
        Finding::Exited { code } => format!("the application ended on its own (code {code})"),
        Finding::Frozen { quiet_for } => {
            format!("the application stopped answering for {} seconds", quiet_for.as_secs())
        }
        Finding::Crashed { code } => format!("the application died without warning (code {code})"),
    }
}

/// The last lines of a log file.
pub fn log_tail(path: &Path, lines: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].iter().map(|line| line.to_string()).collect()
}

/// The moment the application was last alive, in seconds since the epoch.
fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// The moment the application was last alive, in seconds since the epoch.
fn clock() -> String {
    now_seconds().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bdm-crash-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        dir
    }

    #[test]
    fn a_beat_says_the_application_is_alive() {
        let dir = folder("beat");
        let beat = Heartbeat::at(&dir);
        beat.beat();
        assert!(!beat.is_frozen(), "a fresh beat is not a frozen application");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_beat_that_is_old_means_a_frozen_application() {
        let dir = folder("old");
        let beat = Heartbeat::at(&dir);
        // A beat from well before the freeze limit.
        let old = now_seconds().saturating_sub(FREEZE_AFTER.as_secs() + 10);
        std::fs::write(&beat.path, old.to_string()).expect("a beat");
        assert!(beat.is_frozen(), "an old beat means a frozen application");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_heartbeat_that_was_never_written_is_not_a_frozen_application() {
        let dir = folder("none");
        let beat = Heartbeat::at(&dir);
        assert!(!beat.is_frozen());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_report_says_what_happened() {
        let mut report = Report::new(
            Finding::Crashed { code: -1073741819 },
            "1.1.1".to_string(),
        );
        report.log_tail = vec!["12:00:00 [info] the application is starting".to_string()];
        let text = report.text();
        assert!(text.contains("1.1.1"));
        assert!(text.contains("died without warning"));
        assert!(text.contains("the application is starting"));
    }

    #[test]
    fn a_report_of_a_freeze_says_so_and_shows_the_threads() {
        let mut report = Report::new(
            Finding::Frozen {
                quiet_for: Duration::from_secs(45),
            },
            "1.1.1".to_string(),
        );
        report.threads = vec!["thread    12 used 3.0s of processor time".to_string()];
        let text = report.text();
        assert!(text.contains("stopped answering"), "{text}");
        assert!(text.contains("45 seconds"), "{text}");
        assert!(text.contains("thread    12"), "{text}");
    }

    #[test]
    fn the_last_lines_of_a_log_are_the_ones_that_matter() {
        let dir = folder("log");
        let log = dir.join("app.log");
        std::fs::write(&log, "one\ntwo\nthree\nfour\n").expect("a log");
        let tail = log_tail(&log, 2);
        assert_eq!(tail, vec!["three".to_string(), "four".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_log_that_is_not_there_has_no_tail() {
        let dir = folder("nolog");
        assert!(log_tail(&dir.join("nothere.log"), 5).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
