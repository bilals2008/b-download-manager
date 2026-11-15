//! The downloads in the list, their saved index, and the queue's saved order.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use idm_core::downloader::{Download, DownloadInfo, Status};
use idm_core::session::{Index, KnownDownload};

use super::AppState;

impl AppState {
    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    pub fn insert(&self, download: Arc<Download>) {
        self.downloads
            .lock()
            .expect("downloads lock poisoned")
            .insert(download.id, download);
    }

    pub fn get(&self, id: u64) -> Option<Arc<Download>> {
        self.downloads
            .lock()
            .expect("downloads lock poisoned")
            .get(&id)
            .cloned()
    }

    pub fn list(&self) -> Vec<Arc<Download>> {
        let mut out: Vec<_> = self
            .downloads
            .lock()
            .expect("downloads lock poisoned")
            .values()
            .cloned()
            .collect();
        out.sort_by_key(|d| d.id);
        out
    }

    pub fn remove(&self, id: u64) {
        self.downloads
            .lock()
            .expect("downloads lock poisoned")
            .remove(&id);
    }

    /// Put a download in place of the row with the same id, after a move.
    pub fn replace(&self, download: Arc<Download>) {
        self.downloads
            .lock()
            .expect("downloads lock poisoned")
            .insert(download.id, download);
    }

    /// Hold the destination check and the insert of a new download together.
    pub fn start_gate(&self) -> std::sync::MutexGuard<'_, ()> {
        self.start_gate.lock().expect("start gate lock poisoned")
    }

    /// The download queue scheduler.
    pub fn queue(&self) -> &crate::queue::Queue {
        &self.queue
    }

    /// Called when a queue-managed download finishes. Removes it from the
    /// queue's running set and starts the next queued download if the queue is
    /// still active and below its simultaneous limit.
    pub fn queue_on_finished(&self, app: &tauri::AppHandle, id: u64) {
        if self.queue().on_finished(id) {
            crate::queue::Queue::try_start_next(app, self);
        }
    }

    /// The files the downloads in the list write to, minus one row.
    ///
    /// A download that is started again is not a conflict with the row it
    /// replaces, so that row's file is left out of the answer. A finished
    /// download is left out too: it has no task and no parts, and its file may
    /// have been deleted, so a new download of the same name is not a second
    /// writer.
    pub fn destinations_except(&self, except: Option<u64>) -> Vec<PathBuf> {
        self.list()
            .iter()
            .filter(|download| {
                Some(download.id) != except && download.status() != Status::Completed
            })
            .map(|download| download.dest.clone())
            .collect()
    }

    /// The part files and the manifests the downloads in the list write to,
    /// minus one row.
    ///
    /// A staged download keeps its temporaries in a folder named by its id,
    /// so a new start cannot collide with it; a row of the older layout keeps
    /// them beside the file or in one shared flat folder, named after the
    /// destination file. As with the destinations, a finished row holds
    /// nothing and a row being started again is not a conflict with itself.
    pub fn work_bases_except(&self, except: Option<u64>) -> Vec<PathBuf> {
        self.list()
            .iter()
            .filter(|download| {
                Some(download.id) != except && download.status() != Status::Completed
            })
            .map(|download| download.dir.join(&download.stem))
            .collect()
    }

    pub fn infos(&self) -> Vec<DownloadInfo> {
        self.list().iter().map(|d| d.info()).collect()
    }

    /// Restore the queue order from saved preferences, after downloads have
    /// been restored. Only downloads that exist and are still queued are kept.
    pub fn restore_queue(&self) {
        let order: Vec<u64> = self.preferences().queue_order.clone();
        let mut valid: Vec<u64> = Vec::new();
        for id in order {
            if let Some(download) = self.get(id) {
                if download.status() == Status::Queued {
                    valid.push(id);
                }
            }
        }
        if !valid.is_empty() {
            self.queue().set_order(valid);
        }
    }

    /// Persist the current queue order to preferences.
    pub fn save_queue(&self) {
        let order = self.queue().snapshot().order;
        let mut preferences = self.preferences();
        preferences.queue_order = order;
        self.set_preferences(preferences);
        self.save_preferences();
    }

    /// Point the state at the file the index of known downloads lives in.
    pub fn set_index(&self, index: Index) {
        *self.index.lock().expect("index lock poisoned") = Some(index);
    }

    /// Write the resume state of every download to disk.
    pub fn save_manifests(&self) {
        for download in self.list() {
            download.save_manifest();
        }
    }

    /// Write the index of known downloads.
    pub fn save_index(&self) {
        let index = self.index.lock().expect("index lock poisoned");
        let Some(index) = index.as_ref() else { return };
        let known: Vec<KnownDownload> = self.list().iter().map(|d| d.known()).collect();
        let _ = index.save(&known);
    }

    /// Bring the downloads of an earlier run back.
    ///
    /// A download whose file is on disk is finished, one whose parts and
    /// manifest are on disk can be resumed, and one that left nothing behind
    /// is forgotten.
    pub fn restore_downloads(&self) {
        let known = {
            let index = self.index.lock().expect("index lock poisoned");
            let Some(index) = index.as_ref() else { return };
            index.load()
        };

        let mut highest = 0u64;
        for known in known {
            highest = highest.max(known.id);
            match idm_core::session::inspect(&known) {
                idm_core::session::Recovered::Finished => {
                    if let Some(download) = Download::from_known(&known) {
                        download.set_status(idm_core::downloader::Status::Completed);
                        self.insert(download);
                    }
                }
                idm_core::session::Recovered::Resumable => {
                    if let Some(download) = Download::from_known(&known) {
                        self.insert(download);
                    }
                }
                idm_core::session::Recovered::Gone => {}
            }
        }

        // The ids of the restored downloads must not be handed out again.
        self.next_id.fetch_max(highest + 1, Ordering::SeqCst);
        self.save_index();
    }
}
