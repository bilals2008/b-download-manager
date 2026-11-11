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
use tauri::State;

use crate::state::AppState;

/// What unpacking a finished archive put in place.
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
}

/// Unpack the finished download `id`, returning where its contents went.
#[tauri::command(async)]
pub fn extract_download(state: State<'_, AppState>, id: u64) -> Result<ExtractReport, String> {
    let download = state.get(id).ok_or("unknown download")?;
    let info = download.info();
    if info.status != Status::Completed {
        return Err("only a finished file can be unpacked".to_string());
    }
    let report = unpack(Path::new(&info.dest))?;
    Ok(ExtractReport {
        folder: report.folder.to_string_lossy().to_string(),
        files: report.files,
        bytes: report.bytes,
        skipped: report.skipped.len() as u64,
        refused: report.refused,
    })
}

/// Unpack one archive beside itself into a folder of its own name.
///
/// The size of the archive seeds the write limit, so a file that claims a few
/// kilobytes and writes gigabytes is stopped rather than filling the disk.
fn unpack(archive: &Path) -> Result<idm_archive::Report, String> {
    if !archive.exists() {
        return Err(format!("{} is not there", archive.display()));
    }
    let size = std::fs::metadata(archive).map(|meta| meta.len()).unwrap_or(0);
    let dest = archive
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let mut on_file = |_path: &Path| {};
    let mut policy = Policy {
        dest,
        subfolder: Subfolder::Always,
        password: None,
        limits: Limits::for_archive(size),
        tool: find_7zip(),
        on_file: &mut on_file,
        cancel: None,
    };
    idm_archive::extract(archive, &mut policy).map_err(|error| error.to_string())
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
