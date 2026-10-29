//! Shared application state: the set of downloads known to the app.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use idm_core::downloader::{Download, DownloadInfo, Status};
use idm_core::limits::Limits;
use idm_core::session::{Index, KnownDownload};

use crate::prefs::{self, DesktopPreferences};

/// The capture rules the browser extension follows while the application is
/// running. The extension keeps its own copy for when the app is closed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
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
    /// Where the parts and the manifest of a new download live. `None` keeps
    /// them beside the destination, which is what an older index expects.
    unfinished_dir: Mutex<Option<PathBuf>>,
    /// The category folders, in the order a file name is matched against them.
    category_rules: Mutex<Vec<CategoryRule>>,
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
            browser_policy: Mutex::new(BrowserPolicy::default()),
            video_button_settings: Mutex::new(VideoButtonSettings::default()),
            browser_connections: AtomicU32::new(8),
            bridge_running: AtomicBool::new(false),
            last_extension_contact: AtomicU64::new(0),
            preferences: Mutex::new(DesktopPreferences::default()),
            preferences_path: Mutex::new(None),
            tray_available: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            show_complete_dialog: AtomicBool::new(true),
            show_progress_on_start: AtomicBool::new(false),
            clipboard_capture: AtomicBool::new(false),
            unfinished_dir: Mutex::new(None),
            category_rules: Mutex::new(Vec::new()),
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
    /// All of a download's temporaries share one unfinished folder and are
    /// named after the destination file, so a new start has to know which of
    /// those names are taken even when the destinations themselves are all
    /// different files. As with the destinations, a finished row holds
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

    /// Where the parts and the manifest of a new download live. `None` keeps
    /// them beside the destination, which is what an older index expects.
    pub fn unfinished_dir(&self) -> Option<PathBuf> {
        self.unfinished_dir
            .lock()
            .expect("unfinished folder lock poisoned")
            .clone()
    }

    /// Point new downloads at the unfinished folder and give every category
    /// its own destination.
    ///
    /// The window owns both values; the backend only applies them, so a
    /// browser hand-over lands where the options window says. Temporaries no
    /// row claims are cleared from the folder that is being left as well as
    /// from the one being taken, so a change does not strand them.
    ///
    /// The start gate is taken first and the folder lock second, the same
    /// order a start uses, so pushing a folder while one starts cannot
    /// deadlock.
    pub fn set_download_folders(&self, unfinished: Option<PathBuf>, rules: Vec<CategoryRule>) {
        let _gate = self.start_gate();
        let previous = {
            let mut slot = self
                .unfinished_dir
                .lock()
                .expect("unfinished folder lock poisoned");
            let previous = slot.take();
            *slot = unfinished;
            previous
        };
        *self
            .category_rules
            .lock()
            .expect("category rules lock poisoned") = rules;
        if let Some(folder) = previous {
            self.clean_folder(&folder);
        }
        if let Some(folder) = self.unfinished_dir() {
            self.clean_folder(&folder);
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

    /// Remove the parts and manifests in one unfinished folder that no row in
    /// the list claims.
    ///
    /// The caller holds the start gate, so a download cannot be inserting its
    /// row while its fresh files are read as abandoned. Only the engine's own
    /// names are touched: anything else in the folder is the user's.
    fn clean_folder(&self, folder: &std::path::Path) {
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
            }
        }
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

    /// The choices the tray and the close button follow.
    pub fn preferences(&self) -> DesktopPreferences {
        *self.preferences.lock().expect("preferences lock poisoned")
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
        let unfinished = dir.join("unfinished");
        let final_folder = dir.join("final");
        std::fs::create_dir_all(&unfinished).expect("folder");
        std::fs::create_dir_all(&final_folder).expect("folder");

        let state = AppState::default();
        let download = Download::new_in(
            1,
            "https://example.org/kept.mp4".to_string(),
            final_folder.join("kept.mp4"),
            Some(&unfinished),
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
            std::fs::write(unfinished.join(name), b"x").expect("write");
        }

        state.set_download_folders(Some(unfinished.clone()), Vec::new());

        assert!(unfinished.join("kept.mp4.part0").exists());
        assert!(unfinished.join("kept.mp4.idm.json").exists());
        assert!(!unfinished.join("gone.mp4.part0").exists());
        assert!(!unfinished.join("gone.mp4.idm.json.tmp").exists());
        assert!(!unfinished.join("gone.mp4.idmtmp").exists());
        assert!(
            unfinished.join("notes.txt").exists(),
            "a file the engine did not write stays"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rule_finds_the_category_and_its_folder() {
        let dir = temp_dir("rules");
        let state = AppState::default();
        state.set_download_folders(
            Some(dir.clone()),
            vec![
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
            ],
        );

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
        state.set_download_folders(
            Some(dir.clone()),
            vec![CategoryRule {
                name: "Video".to_string(),
                extensions: vec!["mp4".to_string()],
                folder: String::new(),
                builtin: true,
                fallback: false,
            }],
        );
        assert_eq!(
            state.detect_category(Path::new("C:\\Downloads\\unknown.bin")),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
