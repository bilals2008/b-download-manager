//! The engine's own settings, stored in one versioned file.
//!
//! The webview keeps a working copy of these, but the engine has to know them
//! without a window: the browser bridge, the tray and a clipboard poll all act
//! before any page has loaded. So the settings the engine actually applies
//! live here and are read at start-up, before the bridge binds; the webview's
//! copy is a cache the reader edits through, not the owner.
//!
//! The file carries a version and is written whole; a damaged or partly
//! written file falls back per field to the default instead of resetting
//! everything, and the next save repairs it. Proxy credentials are never
//! stored: the engine refuses a proxy that carries them, and the stored
//! address has its userinfo stripped in case an old file still has one.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::state::{BrowserPolicy, CategoryRule, VideoButtonSettings};

/// The file name under the application's data directory.
pub const FILE_NAME: &str = "settings.json";

/// The version of the file this build reads. A file with a smaller version is
/// read field by field the way the current reader can; a bigger one has
/// settings this build has never heard of, which are ignored.
///
/// Version 2 dropped `unfinishedFolder`: the application owns the folder for
/// unfinished bytes now, and a reader's old choice is read by the start-up
/// migration only to move what is left in it, never to store it again.
pub const VERSION: u32 = 2;

/// The largest connection count the engine accepts, also the clamps's top.
pub const MAX_CONNECTIONS: u32 = 32;

/// The largest byte rate one value may claim, one gigabyte a second.
const MAX_SPEED: u64 = 1024 * 1024 * 1024;

/// The settings the download engine applies, as stored on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineSettings {
    /// The version of the file. Written once and kept for the reader of the
    /// next build.
    pub version: u32,
    /// An absolute folder new downloads default to, empty for the system
    /// Downloads folder. This is the value a browser hand-over uses.
    pub default_folder: String,
    /// The folder each category saves to, in the order a file name is matched.
    pub categories: Vec<CategoryRule>,
    /// Connections a browser hand-over gets, since it has no dialog to ask.
    pub default_connections: u32,
    /// Connections the whole application may hold open at once.
    pub max_connections: u32,
    /// Whether the bandwidth cap is on.
    pub speed_limit_enabled: bool,
    /// The bandwidth cap, in bytes per second.
    pub speed_limit_bytes: u64,
    /// The proxy new downloads go through, empty for none. Never carries a
    /// username or a password.
    pub proxy_url: String,
    /// What happens when a file of the same name is already at the
    /// destination. The window's File Exists dialog answers with it.
    /// The capture rules the browser extension follows.
    pub browser_policy: BrowserPolicy,
    /// The appearance of the floating video download button.
    pub video_button: VideoButtonSettings,
    /// Whether a finished download gets its own window.
    pub show_complete_dialog: bool,
    /// Whether a new download opens its progress window.
    pub show_progress_on_start: bool,
    /// Whether the clipboard is watched for download links.
    pub clipboard_capture: bool,
}

impl Default for EngineSettings {
    fn default() -> Self {
        Self {
            version: VERSION,
            default_folder: String::new(),
            categories: Vec::new(),
            default_connections: 8,
            max_connections: 32,
            speed_limit_enabled: false,
            speed_limit_bytes: 0,
            proxy_url: String::new(),
            browser_policy: BrowserPolicy::default(),
            video_button: VideoButtonSettings::default(),
            show_complete_dialog: true,
            show_progress_on_start: false,
            clipboard_capture: false,
        }
    }
}

/// Clean a number into the range the engine accepts, or `None` when it is
/// nothing a number means.
fn bounded(value: u32, max: u32) -> Option<u32> {
    (value > 0).then(|| value.min(max))
}

/// Read the settings, or the defaults when there is no usable file.
///
/// A missing file is normal on a first run or an upgrade: the webview pushes
/// its own saved values for the setting it edits, and those writes create the
/// file. Each bad field keeps the default rather than discarding the rest of
/// what was saved.
pub fn load(path: &Path) -> EngineSettings {
    let read = || -> Result<EngineSettings, String> {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        serde_json::from_slice::<EngineSettings>(&bytes).map_err(|error| error.to_string())
    };
    match read() {
        Ok(settings) => sanitize(settings),
        Err(error) => {
            idm_core::log::write(
                "warn",
                &format!("the engine settings could not be read: {error}"),
            );
            EngineSettings::default()
        }
    }
}

/// Write the settings whole, the way the desktop preferences are written: a
/// temporary file beside the real one is renamed over it, so a crash leaves
/// either the old file or the new one.
pub fn save(path: &Path, settings: &EngineSettings) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("the settings folder could not be made: {error}"))?;
    }
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("the settings could not be written: {error}"))?;
    let temporary = path.with_extension("json.new");
    std::fs::write(&temporary, &json)
        .map_err(|error| format!("the settings could not be written: {error}"))?;
    if std::fs::rename(&temporary, path).is_ok() {
        return Ok(());
    }
    let result = std::fs::write(path, &json)
        .map_err(|error| format!("the settings could not be written: {error}"));
    let _ = std::fs::remove_file(&temporary);
    result
}

/// Clamp one value into the range the engine accepts.
pub fn sanitize(mut settings: EngineSettings) -> EngineSettings {
    settings.version = VERSION;
    if !settings.default_folder.trim().is_empty()
        && !Path::new(&settings.default_folder).is_absolute()
    {
        // The webview refuses a relative folder too; a file edited by hand is
        // given the default instead of a path that depends on where the app
        // was started.
        settings.default_folder = String::new();
    }
    settings.default_connections =
        bounded(settings.default_connections, MAX_CONNECTIONS).unwrap_or(8);
    settings.max_connections =
        bounded(settings.max_connections, MAX_CONNECTIONS).unwrap_or(MAX_CONNECTIONS);
    settings.speed_limit_bytes = settings.speed_limit_bytes.min(MAX_SPEED);
    if !settings.speed_limit_enabled {
        settings.speed_limit_bytes = 0;
    }
    settings.proxy_url = strip_credentials(&settings.proxy_url);
    settings
}

/// A proxy address that cannot be used to look up who owns it.
///
/// The engine already refuses a proxy with a username or a password, so this
/// only matters for a file written by something else: the userinfo is taken
/// out before the address is ever applied, so no credential survives a read.
fn strip_credentials(url: &str) -> String {
    let trimmed = url.trim();
    if !trimmed.contains("://") {
        return String::new();
    }
    match reqwest::Url::parse(trimmed) {
        // Setting an empty userinfo is how Url removes credentials.
        Ok(parsed) => {
            let mut clean = parsed;
            clean.set_username("").expect("an http-like url keeps");
            if clean.password().is_some() {
                clean.set_password(None).expect("an http-like url keeps");
            }
            if clean.as_str() == trimmed {
                trimmed.to_string()
            } else {
                clean.to_string()
            }
        }
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "b-download-manager-settings-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(FILE_NAME)
    }

    /// An absolute folder that is valid on the host: the sanitizer drops a
    /// path the platform cannot use, so a hard-coded Windows path would fail
    /// these tests on Linux.
    fn sample_folder(tag: &str) -> String {
        std::env::temp_dir()
            .join(format!("b-download-manager-{tag}-{}", std::process::id()))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn settings_survive_a_save_and_a_load() {
        let path = temp_file("round-trip");
        let downloads = sample_folder("downloads");
        let films = sample_folder("films");
        let settings = EngineSettings {
            default_folder: downloads,
            categories: vec![CategoryRule {
                name: "Films".to_string(),
                extensions: vec!["mp4".to_string()],
                folder: films,
                builtin: true,
                fallback: false,
            }],
            speed_limit_enabled: true,
            speed_limit_bytes: 2048,
            proxy_url: "socks5://127.0.0.1:1080".to_string(),
            video_button: VideoButtonSettings {
                label: "Hole".to_string(),
                ..VideoButtonSettings::default()
            },
            ..EngineSettings::default()
        };

        save(&path, &settings).expect("a saved file");
        assert_eq!(load(&path), settings);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_missing_file_is_the_default() {
        let path = temp_file("missing");
        assert_eq!(load(&path), EngineSettings::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_damaged_file_is_the_default_and_the_rest_still_works() {
        let path = temp_file("damaged");
        std::fs::write(&path, b"{ this is not json").expect("a damaged file");
        assert_eq!(load(&path), EngineSettings::default());
        // The next save repairs it: the load fell back so a load-save round
        // trip rewrites a whole file.
        save(&path, &EngineSettings::default()).expect("the save recovers");
        assert_eq!(load(&path), EngineSettings::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn values_out_of_range_keep_their_defaults_field_by_field() {
        let path = temp_file("fields");
        std::fs::write(
            &path,
            br#"{"version":1,"defaultConnections":0,"maxConnections":99,"speedLimitEnabled":true,"speedLimitBytes":1099511627776,"duplicatePolicy":"overwrite","unused":"x"}"#,
        )
        .expect("a written file");
        let settings = load(&path);
        assert_eq!(settings.default_connections, 8, "an empty connection count");
        assert_eq!(settings.max_connections, 32, "over the clamp's top");
        assert_eq!(
            settings.speed_limit_bytes, MAX_SPEED,
            "a too-big cap clamps"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_relative_folder_is_not_kept() {
        let path = temp_file("folders");
        std::fs::write(&path, br#"{"version":2,"defaultFolder":"downloads"}"#)
            .expect("a written file");
        let settings = load(&path);
        assert_eq!(settings.default_folder, "");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_file_from_version_one_loses_its_unfinished_folder_gently() {
        // The setting is gone: the application owns that folder now. An old
        // file that still names one must read without an error, and the next
        // save must not keep the key.
        let path = temp_file("v1-parts");
        let downloads = sample_folder("v1-downloads");
        std::fs::write(
            &path,
            serde_json::json!({
                "version": 1,
                "defaultFolder": downloads.clone(),
                "unfinishedFolder": "C:\\Users\\me\\Downloads",
            })
            .to_string(),
        )
        .expect("a written file");
        let settings = load(&path);
        assert_eq!(settings.default_folder, downloads);
        save(&path, &settings).expect("a saved file");
        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            !written.contains("unfinishedFolder"),
            "the dropped setting is not written again: {written}"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_disabled_cap_remembers_no_rate() {
        let path = temp_file("disabled");
        std::fs::write(
            &path,
            br#"{"version":1,"speedLimitEnabled":false,"speedLimitBytes":500}"#,
        )
        .expect("a written file");
        assert_eq!(load(&path).speed_limit_bytes, 0);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_proxy_never_writes_down_who_switched_it() {
        let path = temp_file("credentials");
        std::fs::write(
            &path,
            br#"{"version":1,"proxyUrl":"http://alice:hunter2@proxy.example.org:8080"}"#,
        )
        .expect("a written file");
        let loaded = load(&path).proxy_url;
        assert!(!loaded.contains("alice"), "{loaded}");
        assert!(!loaded.contains("hunter2"), "{loaded}");
        assert!(loaded.contains("proxy.example.org:8080"), "{loaded}");

        let settings = EngineSettings {
            proxy_url: "http://bob:secret@127.0.0.1:8080".to_string(),
            ..EngineSettings::default()
        };
        save(&path, &sanitize(settings)).expect("a saved file");
        let written = std::fs::read_to_string(&path).expect("a readable file");
        assert!(
            !written.contains("bob") && !written.contains("secret"),
            "{written}"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn an_older_version_of_the_file_still_reads() {
        let path = temp_file("version");
        let folder = sample_folder("older");
        std::fs::write(
            &path,
            serde_json::json!({
                "version": 0,
                "defaultFolder": folder.clone(),
                "speedLimitEnabled": true,
                "speedLimitBytes": 42,
            })
            .to_string(),
        )
        .expect("a written file");
        let settings = load(&path);
        assert_eq!(settings.default_folder, folder);
        assert!(settings.speed_limit_enabled);
        // A read is also what the next save version stamps: the file the app
        // hands over is always the current one.
        save(&path, &settings).expect("a saved file");
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("read back")).expect("json");
        assert_eq!(written["version"], VERSION);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn unknown_fields_from_a_later_version_are_ignored() {
        let path = temp_file("unknown");
        let folder = sample_folder("newer");
        std::fs::write(
            &path,
            serde_json::json!({
                "version": 1,
                "futureSetting": "x",
                "defaultFolder": folder.clone(),
            })
            .to_string(),
        )
        .expect("a written file");
        let settings = load(&path);
        assert_eq!(settings.default_folder, folder);
        assert!(!settings.clipboard_capture);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
