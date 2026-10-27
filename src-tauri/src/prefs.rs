//! The desk-side preferences the tray and the close button follow.
//!
//! These two choices decide whether the application may disappear from the
//! taskbar, so they cannot live only in the webview: the close button works
//! before a page has loaded, and a hidden window still has to remember what
//! its close button means. The file is small, versionless and written whole;
//! a damaged file falls back to the safe defaults instead of refusing to
//! start.

use std::path::Path;

use serde::{Deserialize, Serialize};

use idm_core::log;

/// The file name under the application's data directory.
pub const FILE_NAME: &str = "desktop.json";

/// Whether the window hides into the tray, and what closing it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DesktopPreferences {
    /// Hide the window instead of minimizing it when the minimize button is
    /// pressed. Off by default: a minimized window is still where the user
    /// left it.
    pub minimize_to_tray: bool,
    /// Keep the application running in the tray when the window is closed.
    /// On by default, which is what the tray icon is for.
    pub close_to_tray: bool,
}

impl Default for DesktopPreferences {
    fn default() -> Self {
        Self {
            minimize_to_tray: false,
            close_to_tray: true,
        }
    }
}

/// Read the preferences, or the defaults when there is no usable file.
///
/// A missing file is normal on a first run. Broken JSON and a failed read are
/// both logged and treated the same way: the defaults keep the application
/// usable and the next save repairs the file.
pub fn load(path: &Path) -> DesktopPreferences {
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<DesktopPreferences>(&bytes) {
            Ok(prefs) => prefs,
            Err(error) => {
                log::write(
                    "warn",
                    &format!("the desktop preferences could not be read: {error}"),
                );
                DesktopPreferences::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => DesktopPreferences::default(),
        Err(error) => {
            log::write(
                "warn",
                &format!("the desktop preferences could not be opened: {error}"),
            );
            DesktopPreferences::default()
        }
    }
}

/// Write the preferences whole.
///
/// The temporary file is written beside the real one and renamed over it, so
/// a crash in the middle of a save leaves either the old file or the new one,
/// never half of each. The rename replaces the destination on Windows and on
/// Unix; the fallback is for a file system that refuses an overwrite.
pub fn save(path: &Path, prefs: &DesktopPreferences) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("the preferences folder could not be made: {error}"))?;
    }

    let json = serde_json::to_vec_pretty(prefs)
        .map_err(|error| format!("the preferences could not be written: {error}"))?;
    let temporary = path.with_extension("json.new");
    std::fs::write(&temporary, &json)
        .map_err(|error| format!("the preferences could not be written: {error}"))?;

    if std::fs::rename(&temporary, path).is_ok() {
        return Ok(());
    }

    let result = std::fs::write(path, &json)
        .map_err(|error| format!("the preferences could not be written: {error}"));
    let _ = std::fs::remove_file(&temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "b-download-manager-prefs-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn the_defaults_keep_the_close_button_in_the_tray() {
        let prefs = DesktopPreferences::default();
        assert!(prefs.close_to_tray);
        assert!(
            !prefs.minimize_to_tray,
            "minimizing is the window's own job unless the user asks otherwise"
        );
    }

    #[test]
    fn preferences_survive_a_save_and_a_load() {
        let dir = temp_dir("round-trip");
        let path = dir.join(FILE_NAME);
        let prefs = DesktopPreferences {
            minimize_to_tray: true,
            close_to_tray: false,
        };

        save(&path, &prefs).expect("a saved file");
        assert_eq!(load(&path), prefs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_the_default() {
        let dir = temp_dir("missing");
        assert_eq!(load(&dir.join(FILE_NAME)), DesktopPreferences::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_file_falls_back_instead_of_failing() {
        let dir = temp_dir("damaged");
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, b"{ this is not json").expect("a damaged file");
        assert_eq!(load(&path), DesktopPreferences::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_partial_file_keeps_the_default_for_the_missing_choice() {
        let dir = temp_dir("partial");
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, br#"{"minimizeToTray":true}"#).expect("a partial file");
        assert_eq!(
            load(&path),
            DesktopPreferences {
                minimize_to_tray: true,
                close_to_tray: true,
            },
            "the missing close choice keeps the safe default"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_field_is_ignored() {
        let dir = temp_dir("unknown");
        let path = dir.join(FILE_NAME);
        std::fs::write(
            &path,
            br#"{"closeToTray":false,"minimizeToTray":false,"theme":"dark"}"#,
        )
        .expect("a file from a later version");
        assert_eq!(
            load(&path),
            DesktopPreferences {
                minimize_to_tray: false,
                close_to_tray: false,
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
