//! Shared application state: the set of downloads known to the app.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::downloader::{Download, DownloadInfo};

#[derive(Default)]
pub struct AppState {
    next_id: AtomicU64,
    downloads: Mutex<HashMap<u64, Arc<Download>>>,
}

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

    pub fn infos(&self) -> Vec<DownloadInfo> {
        self.list().iter().map(|d| d.info()).collect()
    }
}
