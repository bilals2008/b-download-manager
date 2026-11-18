//! The video manifests the extension registered, and their folders.

use std::path::Path;
use std::sync::atomic::Ordering;

use crate::video::persist::{folder_bytes, KnownMediaJob, MediaIndex};
use crate::video::{MediaJob, MediaState, SourcePlan};

use super::AppState;

impl AppState {
    pub fn next_media_id(&self) -> u64 {
        self.next_media_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Point the state at the file the media history lives in.
    pub fn set_media_index(&self, index: MediaIndex) {
        *self.media_index.lock().expect("media index lock poisoned") = Some(index);
    }

    /// Write the media downloads the app remembers. Called at every change
    /// of state — start, stop, finish, forget — and once more on the way out,
    /// so a restart finds the same list the reader left.
    pub fn save_media_index(&self) {
        let index = self.media_index.lock().expect("media index lock poisoned");
        let Some(index) = index.as_ref() else { return };
        let known: Vec<KnownMediaJob> = self
            .media_jobs_snapshot()
            .iter()
            .filter_map(KnownMediaJob::from_job)
            .collect();
        let _ = index.save(&known);
    }

    /// Bring the media downloads of an earlier run back.
    ///
    /// The same rule the engine's index follows: a job whose finished file is
    /// on disk is completed, one whose tracks are still in its work folder
    /// stopped and resumable, and one that left nothing behind is forgotten.
    /// The bytes on disk decide the numbers, not the last saved report.
    pub fn restore_media_jobs(&self) {
        let known = {
            let index = self.media_index.lock().expect("media index lock poisoned");
            let Some(index) = index.as_ref() else { return };
            index.load()
        };

        let mut highest = 0u64;
        for record in known {
            highest = highest.max(record.id);
            let dest_there = record
                .dest
                .as_deref()
                .is_some_and(|dest| Path::new(dest).exists());
            let work_there = record
                .work_dir
                .as_deref()
                .is_some_and(|dir| Path::new(dir).is_dir());
            if !dest_there && !work_there {
                continue;
            }
            let mut job = record.into_job();
            job.downloading = false;
            if dest_there {
                job.done = true;
                job.error = None;
                job.stage = Some("Saved".to_string());
                job.fraction = 1.0;
                if let Some(dest) = job.dest.as_deref() {
                    if let Ok(meta) = std::fs::metadata(dest) {
                        job.progress.downloaded = meta.len();
                        job.progress.total = job.progress.total.max(meta.len());
                    }
                }
            } else {
                job.done = false;
                job.stage = Some("Stopped".to_string());
                if let Some(dir) = job.work_dir.as_deref() {
                    let bytes = folder_bytes(Path::new(dir));
                    job.progress.downloaded = bytes;
                    job.progress.total = job.progress.total.max(bytes);
                    job.fraction = if job.progress.total > 0 {
                        bytes as f64 / job.progress.total as f64
                    } else {
                        0.0
                    };
                }
            }
            self.insert_media_job(job);
        }

        // The ids of the restored jobs must not be handed out again.
        self.next_media_id.fetch_max(highest + 1, Ordering::SeqCst);
        self.save_media_index();
    }

    /// Keep a parsed (or parsing) manifest until its download is started.
    pub fn insert_media_job(&self, job: MediaJob) {
        self.media_jobs
            .lock()
            .expect("media jobs lock poisoned")
            .insert(job.id, job);
    }

    /// A snapshot of every job, oldest first, for the home list.
    pub fn media_jobs_snapshot(&self) -> Vec<MediaJob> {
        let mut jobs: Vec<MediaJob> = self
            .media_jobs
            .lock()
            .expect("media jobs lock poisoned")
            .values()
            .cloned()
            .collect();
        jobs.sort_by_key(|job| job.id);
        jobs
    }

    pub fn media_job(&self, id: u64) -> Option<MediaJob> {
        let id = self.resolve_media_id(id);
        self.media_jobs
            .lock()
            .expect("media jobs lock poisoned")
            .get(&id)
            .cloned()
    }

    /// The live job a fresh registration for the same manifest belongs to.
    ///
    /// The extension asks for a manifest every time the reader opens the
    /// quality list, so without this a second click would create a second job
    /// for the same address and the merge below would retire the id the
    /// extension is watching. Reusing the live job keeps every watcher on one
    /// id. A finished or running job is left alone: it is not a list to merge
    /// new tracks into.
    pub fn media_job_for_manifest(&self, manifest_url: &str) -> Option<u64> {
        let manifest_url = manifest_url.trim();
        if manifest_url.is_empty() {
            return None;
        }
        self.media_jobs
            .lock()
            .expect("media jobs lock poisoned")
            .values()
            .filter(|job| {
                job.manifest_url == manifest_url
                    && !job.downloading
                    && !job.done
                    && matches!(job.state, MediaState::Parsing | MediaState::Ready)
            })
            .map(|job| job.id)
            .max()
    }

    /// Change one job under the lock, so two updates cannot overwrite each
    /// other. A job that is gone is not an error: the window may have been
    /// closed while its parsing task was still running.
    pub fn update_media_job(&self, id: u64, change: impl FnOnce(&mut MediaJob)) {
        let id = self.resolve_media_id(id);
        if let Some(job) = self
            .media_jobs
            .lock()
            .expect("media jobs lock poisoned")
            .get_mut(&id)
        {
            change(job);
        }
    }

    /// Follow an id that was merged into another one to the job that now holds
    /// the tracks. An id that was never merged resolves to itself.
    pub fn resolve_media_id(&self, id: u64) -> u64 {
        let aliases = self
            .media_aliases
            .lock()
            .expect("media aliases lock poisoned");
        let mut current = id;
        // A chain cannot be longer than the number of merges it took to make
        // it; the bound only keeps a hand-made cycle from looping forever.
        for _ in 0..32 {
            match aliases.get(&current) {
                Some(next) => current = *next,
                None => break,
            }
        }
        current
    }

    /// Retire a merged job without breaking a watcher that still holds its id.
    ///
    /// The tracks themselves are copied into `target` before this is called;
    /// every later request for `from` — a poll, a start, a cancel — is answered
    /// by `target` instead of a 404.
    pub fn alias_media_job(&self, from: u64, target: u64) {
        let target = self.resolve_media_id(target);
        self.media_jobs
            .lock()
            .expect("media jobs lock poisoned")
            .remove(&from);
        let mut aliases = self
            .media_aliases
            .lock()
            .expect("media aliases lock poisoned");
        for current in aliases.values_mut() {
            if *current == from {
                *current = target;
            }
        }
        aliases.insert(from, target);
    }

    /// Hold the merge check and the merge together.
    pub fn media_gate(&self) -> std::sync::MutexGuard<'_, ()> {
        self.media_gate.lock().expect("media gate lock poisoned")
    }

    /// The earlier ready file job that a new candidate from the same page
    /// belongs to. Only plain-file jobs merge; a manifest is its own job.
    pub fn merge_file_target(&self, except: u64, page_url: &str) -> Option<u64> {
        let page_url = page_url.trim();
        if page_url.is_empty() {
            return None;
        }
        self.media_jobs
            .lock()
            .expect("media jobs lock poisoned")
            .values()
            .filter(|job| {
                job.id != except
                    && job.page_url == page_url
                    && job.state == MediaState::Ready
                    && !job.downloading
                    && !job.done
                    && matches!(job.source, Some(SourcePlan::Files { .. }))
                    && job.manifest.is_some()
            })
            .map(|job| job.id)
            .max()
    }

    pub fn remove_media_job(&self, id: u64) {
        let id = self.resolve_media_id(id);
        self.media_jobs
            .lock()
            .expect("media jobs lock poisoned")
            .remove(&id);
        self.media_aliases
            .lock()
            .expect("media aliases lock poisoned")
            .retain(|_, target| *target != id);
    }

    /// The folder one media job owns for its tracks and join files.
    pub fn media_work_dir(
        &self,
        job_id: u64,
        dest: &Path,
        name: &str,
    ) -> Option<std::path::PathBuf> {
        self.staging()
            .map(|staging| staging.media_dir(dest, job_id, name))
    }

    /// Remove the media folders no known job owns.
    ///
    /// A folder that no job names is the leftover of one that was forgotten
    /// or died with the last run. Called when a row is removed and once at
    /// start-up after the history is restored, so the reader sees the disk
    /// match the list at once instead of after the next start.
    pub fn sweep_media_folders(&self) {
        let Some(staging) = self.staging() else {
            return;
        };
        let claimed: Vec<String> = self
            .media_jobs_snapshot()
            .into_iter()
            .filter_map(|job| job.work_dir.clone())
            .collect();
        let Ok(entries) = std::fs::read_dir(staging.media_root()) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path.to_string_lossy().to_string();
            if claimed.iter().any(|claimed| claimed == &name) {
                continue;
            }
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}
