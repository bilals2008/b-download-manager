//! Shared application state: the set of downloads known to the app.
//!
//! One [`AppState`] is managed by the application and handed to every window
//! and background loop. Its fields are private to this module tree; the
//! methods that read and change them are grouped by what they own:
//! [`downloads`] holds the list and its saved index, [`media`] the video jobs
//! the extension registers, [`staging`] the application's own folder for
//! unfinished bytes, [`categories`] the folder each category saves to, and
//! [`settings`] the preferences and the engine's settings.

mod categories;
mod downloads;
mod media;
mod settings;
mod staging;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::sync::{Arc, Mutex};

use idm_core::downloader::Download;
use idm_core::limits::Limits;
use idm_core::session::Index;

use crate::prefs::DesktopPreferences;
use crate::queue::Queue;
use crate::settings_store::EngineSettings;
use crate::staging::Staging;
use crate::video::MediaJob;

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

/// The window's appearance, as the extension has to draw it.
///
/// The application owns the themes; the extension only has the two shipped
/// palettes. The window publishes the tokens of the theme in force, and the
/// bridge hands them to the panel, so a custom theme is the same colour on
/// both sides.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BrowserTheme {
    /// The resolved theme name (`dark`, `cyberpunk`, `bloom`, ...).
    pub theme: String,
    /// Whether the theme is drawn on light surfaces.
    pub light: bool,
    /// The semantic tokens the extension paints with.
    pub palette: std::collections::HashMap<String, String>,
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
    /// The appearance of the window, handed to the extension's panel.
    browser_theme: Mutex<Option<BrowserTheme>>,
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
    /// Whether a video download opens the quality window that follows it.
    show_video_window_on_start: AtomicBool,
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
            browser_theme: Mutex::new(None),
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
            show_video_window_on_start: AtomicBool::new(true),
            clipboard_capture: AtomicBool::new(false),
            appearance_is_light: AtomicBool::new(false),
            staging: Mutex::new(None),
            category_rules: Mutex::new(Vec::new()),
            start_gate: Mutex::new(()),
            queue: Queue::default(),
        }
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
mod tests;
