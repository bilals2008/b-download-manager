//! Unpacking a finished archive.
//!
//! The archive crate does the work and owns the safety rules; this is the
//! window's side of it. The contents land in a folder named after the archive,
//! beside it, and the archive itself is left where it is: deleting it is not
//! this command's decision.
//!
//! The command is `async` on purpose. A large archive takes minutes, and a
//! command that runs on the main thread would hold the window toolkit for all
//! of it — the application would look frozen while it unpacked.

use std::path::{Path, PathBuf};

use idm_archive::{find_7zip, Limits, Policy, Subfolder};
use idm_core::downloader::Status;
use tauri::{Emitter, State};

use crate::state::AppState;

/// What unpacking a finished archive put in place, plus the outcome the
/// completion window shows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractReport {
    /// The folder the contents went into.
    pub folder: String,
    /// How many files were put in place.
    pub files: u64,
    /// How many bytes were written.
    pub bytes: u64,
    /// How many entries were left out (a link, a folder that goes outside).
    pub skipped: u64,
    /// How many entries were refused because their name would leave the
    /// folder. A non-zero count is worth saying out loud.
    pub refused: u64,
    /// How many files landed under another name, one of that name being
    /// there already.
    pub renamed: u64,
}

/// The outcome of an unpack, as it is reported to the window.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractOutcome {
    /// The download the archive belongs to.
    pub id: u64,
    /// The folder the contents went into.
    pub folder: String,
    /// How many files were put in place.
    pub files: u64,
    /// How many bytes were written.
    pub bytes: u64,
    /// How many entries were left out.
    pub skipped: u64,
    /// How many entries were refused.
    pub refused: u64,
    /// How many name collisions were renamed.
    pub renamed: u64,
}

/// The archive waiting for its password, if any.
///
/// The password itself is never kept: it is used for the one unpack it was
/// given for and then dropped.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractPending {
    /// The download the archive belongs to.
    pub id: u64,
    /// The file the reader sees.
    pub name: String,
}

/// Why an unpack did not finish, in the shape the caller can act on.
#[derive(Debug)]
pub(crate) enum ExtractFailure {
    /// It is encrypted and no password was given. The window asks for one.
    Password,
    /// Something else went wrong, in words the log can keep.
    Other(String),
}

impl std::fmt::Display for ExtractFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractFailure::Password => write!(f, "it is encrypted and needs a password"),
            ExtractFailure::Other(reason) => write!(f, "{reason}"),
        }
    }
}


/// A file count the caller may want while an archive unpacks.
///
/// The complete window draws a bar for a large archive from this.
pub(crate) type ProgressHook = Box<dyn FnMut(u64) + Send>;

/// Why an unpack could not be done, as the window shows it.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractError {
    /// The download the archive belongs to.
    pub id: u64,
    /// What went wrong, in words.
    pub message: String,
}

/// The progress of an unpack, as the window shows it.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractProgress {
    /// The download the archive belongs to.
    pub id: u64,
    /// How many files are in place so far.
    pub files: u64,
}

/// Unpack the finished download `id`, returning where its contents went.
#[tauri::command(async)]
pub fn extract_download(state: State<'_, AppState>, id: u64) -> Result<ExtractReport, String> {
    let download = state.get(id).ok_or("unknown download")?;
    let info = download.info();
    if info.status != Status::Completed {
        return Err("only a finished file can be unpacked".to_string());
    }
    let report = unpack_with(Path::new(&info.dest), None, None).map_err(|failure| failure.to_string())?;
    Ok(ExtractReport {
        folder: report.folder.to_string_lossy().to_string(),
        files: report.files,
        bytes: report.bytes,
        skipped: report.skipped.len() as u64,
        refused: report.refused,
        renamed: report.renamed,
    })
}

/// The archive waiting for its password, if any.
///
/// The password itself is never kept: it is used for the one unpack it was
/// given for and then dropped.
fn pending() -> std::sync::MutexGuard<'static, Option<ExtractPending>> {
    static PENDING: std::sync::Mutex<Option<ExtractPending>> = std::sync::Mutex::new(None);
    PENDING.lock().expect("pending extract lock poisoned")
}

/// The archive waiting for its password, if any.
#[tauri::command]
pub fn extract_pending() -> Option<ExtractPending> {
    pending().clone()
}

/// Unpack the archive of download `id` with the password the reader typed.
///
/// The password is used once and is not stored anywhere, not in the settings
/// and not in this process beyond the unpack it is let into.
#[tauri::command(async)]
pub fn extract_download_with_password(
    state: State<'_, AppState>,
    id: u64,
    password: String,
) -> Result<ExtractReport, String> {
    let download = state.get(id).ok_or("unknown download")?;
    let info = download.info();
    if info.status != Status::Completed {
        return Err("only a finished file can be unpacked".to_string());
    }
    let report = unpack_with(Path::new(&info.dest), Some(password.trim()), None)
        .map_err(|failure| failure.to_string())?;
    {
        let mut held = pending();
        if held.as_ref().is_some_and(|pending| pending.id == id) {
            *held = None;
        }
    }
    Ok(ExtractReport {
        folder: report.folder.to_string_lossy().to_string(),
        files: report.files,
        bytes: report.bytes,
        skipped: report.skipped.len() as u64,
        refused: report.refused,
        renamed: report.renamed,
    })
}

/// Unpack one archive beside itself into a folder of its own name, without a
/// password and without a progress callback.
#[cfg(test)]
pub(crate) fn unpack(archive: &Path) -> Result<idm_archive::Report, String> {
    unpack_with(archive, None, None).map_err(|failure| failure.to_string())
}

/// Unpack one archive, with the password and the per-file count when the
/// caller has them.
pub(crate) fn unpack_with(
    archive: &Path,
    password: Option<&str>,
    progress: Option<ProgressHook>,
) -> Result<idm_archive::Report, ExtractFailure> {
    if !archive.exists() {
        return Err(ExtractFailure::Other(format!(
            "{} is not there",
            archive.display()
        )));
    }
    let size = std::fs::metadata(archive).map(|meta| meta.len()).unwrap_or(0);
    let dest = archive
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let mut count = 0u64;
    let mut progress = progress;
    let mut on_file = move |_path: &Path| {
        count += 1;
        if let Some(hook) = progress.as_mut() {
            hook(count);
        }
    };
    let mut policy = Policy {
        dest,
        subfolder: Subfolder::Always,
        password: password.map(str::to_string),
        limits: Limits::for_archive(size),
        tool: find_7zip(),
        on_file: &mut on_file,
        cancel: None,
    };
    idm_archive::extract(archive, &mut policy).map_err(|error| match error {
        idm_archive::Error::NeedsPassword => ExtractFailure::Password,
        other => ExtractFailure::Other(other.to_string()),
    })
}

/// Unpack a finished archive when the reader asked for it, and remove the
/// archive afterwards when a clean unpack proves the contents are complete.
///
/// Runs off the reporting thread: unpacking is minutes of disk work, and the
/// engine reports a download's state on the thread that must not wait for it.
/// A row is unpacked once per run: the engine may report the same completion
/// more than once, and a second pass would only rewrite the same files.
pub(crate) async fn extract_automatically(app: tauri::AppHandle, id: u64) {
    use tauri::Manager;
    use std::collections::HashSet;
    use std::sync::Mutex;

    static UNPACKED: Mutex<Option<HashSet<u64>>> = Mutex::new(None);

    let state = app.state::<crate::state::AppState>();
    let settings = state.engine_settings();
    if !settings.extract_after_done {
        return;
    }
    {
        let mut seen = UNPACKED.lock().expect("unpacked set lock poisoned");
        let ids = seen.get_or_insert_with(HashSet::new);
        if !ids.insert(id) {
            return;
        }
    }
    let Some(download) = state.get(id) else {
        return;
    };
    let info = download.info();
    if info.status != Status::Completed {
        return;
    }
    let archive = PathBuf::from(&info.dest);
    if !idm_archive::is_archive(&archive) {
        return;
    }

    // The completion window says what is happening to a large archive, so a
    // hundred thousand files do not look like a stall. Every tenth file is
    // one event; a whole archive is a few thousand numbers, not a flood.
    let app_for_progress = app.clone();
    let progress: ProgressHook = Box::new(move |files| {
        if files % 10 == 0 {
            let _ = app_for_progress.emit("extract://progress", ExtractProgress { id, files });
        }
    });
    let target = archive.clone();
    let unpacked =
        tauri::async_runtime::spawn_blocking(move || unpack_with(&target, None, Some(progress)))
            .await;
    let report = match unpacked {
        Ok(Ok(report)) => report,
        Ok(Err(ExtractFailure::Password)) => {
            // Hold the archive: the completion window asks for its password,
            // and nothing is written until the reader answers.
            let name = info
                .dest
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&info.dest)
                .to_string();
            {
                *pending() = Some(ExtractPending {
                    id,
                    name: name.clone(),
                });
            }
            idm_core::log::write(
                "info",
                &format!("{} is encrypted; the window asks for its password", info.dest),
            );
            let _ = app.emit(
                "extract://needs-password",
                ExtractPending {
                    id,
                    name,
                },
            );
            return;
        }
        Ok(Err(ExtractFailure::Other(error))) => {
            idm_core::log::write(
                "warn",
                &format!("{} could not be unpacked: {error}", info.dest),
            );
            let _ = app.emit(
                "extract://failed",
                ExtractError { id, message: error },
            );
            return;
        }
        Err(_) => return,
    };
    idm_core::log::write(
        "info",
        &format!(
            "unpacked {} into {} ({} files)",
            info.dest,
            report.folder.display(),
            report.files
        ),
    );
    let _ = app.emit(
        "extract://outcome",
        ExtractOutcome {
            id,
            folder: report.folder.to_string_lossy().to_string(),
            files: report.files,
            bytes: report.bytes,
            skipped: report.skipped.len() as u64,
            refused: report.refused,
            renamed: report.renamed,
        },
    );

    // Only a clean unpack removes the archive: a refused entry means the
    // extracted folder is not what the archive held, and the archive is the
    // only copy left.
    if !settings.delete_archive_after_extract || report.refused > 0 || report.files == 0 {
        return;
    }
    if std::fs::remove_file(&archive).is_err() {
        return;
    }
    state.remove(id);
    state.save_index();
    crate::progress::close(&app, id);
    crate::complete::close(&app, id);
    crate::tray::refresh(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A file of the given name and contents, in a folder of its own.
    fn a_file(name: &str, contents: &[u8]) -> PathBuf {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let folder = std::env::temp_dir().join(format!(
            "bdm-extract-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&folder).expect("the folder is made");
        let path = folder.join(name);
        std::fs::write(&path, contents).expect("the file is written");
        path
    }

    #[test]
    fn an_archive_that_is_not_there_is_refused() {
        let error = unpack(Path::new("there-is-no-such-archive.zip"))
            .expect_err("there is no archive");
        assert!(error.contains("not there"), "{error}");
    }

    #[test]
    fn a_file_that_is_not_an_archive_is_refused() {
        // The name claims a zip; the bytes are prose. The crate reads the
        // format, not the name, so this is refused rather than unpacked.
        let path = a_file("notes.zip", b"this is not an archive");
        let error = unpack(&path).expect_err("the bytes are not a zip");
        assert!(!error.is_empty(), "a reason is given: {error}");
        let _ = std::fs::remove_dir_all(path.parent().expect("a folder"));
    }
}
