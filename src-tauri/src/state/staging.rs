//! The application's own folder for unfinished bytes, and its sweep.

use std::path::{Path, PathBuf};

use idm_core::downloader::Status;

use crate::staging::Staging;

use super::{same_folder, unfinished_stem, AppState};

impl AppState {
    /// Where the parts and the manifest of a new download live. `None` until
    /// setup has resolved the staging tree; a start without it fails rather
    /// than writing the user's download folder full of temporaries.
    pub fn staging(&self) -> Option<Staging> {
        self.staging.lock().expect("staging lock poisoned").clone()
    }

    /// The folder one download owns for its ranges and manifest, for a
    /// destination at `dest`. The folder is on the volume of the destination
    /// when that volume can hold it, so the finished file is renamed into
    /// place rather than copied across a disk.
    pub fn work_dir(&self, id: u64, dest: &Path) -> Option<PathBuf> {
        self.staging().map(|staging| staging.download_dir(dest, id))
    }

    /// Point the state at the staging tree and sweep what no row claims.
    ///
    /// Called from setup once the tree is created. The sweep runs under the
    /// start gate's order (gate first, staging second) so it cannot race a
    /// start into deleting fresh files.
    pub fn set_staging(&self, staging: Staging) {
        let _gate = self.start_gate();
        {
            let mut slot = self.staging.lock().expect("staging lock poisoned");
            *slot = Some(staging);
        }
        self.sweep_staging();
    }

    /// Remove the staging folders no row in the list claims.
    ///
    /// A per-download folder is named by its id: one no row carries is the
    /// leftover of a deleted row or of a run whose files outlived the record,
    /// and by the recovery policy its bytes go with the row. Only numeric
    /// folders are touched — anything else in the tree was not written by a
    /// download. The same sweep covers the flat layout of the earlier
    /// releases, where files sat loose in the old unfinished folder.
    fn sweep_staging(&self) {
        let Some(staging) = self.staging() else {
            return;
        };
        // A finished row holds nothing: its ranges were joined and its folder
        // is left over from a run that ended before the cleanup. An unfinished
        // row claims its folder.
        let rows = self.list();
        // A row claims the folder it writes in, not merely its id: a
        // download whose folder moved to another volume leaves the old one
        // behind, and that one is empty and goes.
        let claimed: Vec<PathBuf> = rows
            .iter()
            .filter(|download| download.status() != Status::Completed)
            .map(|download| download.dir.clone())
            .collect();

        // The folders that hold the per-download folders: the one beside the
        // application data, and the one on every other volume a row uses.
        let mut roots: Vec<PathBuf> = vec![staging.downloads_root().to_path_buf()];
        for download in &rows {
            if let Some(parent) = download.dir.parent() {
                if !roots.iter().any(|root| root == parent) {
                    roots.push(parent.to_path_buf());
                }
            }
        }

        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if claimed.iter().any(|dir| dir == &path) {
                    continue;
                }
                if path.is_dir() {
                    let _ = std::fs::remove_dir_all(&path);
                }
            }
            // A volume that holds nothing of ours any more keeps no folder:
            // the hidden folder at its root would only be litter.
            let _ = std::fs::remove_dir(&root);
            if let Some(volume) = root.parent() {
                let _ = std::fs::remove_dir(volume);
            }
        }

        // The flat layout of the earlier releases: loose part and manifest
        // files in the old unfinished folder beside the application data.
        let legacy = staging
            .root()
            .parent()
            .map(|data| data.join(crate::staging::LEGACY_ROOT_NAME));
        if let Some(legacy) = legacy.filter(|dir| dir.exists() && !dir.eq(&staging.root())) {
            self.clean_flat_folder(&legacy);
        }
        // A media job lives in the memory of the application, not in a file:
        // a folder in the media tree that no running job claims is the
        // leftover of a job that died with the last run.
        let claimed_media: Vec<String> = self
            .media_jobs_snapshot()
            .into_iter()
            .filter_map(|job| job.work_dir.clone())
            .collect();
        if let Ok(entries) = std::fs::read_dir(staging.media_root()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path.to_string_lossy().to_string();
                if claimed_media.iter().any(|claimed| claimed == &name) {
                    continue;
                }
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }

    /// Remove the parts and manifests in one old flat folder that no row in
    /// the list claims.
    ///
    /// The caller holds the start gate, so a download cannot be inserting its
    /// row while its fresh files are read as abandoned. Only the engine's own
    /// names are touched: anything else in the folder is the user's.
    fn clean_flat_folder(&self, folder: &std::path::Path) {
        let claimed: Vec<String> = self
            .list()
            .into_iter()
            .filter(|download| same_folder(&download.dir, folder))
            .map(|download| download.stem.clone())
            .collect();
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let Some(stem) = unfinished_stem(&name) else {
                continue;
            };
            if claimed.iter().any(|known| known == stem) {
                continue;
            }
            let path = entry.path();
            if path.is_file() {
                let _ = std::fs::remove_file(&path);
            } else if path.is_dir() {
                let _ = std::fs::remove_dir(&path);
            }
        }
    }
}
