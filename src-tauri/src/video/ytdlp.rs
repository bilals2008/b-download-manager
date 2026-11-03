//! The optional `yt-dlp` last resort.
//!
//! The default YouTube path is URL reuse: the page's player already asked for
//! a signed `videoplayback` address, and the engine downloads that address as
//! a plain file. When that address is refused with `403`, the page has changed
//! its signature and the app cannot re-sign it (it never will: no cipher
//! solver, no `n` solver). If a `yt-dlp` binary is present, this module asks it
//! for the same video once; if it is not present, the download fails with one
//! sentence and nothing is downloaded silently.
//!
//! The binary is located by [`super::tools`]: `BDM_YTDLP`, a sidecar beside the
//! executable, or `PATH`. There is no download of the tool itself.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use idm_core::disk;
use idm_core::log;

use super::tools::{self, Tool};

/// How long the fallback may run.
pub const TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YtDlpError {
    NotFound,
    Failed(Option<i32>),
    TimedOut,
    Cancelled,
    Io(String),
}

impl std::fmt::Display for YtDlpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            YtDlpError::NotFound => write!(f, "yt-dlp was not found"),
            YtDlpError::Failed(code) => {
                write!(f, "yt-dlp could not download the video (exit {code:?})")
            }
            YtDlpError::TimedOut => write!(f, "yt-dlp took too long and was stopped"),
            YtDlpError::Cancelled => write!(f, "yt-dlp was stopped"),
            YtDlpError::Io(message) => write!(f, "yt-dlp could not be run: {message}"),
        }
    }
}

impl std::error::Error for YtDlpError {}

/// The outputs whose fallback was stopped, scoped to the destination.
static CANCEL: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

pub fn cancel(output: &Path) {
    let mut cancelled = CANCEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !cancelled.iter().any(|path| path == output) {
        cancelled.push(output.to_path_buf());
    }
}

fn is_cancelled(output: &Path) -> bool {
    CANCEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .any(|path| path == output)
}

fn clear_cancel(output: &Path) {
    CANCEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(|path| path != output);
}

struct CancelFlag<'a>(&'a Path);

impl Drop for CancelFlag<'_> {
    fn drop(&mut self) {
        clear_cancel(self.0);
    }
}

/// Whether a binary this module can use is present.
pub fn available() -> bool {
    Tool::YtDlp.available()
}

/// Download `page_url` with the chosen format, into `output`.
///
/// `selector` is a yt-dlp format expression (`137+bestaudio/137/best`), or
/// `None` for the tool's own choice. The tool writes into `work_dir`, the
/// job's own staging folder, and the finished file is moved into place only
/// after the tool succeeded — so nothing of the tool's making ever appears
/// in the folder the user chose.
pub fn download(
    page_url: &str,
    selector: Option<&str>,
    output: &Path,
    work_dir: &Path,
) -> Result<(), YtDlpError> {
    if !page_url.starts_with("http://") && !page_url.starts_with("https://") {
        return Err(YtDlpError::Io(
            "the page address is not a web address".to_string(),
        ));
    }
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    clear_cancel(output);
    let _cancel_flag = CancelFlag(output);

    let tool = Tool::YtDlp.locate().ok_or(YtDlpError::NotFound)?;
    std::fs::create_dir_all(work_dir).map_err(|error| YtDlpError::Io(error.to_string()))?;

    let stem = output
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    let prefix = format!("ytdlp-{stem}");
    cleanup(work_dir, &prefix);

    let pattern = work_dir.join(format!("{prefix}.%(ext)s"));
    let mut command = tools::command_for(&tool);
    command
        .arg("--no-playlist")
        .arg("--no-progress")
        .arg("--no-warnings")
        .arg("--newline")
        .arg("--merge-output-format")
        .arg("mp4/mkv")
        .arg("-o")
        .arg(&pattern);
    if let Some(selector) = selector.filter(|selector| !selector.trim().is_empty()) {
        command.arg("-f").arg(selector);
    }
    command.arg(page_url);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|error| YtDlpError::Io(error.to_string()))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    cleanup(work_dir, &prefix);
                    let code = status.code();
                    log::write("warn", &format!("yt-dlp refused the video (exit {code:?})"));
                    return Err(YtDlpError::Failed(code));
                }
                let produced = find_output(work_dir, &prefix);
                let Some(produced) = produced else {
                    cleanup(work_dir, &prefix);
                    return Err(YtDlpError::Io("yt-dlp finished without a file".to_string()));
                };
                let result = disk::replace_file(&produced, output)
                    .map_err(|error| YtDlpError::Io(error.to_string()));
                cleanup(work_dir, &prefix);
                result?;
                log::write("info", &format!("yt-dlp saved {}", output.display()));
                return Ok(());
            }
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                cleanup(work_dir, &prefix);
                return Err(YtDlpError::Io(error.to_string()));
            }
        }
        if is_cancelled(output) {
            let _ = child.kill();
            let _ = child.wait();
            cleanup(work_dir, &prefix);
            log::write("info", "yt-dlp was stopped");
            return Err(YtDlpError::Cancelled);
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            cleanup(work_dir, &prefix);
            log::write("warn", "yt-dlp timed out and was stopped");
            return Err(YtDlpError::TimedOut);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn cleanup(work_dir: &Path, prefix: &str) {
    let Ok(entries) = std::fs::read_dir(work_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(prefix) && name != prefix {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn find_output(work_dir: &Path, prefix: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(work_dir).ok()?;
    let mut found: Option<(u64, PathBuf)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(prefix) || name.ends_with(".part") {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        if found.as_ref().map_or(true, |(size, _)| meta.len() > *size) {
            found = Some((meta.len(), path));
        }
    }
    found.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    // Every test here drives a .cmd stand-in, so the whole module is Windows
    // only and the import would be unused on Linux.
    #[cfg(windows)]
    use super::*;

    // The stand-in tool is a .cmd script, so these three helpers belong to
    // the Windows tests only; gating them here keeps the Linux clippy leg
    // free of dead-code warnings.
    #[cfg(windows)]
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[cfg(windows)]
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(windows)]
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bdm-ytdlp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        dir
    }

    #[cfg(windows)]
    fn fake(tag: &str, body: &str, dir: &Path) -> PathBuf {
        let path = dir.join(format!("{tag}.cmd"));
        std::fs::write(&path, format!("@echo off\r\n{body}\r\n")).expect("the fake");
        path
    }

    #[cfg(windows)]
    #[test]
    fn a_finished_fallback_moves_the_file_into_place() {
        let _guard = lock();
        let dir = temp_dir("ok");
        // The tool writes into the staging folder; the destination folder only
        // ever receives the finished file.
        let work = dir.join("staging");
        std::fs::create_dir_all(&work).expect("the staging folder");
        std::env::set_var("BDM_YTDLP_OUT", &work);
        let tool = fake(
            "yt-dlp",
            "> \"%BDM_YTDLP_OUT%\\ytdlp-film.mp4\" echo YTDLP",
            &dir,
        );
        std::env::set_var("BDM_YTDLP", &tool);

        let output = dir.join("film.mp4");
        download(
            "https://www.youtube.com/watch?v=abc",
            Some("137+bestaudio/137/best"),
            &output,
            &work,
        )
        .expect("a download");
        assert_eq!(std::fs::read(&output).expect("the file"), b"YTDLP\r\n");
        let leftovers: Vec<String> = std::fs::read_dir(&work)
            .expect("the folder")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with("ytdlp-film"))
            .collect();
        assert!(leftovers.is_empty(), "the pieces are gone: {leftovers:?}");

        std::env::remove_var("BDM_YTDLP");
        std::env::remove_var("BDM_YTDLP_OUT");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_refusing_tool_leaves_no_file() {
        let _guard = lock();
        let dir = temp_dir("fail");
        let tool = fake("yt-dlp", "exit /b 4", &dir);
        std::env::set_var("BDM_YTDLP", &tool);
        let output = dir.join("film.mp4");
        let error = download("https://www.youtube.com/watch?v=abc", None, &output, &dir)
            .expect_err("a refusal");
        assert_eq!(error, YtDlpError::Failed(Some(4)));
        assert!(!output.exists());

        std::env::remove_var("BDM_YTDLP");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_cancelled_fallback_is_killed() {
        let _guard = lock();
        let dir = temp_dir("cancel");
        let work = dir.join("staging");
        std::fs::create_dir_all(&work).expect("the staging folder");
        std::env::set_var("BDM_YTDLP_OUT", &work);
        let tool = fake(
            "yt-dlp",
            "ping -n 20 127.0.0.1 >nul\r\n> \"%BDM_YTDLP_OUT%\\ytdlp-film.mp4\" echo YTDLP",
            &dir,
        );
        std::env::set_var("BDM_YTDLP", &tool);
        let output = dir.join("film.mp4");
        let output_for_thread = output.clone();
        let work_for_thread = work.clone();
        let handle = std::thread::spawn(move || {
            download(
                "https://www.youtube.com/watch?v=abc",
                None,
                &output_for_thread,
                &work_for_thread,
            )
        });
        std::thread::sleep(Duration::from_millis(300));
        cancel(&output);
        let error = handle.join().expect("the thread").expect_err("cancelled");
        assert_eq!(error, YtDlpError::Cancelled);
        assert!(!output.exists());

        std::env::remove_var("BDM_YTDLP");
        std::env::remove_var("BDM_YTDLP_OUT");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
