//! The list of downloads the app knows about, kept on disk.
//!
//! IDM keeps the equivalent in the registry, under
//! `HKCU\Software\DownloadManager\<id>`. The clone keeps a JSON index in the
//! application data directory, so a download survives a restart of the app:
//! the part files and the manifest are still on disk, and the index says
//! which files they belong to.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::downloader::Status;

/// One download the app remembers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownDownload {
    pub id: u64,
    pub url: String,
    pub dest: String,
    pub connections: u32,
    pub total: u64,
    pub downloaded: u64,
    /// The status the download had when the app was closed.
    pub status: Status,
    pub error: Option<String>,
    /// The page the link was on, when the browser knew it. Older index files
    /// do not have it.
    #[serde(default)]
    pub referer: Option<String>,
    /// The bandwidth cap the download had, in bytes per second. Older index
    /// files have no cap.
    #[serde(default)]
    pub speed_limit: u64,
}

/// What is left on disk for a remembered download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovered {
    /// The destination file is there, the download finished.
    Finished,
    /// The manifest and the part files are there, the download can resume.
    Resumable,
    /// Nothing is left of it.
    Gone,
}

/// Look at the disk and decide what can be done with a remembered download.
pub fn inspect(known: &KnownDownload) -> Recovered {
    let dest = PathBuf::from(&known.dest);
    if dest.exists() {
        return Recovered::Finished;
    }
    if manifest_path(&dest).exists() {
        return Recovered::Resumable;
    }
    Recovered::Gone
}

/// The manifest that belongs to a destination file.
pub fn manifest_path(dest: &Path) -> PathBuf {
    let stem = dest
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    dest.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(format!("{stem}.idm.json"))
}

/// The index of remembered downloads.
#[derive(Debug, Clone)]
pub struct Index {
    path: PathBuf,
}

impl Index {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read the index. A missing or broken file is an empty index: the
    /// application must still start.
    pub fn load(&self) -> Vec<KnownDownload> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        serde_json::from_str::<Vec<KnownDownload>>(&text).unwrap_or_default()
    }

    /// Write the index. The write goes to a temporary file that is renamed
    /// into place, so a crash cannot leave a half written index behind.
    pub fn save(&self, known: &[KnownDownload]) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let text = serde_json::to_string_pretty(known)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "my-idm-clone-session-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn known(dest: &Path) -> KnownDownload {
        KnownDownload {
            id: 1,
            url: "https://example.com/a.bin".to_string(),
            dest: dest.to_string_lossy().to_string(),
            connections: 8,
            total: 100,
            downloaded: 40,
            status: Status::Paused,
            error: None,
            referer: None,
            speed_limit: 0,
        }
    }

    #[test]
    fn the_index_round_trips() {
        let dir = temp_dir("round-trip");
        let index = Index::new(dir.join("downloads.json"));
        let entry = known(&dir.join("a.bin"));

        index.save(std::slice::from_ref(&entry)).expect("save");
        let loaded = index.load();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].url, entry.url);
        assert_eq!(loaded[0].downloaded, 40);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_index_is_an_empty_index() {
        let dir = temp_dir("missing");
        let index = Index::new(dir.join("nothing.json"));
        assert!(index.load().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_index_is_an_empty_index() {
        let dir = temp_dir("broken");
        let path = dir.join("downloads.json");
        std::fs::write(&path, "{ not json").expect("write");
        let index = Index::new(path);
        assert!(index.load().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_download_with_its_file_on_disk_is_finished() {
        let dir = temp_dir("finished");
        let dest = dir.join("a.bin");
        std::fs::write(&dest, b"done").expect("write");
        assert_eq!(inspect(&known(&dest)), Recovered::Finished);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_download_with_its_manifest_on_disk_can_resume() {
        let dir = temp_dir("resumable");
        let dest = dir.join("a.bin");
        std::fs::write(manifest_path(&dest), "{}").expect("write");
        assert_eq!(inspect(&known(&dest)), Recovered::Resumable);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_download_with_nothing_left_is_gone() {
        let dir = temp_dir("gone");
        assert_eq!(inspect(&known(&dir.join("a.bin"))), Recovered::Gone);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
