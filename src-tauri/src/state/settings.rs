//! Saved choices: the engine's settings file, the desktop preferences, and the
//! small switches the windows and the tray read.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use idm_core::limits::Limits;

use crate::prefs::{self, DesktopPreferences};
use crate::settings_store::{self, EngineSettings};

use super::{string_folder, AppState, BrowserPolicy, BrowserTheme, VideoButtonSettings};

impl AppState {
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

    /// The appearance the window last settled on, for the extension's panel.
    pub fn browser_theme(&self) -> Option<BrowserTheme> {
        self.browser_theme
            .lock()
            .expect("browser theme lock poisoned")
            .clone()
    }

    /// Record the window's appearance. A window that is still starting has
    /// none yet, and the extension keeps its own palette until it does.
    pub fn set_browser_theme(&self, value: BrowserTheme) {
        *self
            .browser_theme
            .lock()
            .expect("browser theme lock poisoned") = Some(value);
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

    /// Whether a video download opens the quality window that follows it.
    ///
    /// A video download is not an engine download: the window that shows its
    /// progress is the quality panel. This is the switch that decides whether
    /// that panel comes up on its own when the download starts.
    pub fn show_video_window_on_start(&self) -> bool {
        self.show_video_window_on_start.load(Ordering::SeqCst)
    }

    pub fn set_show_video_window_on_start(&self, on: bool) {
        self.show_video_window_on_start.store(on, Ordering::SeqCst);
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

    /// The folder the application keeps its own data in, when it has been
    /// told where. The device list of the phone remote lives there.
    pub fn data_directory(&self) -> Option<PathBuf> {
        self.data_directory
            .lock()
            .expect("data dir lock poisoned")
            .clone()
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
        // One resolver, built here and put in force for the whole
        // application: every client the engine and the application build
        // looks names up through it. A mode whose endpoint cannot be used
        // reads as the system's, and the file says why in the log rather than
        // failing the download.
        let choice = settings.dns.choice();
        if let Err(error) = settings.dns.validated() {
            idm_core::log::write(
                "warn",
                &format!(
                    "the saved DNS settings cannot be used, the system's resolver is in force: {error}"
                ),
            );
        }
        idm_core::net::resolver::set_current(if choice.is_system() {
            None
        } else {
            Some(Arc::new(idm_core::net::Dns::new(
                &idm_core::net::Hosts::default(),
                &choice,
            )))
        });
        // The adapter every connection leaves through. A chosen adapter that
        // is not there is a route that is down: the clients refuse to be
        // built, and nothing takes the normal route instead.
        let route = idm_core::net::adapter::route_for(
            &settings.network_adapter,
            &idm_core::net::adapter::adapters(),
        );
        idm_core::net::adapter::set_route(route);
        self.set_show_complete_dialog(settings.show_complete_dialog);
        self.set_show_progress_on_start(settings.show_progress_on_start);
        self.set_show_video_window_on_start(settings.show_video_window_on_start);
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
}
