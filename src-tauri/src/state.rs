//! Shared application state: the set of downloads known to the app.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use idm_core::downloader::{Download, DownloadInfo, Status};
use idm_core::limits::Limits;
use idm_core::session::{Index, KnownDownload};

pub struct AppState {
    next_id: AtomicU64,
    downloads: Mutex<HashMap<u64, Arc<Download>>>,
    limits: Limits,
    index: Mutex<Option<Index>>,
    /// Where a download handed over by the browser is saved. `None` means the
    /// system Downloads folder.
    browser_folder: Mutex<Option<PathBuf>>,
    /// Whether closing the window hides it into the tray.
    minimize_to_tray: AtomicBool,
    /// Whether the window of a finished download is shown.
    show_complete_dialog: AtomicBool,
    /// Held while a start checks its destination and inserts the download, so
    /// two starts cannot both find the same name free.
    start_gate: Mutex<()>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            downloads: Mutex::new(HashMap::new()),
            limits: Limits::default(),
            index: Mutex::new(None),
            browser_folder: Mutex::new(None),
            minimize_to_tray: AtomicBool::new(true),
            show_complete_dialog: AtomicBool::new(true),
            start_gate: Mutex::new(()),
        }
    }
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

    /// The files the downloads in the list write to.
    pub fn destinations(&self) -> Vec<PathBuf> {
        self.destinations_except(None)
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

    pub fn infos(&self) -> Vec<DownloadInfo> {
        self.list().iter().map(|d| d.info()).collect()
    }

    /// The limits that apply to every download.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// The folder a browser hand-over is saved to, when the window has told
    /// the backend which one it uses.
    pub fn browser_folder(&self) -> Option<PathBuf> {
        self.browser_folder
            .lock()
            .expect("browser folder lock poisoned")
            .clone()
    }

    /// Point the browser hand-over at the folder the options window uses.
    pub fn set_browser_folder(&self, folder: Option<PathBuf>) {
        *self
            .browser_folder
            .lock()
            .expect("browser folder lock poisoned") = folder;
    }

    /// Whether closing the window hides it into the tray.
    pub fn minimize_to_tray(&self) -> bool {
        self.minimize_to_tray.load(Ordering::SeqCst)
    }

    /// Choose whether closing the window hides it into the tray.
    pub fn set_minimize_to_tray(&self, on: bool) {
        self.minimize_to_tray.store(on, Ordering::SeqCst);
    }

    /// Whether a finished download gets its own window.
    pub fn show_complete_dialog(&self) -> bool {
        self.show_complete_dialog.load(Ordering::SeqCst)
    }

    /// Choose whether a finished download gets its own window.
    pub fn set_show_complete_dialog(&self, on: bool) {
        self.show_complete_dialog.store(on, Ordering::SeqCst);
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
