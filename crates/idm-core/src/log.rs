//! A diagnostics log.
//!
//! IDM keeps a log of what its downloads did. The clone writes one line per
//! event in the application data directory, so a download that went wrong can
//! be looked at after the fact.
//!
//! The log is opened once, when the application starts. Before that, and
//! when the file cannot be written, the calls do nothing.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// The file the log is written to.
static LOG: OnceLock<PathBuf> = OnceLock::new();

/// Open the log. The first call wins.
pub fn open(path: PathBuf) {
    let _ = LOG.set(path);
}

/// Write one line to the log.
pub fn write(level: &str, message: &str) {
    let Some(path) = LOG.get() else { return };

    let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{}", line(level, message));
}

/// One line of the log.
fn line(level: &str, message: &str) -> String {
    format!("{} [{level}] {message}", clock())
}

/// The time of day, as `HH:MM:SS`.
fn clock() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_carries_the_time_and_the_level() {
        let text = line("warn", "the disk is full");
        assert!(text.ends_with("[warn] the disk is full"), "line was: {text}");
    }

    #[test]
    fn the_clock_has_the_shape_of_a_time() {
        let time = clock();
        assert_eq!(time.len(), 8, "the time was: {time}");
        assert_eq!(time.as_bytes()[2], b':', "the time was: {time}");
    }

    #[test]
    fn writing_without_a_log_does_nothing() {
        // The log of the process may or may not be open by now; the call
        // must not fail either way.
        write("info", "a line that may go nowhere");
    }
}
