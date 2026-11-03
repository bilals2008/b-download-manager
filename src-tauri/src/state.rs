//! Shared application state: the set of downloads known to the app.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use idm_core::downloader::{Download, DownloadInfo, Status};
use idm_core::limits::Limits;
use idm_core::session::{Index, KnownDownload};

use crate::prefs::{self, DesktopPreferences};
use crate::queue::Queue;
use crate::settings_store::{self, EngineSettings};
use crate::staging::Staging;
use crate::video::{MediaJob, MediaState, SourcePlan};

/// The capture rules the browser extension follows while the application is
/// running. The extension keeps its own copy for when the app is closed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BrowserPolicy {
    /// Whether a browser download is handed to the application.
    pub capture: bool,
    /// Whether the hand-over opens the add dialog instead of starting.
    pub ask: bool,
    /// File types to take over. Empty means every type.
    pub types: Vec<String>,
    /// Hosts left to the browser.
    pub skip_sites: Vec<String>,
}

impl Default for BrowserPolicy {
    fn default() -> Self {
        Self {
            capture: true,
            ask: false,
            types: Vec::new(),
            skip_sites: Vec::new(),
        }
    }
}

/// The appearance of the floating download button the browser extension draws
/// on a video. The extension reads this from the loopback bridge.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VideoButtonSettings {
    /// "icon", "text" or "both".
    pub style: String,
    /// The text shown when the style includes text.
    pub label: String,
    /// Only show while the pointer is on the video.
    pub hover: bool,
}

impl Default for VideoButtonSettings {
    fn default() -> Self {
        Self {
            style: "icon".to_string(),
            label: "Download".to_string(),
            hover: false,
        }
    }
}

/// One category and the folder its downloads save to.
///
/// The window owns the list; the backend only matches a file name against it,
/// so a browser hand-over lands where the options window says.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CategoryRule {
    pub name: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub folder: String,
    /// Whether the category came with the application rather than from the
    /// reader. A rule the reader wrote wins over one of these when both claim
    /// a file type, which is how the categories pane detects one.
    #[serde(default)]
    pub builtin: bool,
    /// Whether this is where a file type nobody claims lands. The window
    /// marks the one category it sends everything unknown to, so the backend
    /// does not have to know its name.
    #[serde(default)]
    pub fallback: bool,
}

pub struct AppState {
    next_id: AtomicU64,
    downloads: Mutex<HashMap<u64, Arc<Download>>>,
    /// The manifests the extension has asked about. They live beside the
    /// downloads but are not downloads: a job becomes tracks only after the
    /// user picks a quality.
    next_media_id: AtomicU64,
    media_jobs: Mutex<HashMap<u64, MediaJob>>,
    /// Old job ids and the job they were merged into. The extension may still
    /// be watching the first id it was given, so it has to keep answering with
    /// the merged job instead of "unknown media job".
    media_aliases: Mutex<HashMap<u64, u64>>,
    /// Held while a file candidate is merged into an earlier one, so two
    /// candidates from the same page cannot both find no target.
    media_gate: Mutex<()>,
    limits: Limits,
    index: Mutex<Option<Index>>,
    /// Where a download handed over by the browser is saved. `None` means the
    /// system Downloads folder.
    browser_folder: Mutex<Option<PathBuf>>,
    /// The rules the extension follows while this application runs.
    browser_policy: Mutex<BrowserPolicy>,
    /// The appearance of the floating video download button the extension draws.
    video_button_settings: Mutex<VideoButtonSettings>,
    /// Connections a browser hand-over gets, since it has no dialog to ask.
    browser_connections: AtomicU32,
    /// Whether the loopback bridge bound its port.
    bridge_running: AtomicBool,
    /// The last time the extension reached the bridge, in seconds since the
    /// Unix epoch. Zero means it has not reached it yet.
    last_extension_contact: AtomicU64,
    /// What the minimize and close buttons mean, and where that choice is
    /// kept. The file is loaded before the tray is built, so an early close
    /// already follows the user's choice.
    preferences: Mutex<DesktopPreferences>,
    preferences_path: Mutex<Option<PathBuf>>,
    /// The engine's own settings, saved in one versioned file and applied to
    /// the runtime fields below. The webview edits them through, the file is
    /// the owner: a restart reads it before any window exists.
    engine_settings: Mutex<EngineSettings>,
    settings_path: Mutex<Option<PathBuf>>,
    /// Whether the settings file was on disk when the app opened. A first run
    /// or an upgrade has none, so the webview's copy seeds it.
    settings_file: AtomicBool,
    /// The application's own data folder, held so the staging tree can be
    /// re-resolved and the legacy folders swept.
    data_directory: Mutex<Option<PathBuf>>,
    /// Whether the main tray icon exists. Without it, hiding the window would
    /// strand the application with no way back.
    tray_available: AtomicBool,
    /// Set by the first genuine exit, so shutdown work happens once even when
    /// several exit paths meet.
    shutting_down: AtomicBool,
    /// Whether the window of a finished download is shown.
    show_complete_dialog: AtomicBool,
    /// Whether a new download opens its progress window.
    show_progress_on_start: AtomicBool,
    /// Whether the clipboard is watched for download links.
    clipboard_capture: AtomicBool,
    /// Which appearance the window is drawn in, as the window last said. The
    /// tray icon picks its inks from this when "Dark Mode support" is on.
    appearance_is_light: AtomicBool,
    /// Where the parts and the manifest of a new download live: the
    /// application's managed staging tree, never the user's download folder.
    /// `None` until setup has resolved it, which refuses to start downloads
    /// rather than letting their bytes land beside the finished file.
    staging: Mutex<Option<Staging>>,
    /// The category folders, in the order a file name is matched against them.
    category_rules: Mutex<Vec<CategoryRule>>,
    /// Held while a start checks its destination and inserts the download, so
    /// two starts cannot both find the same name free.
    start_gate: Mutex<()>,
    /// The download queue scheduler: order, running set, limit, active flag.
    /// Its own fields are interior-mutable, so it needs no lock here; holding
    /// one while it calls back into this state would deadlock.
    queue: Queue,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            downloads: Mutex::new(HashMap::new()),
            next_media_id: AtomicU64::new(1),
            media_jobs: Mutex::new(HashMap::new()),
            media_aliases: Mutex::new(HashMap::new()),
            media_gate: Mutex::new(()),
            limits: Limits::default(),
            index: Mutex::new(None),
            browser_folder: Mutex::new(None),
            browser_policy: Mutex::new(BrowserPolicy::default()),
            video_button_settings: Mutex::new(VideoButtonSettings::default()),
            browser_connections: AtomicU32::new(8),
            bridge_running: AtomicBool::new(false),
            last_extension_contact: AtomicU64::new(0),
            preferences: Mutex::new(DesktopPreferences::default()),
            preferences_path: Mutex::new(None),
            engine_settings: Mutex::new(EngineSettings::default()),
            settings_path: Mutex::new(None),
            settings_file: AtomicBool::new(false),
            data_directory: Mutex::new(None),
            tray_available: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            show_complete_dialog: AtomicBool::new(true),
            show_progress_on_start: AtomicBool::new(false),
            clipboard_capture: AtomicBool::new(false),
            appearance_is_light: AtomicBool::new(false),
            staging: Mutex::new(None),
            category_rules: Mutex::new(Vec::new()),
            start_gate: Mutex::new(()),
            queue: Queue::default(),
        }
    }
}

impl AppState {
    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    pub fn next_media_id(&self) -> u64 {
        self.next_media_id.fetch_add(1, Ordering::SeqCst)
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
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    /// Called when a queue-managed download finishes. Removes it from the
    /// queue's running set and starts the next queued download if the queue is
    /// still active and below its simultaneous limit.
    pub fn queue_on_finished(&self, app: &tauri::AppHandle, id: u64) {
        if self.queue().on_finished(id) {
            Queue::try_start_next(app, self);
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

    /// The folder one media job owns for its tracks and join files.
    pub fn media_work_dir(&self, job_id: u64, dest: &Path, name: &str) -> Option<PathBuf> {
        self.staging()
            .map(|staging| staging.media_dir(dest, job_id, name))
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

    /// Give every category its own destination.
    ///
    /// The window owns the list; the backend only applies it, so a browser
    /// hand-over lands where the options window says.
    pub fn set_categories(&self, rules: Vec<CategoryRule>) {
        let _gate = self.start_gate();
        *self
            .category_rules
            .lock()
            .expect("category rules lock poisoned") = rules;
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

    /// The category a file name belongs to, in the order the window sent the
    /// rules: a rule the reader wrote wins over a built-in one, the way the
    /// categories pane detects one. A type nobody claims belongs to the
    /// category the window marked as the fallback, which is what the add
    /// dialog would have shown for it.
    pub fn detect_category(&self, dest: &Path) -> Option<String> {
        let rules = self
            .category_rules
            .lock()
            .expect("category rules lock poisoned");
        let owns = |rule: &CategoryRule, extension: &str| {
            rule.extensions
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        };
        let matched = dest
            .extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .and_then(|extension| {
                rules
                    .iter()
                    .filter(|rule| !rule.builtin)
                    .find(|rule| owns(rule, &extension))
                    .or_else(|| rules.iter().find(|rule| owns(rule, &extension)))
            });
        matched
            .or_else(|| rules.iter().find(|rule| rule.fallback))
            .map(|rule| rule.name.clone())
    }

    /// The folder a category saves to, when the window gave one. A relative
    /// path is ignored: it would depend on where the app was started.
    pub fn category_folder(&self, name: &str) -> Option<PathBuf> {
        let rules = self
            .category_rules
            .lock()
            .expect("category rules lock poisoned");
        let folder = rules
            .iter()
            .find(|rule| rule.name == name)
            .map(|rule| rule.folder.trim())
            .filter(|folder| !folder.is_empty())?;
        let path = PathBuf::from(folder);
        path.is_absolute().then_some(path)
    }

    /// The capture rules the extension follows while the app runs.
    pub fn browser_policy(&self) -> BrowserPolicy {
        self.browser_policy
            .lock()
            .expect("browser policy lock poisoned")
            .clone()
    }

    pub fn set_browser_policy(&self, policy: BrowserPolicy) {
        *self
            .browser_policy
            .lock()
            .expect("browser policy lock poisoned") = policy;
    }

    /// The video download button appearance the extension applies to the overlay.
    pub fn video_button_settings(&self) -> VideoButtonSettings {
        self.video_button_settings
            .lock()
            .expect("video button settings lock poisoned")
            .clone()
    }

    /// Replace the video button appearance. The options window owns the value;
    /// the bridge serves it to the extension.
    pub fn set_video_button_settings(&self, value: VideoButtonSettings) {
        *self
            .video_button_settings
            .lock()
            .expect("video button settings lock poisoned") = value;
    }

    /// Connections a browser hand-over gets. Clamped to what the engine takes.
    pub fn browser_connections(&self) -> u32 {
        self.browser_connections.load(Ordering::SeqCst)
    }

    pub fn set_browser_connections(&self, count: u32) {
        self.browser_connections
            .store(count.clamp(1, 32), Ordering::SeqCst);
    }

    /// Whether the loopback bridge bound its port.
    pub fn bridge_running(&self) -> bool {
        self.bridge_running.load(Ordering::SeqCst)
    }

    pub fn set_bridge_running(&self, running: bool) {
        self.bridge_running.store(running, Ordering::SeqCst);
    }

    /// Record that the extension has just talked to the bridge.
    pub fn touch_extension(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        self.last_extension_contact.store(now, Ordering::SeqCst);
    }

    /// The last extension contact, in seconds since the Unix epoch.
    pub fn last_extension_contact(&self) -> Option<u64> {
        match self.last_extension_contact.load(Ordering::SeqCst) {
            0 => None,
            seconds => Some(seconds),
        }
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

    /// Whether a new download opens its progress window.
    pub fn show_progress_on_start(&self) -> bool {
        self.show_progress_on_start.load(Ordering::SeqCst)
    }

    pub fn set_show_progress_on_start(&self, on: bool) {
        self.show_progress_on_start.store(on, Ordering::SeqCst);
    }

    /// Whether the clipboard is watched for download links.
    pub fn clipboard_capture(&self) -> bool {
        self.clipboard_capture.load(Ordering::SeqCst)
    }

    pub fn set_clipboard_capture(&self, on: bool) {
        self.clipboard_capture.store(on, Ordering::SeqCst);
    }

    /// The protocols a copied address has to start with to be offered.
    pub fn url_protocols(&self) -> Vec<String> {
        self.preferences().url_protocols
    }

    /// Whether the window is drawn in the light appearance. Dark until the
    /// window says otherwise, which is the surface the icon was made for.
    pub fn appearance_is_light(&self) -> bool {
        self.appearance_is_light.load(Ordering::SeqCst)
    }

    pub fn set_appearance_is_light(&self, on: bool) {
        self.appearance_is_light.store(on, Ordering::SeqCst);
    }

    /// Point the state at the file the engine's settings live in and apply it.
    ///
    /// Called once during setup, before the bridge or the clipboard start, so
    /// a browser hand-over or a copied link meets the settings the reader
    /// chose, not the defaults. A file that cannot be read leaves the defaults
    /// in place, so the application still opens.
    pub fn set_settings_path(&self, path: PathBuf, data_dir: PathBuf) {
        // A file that exists, even one that cannot be read, is treated as the
        // reader's choice: its fields are read one by one, and an unreadable
        // one keeps its default. A missing file means the webview still owns
        // the values, and its next push creates the file.
        self.settings_file
            .store(path.try_exists().unwrap_or(false), Ordering::SeqCst);
        let settings = settings_store::load(&path);
        *self.engine_settings.lock().expect("settings lock poisoned") = settings;
        *self
            .settings_path
            .lock()
            .expect("settings path lock poisoned") = Some(path);
        *self.data_directory.lock().expect("data dir lock poisoned") = Some(data_dir);
        self.apply_engine_settings();
    }

    /// Whether the engine's settings file was there when the app opened, so
    /// the webview knows whether its own copy is a seed or a cache.
    pub fn settings_file_exists(&self) -> bool {
        self.settings_file.load(Ordering::SeqCst)
    }

    /// The engine's settings as they are saved.
    pub fn engine_settings(&self) -> EngineSettings {
        self.engine_settings
            .lock()
            .expect("settings lock poisoned")
            .clone()
    }

    /// Change the engine's settings and write the file.
    ///
    /// The caller says what to change; the copy is sanitized, applied to the
    /// runtime whole, and saved, so one set of rules feeds both the file and
    /// the running engine and they cannot drift.
    pub fn modify_settings(&self, change: impl FnOnce(&mut EngineSettings)) {
        let mut settings = self.engine_settings();
        change(&mut settings);
        let settings = settings_store::sanitize(settings);
        *self.engine_settings.lock().expect("settings lock poisoned") = settings;
        let data_dir = self
            .data_directory
            .lock()
            .expect("data dir lock poisoned")
            .is_some();
        self.save_engine_settings();
        if data_dir {
            self.apply_engine_settings();
        }
    }

    /// Write the choices to the file, when one has been chosen. A failure is
    /// logged rather than refused: the settings still apply for this run.
    pub fn save_engine_settings(&self) {
        let value = self.engine_settings();
        let path = self
            .settings_path
            .lock()
            .expect("settings path lock poisoned")
            .clone();
        let Some(path) = path else { return };
        let value = settings_store::sanitize(value);
        if let Err(error) = settings_store::save(&path, &value) {
            idm_core::log::write("warn", &error);
        } else {
            // The first write, from the webview's seed on an upgrade, makes
            // the file the owner from now on.
            self.settings_file.store(true, Ordering::SeqCst);
        }
    }

    /// Push the saved settings into the runtime fields, the whole object at a
    /// time, so a change cannot leave the file and the engine disagreeing.
    ///
    /// The staging tree is not part of the settings: the application owns it,
    /// setup resolved it, and only the category folders arrive from the file.
    pub fn apply_engine_settings(&self) {
        let settings = self.engine_settings();
        let rate = if settings.speed_limit_enabled {
            settings.speed_limit_bytes
        } else {
            0
        };
        self.limits.global_speed.set_rate(rate);
        self.limits.budget.resize(settings.max_connections);
        self.limits.set_proxy(if settings.proxy_url.is_empty() {
            None
        } else {
            Some(&settings.proxy_url)
        });
        self.set_show_complete_dialog(settings.show_complete_dialog);
        self.set_show_progress_on_start(settings.show_progress_on_start);
        self.set_clipboard_capture(settings.clipboard_capture);
        self.set_browser_connections(settings.default_connections);
        self.set_browser_folder(string_folder(&settings.default_folder));
        self.set_video_button_settings(settings.video_button.clone());
        self.set_browser_policy(settings.browser_policy.clone());
        self.set_categories(settings.categories.clone());
    }

    /// The choices the tray and the close button follow.
    pub fn preferences(&self) -> DesktopPreferences {
        self.preferences
            .lock()
            .expect("preferences lock poisoned")
            .clone()
    }

    /// The last quality chosen for a host, in the panel's own words
    /// (`1080p`). A host with no choice has none.
    pub fn video_quality(&self, host: &str) -> Option<String> {
        let host = host.trim().to_ascii_lowercase();
        if host.is_empty() {
            return None;
        }
        self.preferences().video_quality.get(&host).cloned()
    }

    /// Allow the browser's cookies to travel with media requests, once and
    /// for every site. The panel asks before the first cookie is forwarded.
    pub fn set_cookie_consent(&self, consent: bool) {
        let mut preferences = self.preferences();
        preferences.cookie_consent = consent;
        self.set_preferences(preferences);
        self.save_preferences();
    }

    /// Remember the quality chosen for a host and write the file. A failure
    /// is logged rather than refused: the choice still applies this run.
    pub fn remember_video_quality(&self, host: &str, quality: &str) {
        let host = host.trim().to_ascii_lowercase();
        let quality = quality.trim();
        if host.is_empty() || quality.is_empty() {
            return;
        }
        let mut preferences = self.preferences();
        preferences.video_quality.insert(host, quality.to_string());
        self.set_preferences(preferences);
        self.save_preferences();
    }

    /// Replace the choices. The caller saves them when the change is worth a
    /// write; a whole object keeps the two choices from drifting apart.
    pub fn set_preferences(&self, value: DesktopPreferences) {
        *self.preferences.lock().expect("preferences lock poisoned") = value;
    }

    /// Point the state at the preferences file and read it.
    ///
    /// Called once during setup, before the tray is built. A file that cannot
    /// be read leaves the defaults in place, so the application still opens.
    pub fn set_preferences_path(&self, path: PathBuf) {
        let value = prefs::load(&path);
        *self.preferences.lock().expect("preferences lock poisoned") = value;
        *self
            .preferences_path
            .lock()
            .expect("preferences path lock poisoned") = Some(path);
    }

    /// Write the choices to the file, when one has been chosen. A failure is
    /// logged rather than refused: the setting still applies for this run.
    pub fn save_preferences(&self) {
        let value = self.preferences();
        let path = self
            .preferences_path
            .lock()
            .expect("preferences path lock poisoned")
            .clone();
        let Some(path) = path else { return };
        if let Err(error) = prefs::save(&path, &value) {
            idm_core::log::write("warn", &error);
        }
    }

    /// Whether the tray icon exists, so hiding the window is reversible.
    pub fn tray_available(&self) -> bool {
        self.tray_available.load(Ordering::SeqCst)
    }

    pub fn set_tray_available(&self, on: bool) {
        self.tray_available.store(on, Ordering::SeqCst);
    }

    /// Take the one shutdown. `true` means this caller owns it.
    pub fn begin_shutdown(&self) -> bool {
        !self.shutting_down.swap(true, Ordering::SeqCst)
    }

    /// Whether the application is on its way out. Background loops check this
    /// before doing more work.
    pub fn shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
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

/// The download a temporary file in the unfinished folder belongs to.
///
/// Only the names the engine writes count: a part file, a manifest, the
/// temporary of a manifest, and the temporary of a join. Anything else in the
/// folder was not written by a download.
fn unfinished_stem(name: &str) -> Option<&str> {
    for suffix in [".idm.json.tmp", ".idm.json", ".idmtmp"] {
        if let Some(stem) = name.strip_suffix(suffix) {
            return (!stem.is_empty()).then_some(stem);
        }
    }
    let (stem, id) = name.rsplit_once(".part")?;
    let numbered = !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit());
    (numbered && !stem.is_empty()).then_some(stem)
}

/// A saved folder string as an engine folder: `None` when it is blank, which
/// is how "the system Downloads folder" travels.
fn string_folder(folder: &str) -> Option<PathBuf> {
    let trimmed = folder.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

/// Whether two paths name the same folder, matching the platform's case rules.
fn same_folder(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("my-idm-clone-state-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn abandoned_temporaries_are_cleared_and_a_known_stem_is_kept() {
        let dir = temp_dir("clean");
        // The flat layout of the earlier releases: loose files in the old
        // unfinished folder beside the application data.
        let legacy = dir.join("unfinished");
        let final_folder = dir.join("final");
        std::fs::create_dir_all(&legacy).expect("folder");
        std::fs::create_dir_all(&final_folder).expect("folder");

        let state = AppState::default();
        let download = Download::new_in(
            1,
            "https://example.org/kept.mp4".to_string(),
            final_folder.join("kept.mp4"),
            Some(&legacy),
            4,
            None,
            None,
        )
        .expect("a download");
        state.insert(download);

        for name in [
            "kept.mp4.part0",
            "kept.mp4.idm.json",
            "gone.mp4.part0",
            "gone.mp4.idm.json.tmp",
            "gone.mp4.idmtmp",
            "notes.txt",
        ] {
            std::fs::write(legacy.join(name), b"x").expect("write");
        }

        let staging = crate::staging::Staging::resolve(&dir).expect("a staging tree");
        state.set_staging(staging);

        assert!(legacy.join("kept.mp4.part0").exists());
        assert!(legacy.join("kept.mp4.idm.json").exists());
        assert!(!legacy.join("gone.mp4.part0").exists());
        assert!(!legacy.join("gone.mp4.idm.json.tmp").exists());
        assert!(!legacy.join("gone.mp4.idmtmp").exists());
        assert!(
            legacy.join("notes.txt").exists(),
            "a file the engine did not write stays"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn ready_job(id: u64, manifest_url: &str) -> MediaJob {
        use crate::video::{MediaKindHint, MediaState};
        use idm_core::media::{Format, Manifest, MediaKind};

        let mut job = MediaJob::new(
            id,
            "https://www.example.org/watch?v=1".to_string(),
            manifest_url.to_string(),
            MediaKindHint::File,
            "Film".to_string(),
            "https://www.example.org/watch?v=1".to_string(),
            Vec::new(),
            None,
        );
        job.state = MediaState::Ready;
        job.manifest = Some(Manifest {
            title: "Film".to_string(),
            duration: None,
            formats: vec![Format::new("137", MediaKind::Video, "mp4")],
            pairs: Vec::new(),
            container_hint: Some("file".to_string()),
        });
        job
    }

    #[test]
    fn a_merged_job_keeps_answering_for_the_id_the_extension_watches() {
        let state = AppState::default();
        state.insert_media_job(ready_job(1, "https://cdn.example.org/a.mp4"));
        state.insert_media_job(ready_job(2, "https://cdn.example.org/b.mp4"));

        state.alias_media_job(2, 1);

        assert_eq!(
            state.media_job(2).map(|job| job.id),
            Some(1),
            "the old id resolves to the job that now holds the tracks"
        );
        state.update_media_job(2, |job| job.title = "Changed".to_string());
        assert_eq!(
            state.media_job(1).map(|job| job.title),
            Some("Changed".to_string()),
            "a write through the old id lands on the merged job"
        );
        assert!(
            state.media_jobs_snapshot().iter().all(|job| job.id != 2),
            "the retired id is not a second row"
        );

        state.remove_media_job(2);
        assert!(
            state.media_job(1).is_none(),
            "forget removes the merged job"
        );
        assert!(state.media_job(2).is_none(), "and every alias with it");
    }

    #[test]
    fn a_manifest_that_is_already_open_reuses_its_job() {
        let state = AppState::default();
        state.insert_media_job(ready_job(4, "https://cdn.example.org/a.mp4"));

        assert_eq!(
            state.media_job_for_manifest("https://cdn.example.org/a.mp4"),
            Some(4)
        );
        assert_eq!(
            state.media_job_for_manifest("https://cdn.example.org/other.mp4"),
            None
        );

        state.update_media_job(4, |job| job.done = true);
        assert_eq!(
            state.media_job_for_manifest("https://cdn.example.org/a.mp4"),
            None,
            "a finished job is not a list to open again"
        );
    }

    #[test]
    fn file_candidates_from_one_page_find_their_target() {
        use crate::video::{FileTrack, MediaJob, MediaKindHint, MediaState};
        use idm_core::media::{Format, Manifest, MediaKind};

        let state = AppState::default();
        let mut job = MediaJob::new(
            1,
            "https://www.example.org/watch?v=1".to_string(),
            "https://cdn.example.org/videoplayback?itag=137".to_string(),
            MediaKindHint::File,
            "Film".to_string(),
            "https://www.example.org/watch?v=1".to_string(),
            Vec::new(),
            None,
        );
        job.state = MediaState::Ready;
        job.manifest = Some(Manifest {
            title: "Film".to_string(),
            duration: None,
            formats: vec![Format::new("137", MediaKind::Video, "mp4")],
            pairs: Vec::new(),
            container_hint: Some("file".to_string()),
        });
        job.source = Some(SourcePlan::Files {
            tracks: vec![FileTrack {
                format: Format::new("137", MediaKind::Video, "mp4"),
                url: "https://cdn.example.org/videoplayback?itag=137".to_string(),
            }],
        });
        state.insert_media_job(job);

        assert_eq!(
            state.merge_file_target(2, "https://www.example.org/watch?v=1"),
            Some(1)
        );
        assert_eq!(
            state.merge_file_target(2, "https://www.example.org/watch?v=2"),
            None,
            "a different video is a different job"
        );
        assert_eq!(
            state.merge_file_target(1, "https://www.example.org/watch?v=1"),
            None
        );

        state.update_media_job(1, |job| job.downloading = true);
        assert_eq!(
            state.merge_file_target(2, "https://www.example.org/watch?v=1"),
            None,
            "a running download takes no new tracks"
        );
    }

    #[test]
    fn a_rule_finds_the_category_and_its_folder() {
        let dir = temp_dir("rules");
        let state = AppState::default();
        state.set_categories(vec![
            CategoryRule {
                name: "Video".to_string(),
                extensions: vec!["mp4".to_string(), "mkv".to_string()],
                folder: "relative\\folder".to_string(),
                builtin: true,
                fallback: false,
            },
            CategoryRule {
                name: "Films".to_string(),
                extensions: vec!["mp4".to_string()],
                folder: dir.join("films").to_string_lossy().to_string(),
                builtin: false,
                fallback: false,
            },
            CategoryRule {
                name: "Other".to_string(),
                extensions: Vec::new(),
                folder: dir.join("misc").to_string_lossy().to_string(),
                builtin: true,
                fallback: true,
            },
        ]);

        assert_eq!(
            state
                .detect_category(Path::new("C:\\Downloads\\Film.MP4"))
                .as_deref(),
            Some("Films"),
            "a rule the reader wrote wins over a built-in one claiming the same type"
        );
        assert_eq!(
            state
                .detect_category(Path::new("C:\\Downloads\\Clip.MKV"))
                .as_deref(),
            Some("Video"),
            "a type only the built-in knows still lands there"
        );
        assert_eq!(
            state.category_folder("Films"),
            Some(dir.join("films")),
            "the folder the rule names"
        );
        assert_eq!(
            state.category_folder("Video"),
            None,
            "a relative folder is not applied"
        );
        assert_eq!(
            state
                .detect_category(Path::new("C:\\Downloads\\unknown.bin"))
                .as_deref(),
            Some("Other"),
            "a type nobody claims lands where the window sends it"
        );
        assert_eq!(
            state
                .detect_category(Path::new("C:\\Downloads\\README"))
                .as_deref(),
            Some("Other"),
            "a name with no type at all is the same case"
        );
        assert_eq!(state.category_folder("Other"), Some(dir.join("misc")));

        // Without a fallback the answer is no category at all, and the
        // destination stays where the start put it.
        state.set_categories(vec![CategoryRule {
            name: "Video".to_string(),
            extensions: vec!["mp4".to_string()],
            folder: String::new(),
            builtin: true,
            fallback: false,
        }]);
        assert_eq!(
            state.detect_category(Path::new("C:\\Downloads\\unknown.bin")),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
