//! The media downloads the app remembers, kept on disk.
//!
//! A video download is a media job, not an engine download, and the engine's
//! index does not carry it. Without a file of its own the whole video history
//! would live in memory only and vanish on every restart. This index is the
//! counterpart of `idm_core::session::Index` for those jobs: one record per
//! job that was actually started, written at every change of state and read
//! back before the bridge binds.
//!
//! The page's headers are never written here. They are a permission the
//! browser lent for one session, and a restored job asks the page again
//! rather than replaying a cookie from a file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use idm_core::media::{download::TrackProgress, Manifest};

use super::capture::DownloadRequest;
use super::download::Delivery;
use super::{MediaJob, MediaKindHint, MediaState, SourcePlan};

/// One media download the app remembers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownMediaJob {
    pub id: u64,
    pub state: MediaState,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub notice: Option<String>,
    #[serde(default)]
    pub page_url: String,
    #[serde(default)]
    pub manifest_url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub referer: String,
    #[serde(default)]
    pub kind: MediaKindHint,
    #[serde(default)]
    pub preferred: Option<String>,
    #[serde(default)]
    pub manifest: Option<Manifest>,
    #[serde(default)]
    pub source: Option<SourcePlan>,
    #[serde(default)]
    pub dest: Option<String>,
    #[serde(default)]
    pub work_dir: Option<String>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub progress: TrackProgress,
    #[serde(default)]
    pub fraction: f64,
    #[serde(default)]
    pub request: Option<DownloadRequest>,
    #[serde(default)]
    pub delivery: Option<Delivery>,
}

impl KnownMediaJob {
    /// The record of a job, when it is download history. A job that was only
    /// parsed — the extension offered a manifest nobody downloaded — is not
    /// history and is not kept.
    pub fn from_job(job: &MediaJob) -> Option<Self> {
        job.request.as_ref()?;
        Some(Self {
            id: job.id,
            state: job.state,
            error: job.error.clone(),
            notice: job.notice.clone(),
            page_url: job.page_url.clone(),
            manifest_url: job.manifest_url.clone(),
            title: job.title.clone(),
            referer: job.referer.clone(),
            kind: job.kind,
            preferred: job.preferred.clone(),
            manifest: job.manifest.clone(),
            source: job.source.clone(),
            dest: job.dest.clone(),
            work_dir: job.work_dir.clone(),
            done: job.done,
            stage: job.stage.clone(),
            progress: job.progress,
            fraction: job.fraction,
            request: job.request.clone(),
            delivery: job.delivery.clone(),
        })
    }

    /// The job a record restores. It is never downloading: the task that ran
    /// it died with the last run, and the caller decides from the disk
    /// whether it finished or can continue.
    pub fn into_job(self) -> MediaJob {
        let mut job = MediaJob::new(
            self.id,
            self.page_url,
            self.manifest_url,
            self.kind,
            self.title,
            self.referer,
            Vec::new(),
            self.preferred,
        );
        job.state = self.state;
        job.error = self.error;
        job.notice = self.notice;
        job.manifest = self.manifest;
        job.source = self.source;
        job.dest = self.dest;
        job.work_dir = self.work_dir;
        job.done = self.done;
        job.stage = self.stage;
        job.progress = self.progress;
        job.fraction = self.fraction;
        job.request = self.request;
        job.delivery = self.delivery;
        job
    }
}

/// The bytes one folder holds, counted through its subfolders. A restored
/// download reports what is really on disk rather than the number the last
/// run happened to save.
pub fn folder_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total += folder_bytes(&path);
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

/// The index of remembered media downloads.
#[derive(Debug, Clone)]
pub struct MediaIndex {
    path: PathBuf,
}

impl MediaIndex {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read the index. A missing or broken file is an empty index, and one
    /// record this build cannot read is skipped rather than taking the whole
    /// history down with it: the application must still start.
    pub fn load(&self) -> Vec<KnownMediaJob> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(&text) else {
            return Vec::new();
        };
        values
            .into_iter()
            .filter_map(|value| serde_json::from_value(value).ok())
            .collect()
    }

    /// Write the index. The write goes to a temporary file that is renamed
    /// into place, so a crash cannot leave a half written index behind.
    pub fn save(&self, known: &[KnownMediaJob]) -> std::io::Result<()> {
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
mod tests;
