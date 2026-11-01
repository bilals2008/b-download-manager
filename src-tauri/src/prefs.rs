//! The desk-side preferences the tray and the close button follow.
//!
//! These choices decide whether the application may disappear from the
//! taskbar, so they cannot live only in the webview: the close button works
//! before a page has loaded, and a hidden window still has to remember what
//! its close button means. The file is small, versionless and written whole;
//! a damaged file falls back to the safe defaults instead of refusing to
//! start.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use idm_core::log;

/// The file name under the application's data directory.
pub const FILE_NAME: &str = "desktop.json";

/// The protocols the clipboard is offered for, when the reader has not said
/// otherwise. The same three the reference product starts with.
pub const DEFAULT_URL_PROTOCOLS: &[&str] = &["http", "https", "ftp"];

/// How many protocols the list keeps. `src/lib/tray.ts` holds the same number
/// for the dialog that edits the list.
pub const MAX_URL_PROTOCOLS: usize = 32;

/// How the tray icon is drawn. The reference product offers these three, and
/// the menu of the clone keeps them in the same order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrayStyle {
    /// The flat mark of the application.
    #[default]
    Default,
    /// The same mark with a ramp on every body and a bevel around it.
    #[serde(rename = "3d")]
    ThreeD,
    /// The same mark drawn flat under one heavy outline.
    Classic,
}

/// Whether the minimize button hides the window into the tray, and what
/// closing it means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DesktopPreferences {
    /// Hide the window instead of minimizing it when the minimize button is
    /// pressed. Off by default: a minimized window is still where the user
    /// left it.
    pub minimize_to_tray: bool,
    /// Keep the application running in the tray when the window is closed.
    /// On by default, which is what the tray icon is for.
    pub close_to_tray: bool,
    /// The drawing the tray icon uses.
    pub tray_style: TrayStyle,
    /// Take the tray icon out of the notification area. The window is then the
    /// only way back, so closing it leaves rather than hides.
    pub tray_hidden: bool,
    /// Let the tray icon change its inks with the appearance of the window.
    /// Off leaves it on the dark drawing, which is what it has always used.
    pub tray_dark_mode: bool,
    /// The protocols a copied address has to start with to be offered as a
    /// download. An empty list offers nothing, so the list is never empty.
    pub url_protocols: Vec<String>,
    /// The last quality chosen for a host, keyed by the host's name. The
    /// quality panel opens on it; the engine never reads it.
    #[serde(default)]
    pub video_quality: HashMap<String, String>,
    /// Whether the reader allowed the browser's cookies to travel with media
    /// requests. Off by default; the panel asks once before the first cookie.
    #[serde(default)]
    pub cookie_consent: bool,
}

impl Default for DesktopPreferences {
    fn default() -> Self {
        Self {
            minimize_to_tray: false,
            close_to_tray: true,
            tray_style: TrayStyle::default(),
            tray_hidden: false,
            tray_dark_mode: false,
            url_protocols: DEFAULT_URL_PROTOCOLS
                .iter()
                .map(|p| (*p).to_string())
                .collect(),
            video_quality: HashMap::new(),
            cookie_consent: false,
        }
    }
}

/// A protocol as it is stored: lower-case, no `://`, nothing the engine cannot
/// match a prefix against. Junk becomes `None`.
///
/// The shape is a URI scheme (`RFC 3986`), so `mms` and `rtsp` are accepted and
/// `javascript`, `file` and anything with a slash in it stay out of the list
/// unless the reader puts a clean scheme in.
pub fn clean_url_protocol(value: &str) -> Option<String> {
    let entry = value
        .trim()
        .trim_end_matches("://")
        .trim_end_matches(':')
        .to_ascii_lowercase();
    let mut characters = entry.chars();
    let first = characters.next()?;
    if !first.is_ascii_alphabetic() || entry.len() > 16 {
        return None;
    }
    if !characters
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'))
    {
        return None;
    }
    Some(entry)
}

/// The protocols as stored, from whatever the options window sent: cleaned,
/// without repeats, and never empty.
pub fn clean_url_protocols(values: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in values {
        let Some(entry) = clean_url_protocol(value) else {
            continue;
        };
        if !out.contains(&entry) {
            out.push(entry);
        }
        if out.len() >= MAX_URL_PROTOCOLS {
            break;
        }
    }
    if out.is_empty() {
        return DEFAULT_URL_PROTOCOLS
            .iter()
            .map(|p| (*p).to_string())
            .collect();
    }
    out
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
        assert_eq!(prefs.tray_style, TrayStyle::Default);
        assert!(
            !prefs.tray_hidden,
            "the icon is there until the reader takes it away"
        );
        assert_eq!(prefs.url_protocols, ["http", "https", "ftp"]);
    }

    #[test]
    fn preferences_survive_a_save_and_a_load() {
        let dir = temp_dir("round-trip");
        let path = dir.join(FILE_NAME);
        let prefs = DesktopPreferences {
            minimize_to_tray: true,
            close_to_tray: false,
            tray_style: TrayStyle::ThreeD,
            tray_hidden: true,
            tray_dark_mode: true,
            url_protocols: vec!["https".to_string(), "rtsp".to_string()],
            video_quality: HashMap::from([("example.org".to_string(), "1080p".to_string())]),
            cookie_consent: true,
        };

        save(&path, &prefs).expect("a saved file");
        assert_eq!(load(&path), prefs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_three_styles_are_written_the_way_the_menu_names_them() {
        let dir = temp_dir("styles");
        let path = dir.join(FILE_NAME);
        for (style, name) in [
            (TrayStyle::Default, "default"),
            (TrayStyle::ThreeD, "3d"),
            (TrayStyle::Classic, "classic"),
        ] {
            let prefs = DesktopPreferences {
                tray_style: style,
                ..DesktopPreferences::default()
            };
            save(&path, &prefs).expect("a saved file");
            let written = std::fs::read_to_string(&path).expect("a readable file");
            assert!(
                written.contains(&format!("\"trayStyle\": \"{name}\"")),
                "{name} should be written as it is named: {written}"
            );
            assert_eq!(load(&path).tray_style, style);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_protocol_list_keeps_what_is_a_scheme_and_drops_the_rest() {
        let given = [
            "HTTPS".to_string(),
            " rtsp ".to_string(),
            "mms://".to_string(),
            "javascript".to_string(),
            "file:///C:/x".to_string(),
            "".to_string(),
            "a".repeat(40),
        ];
        assert_eq!(
            clean_url_protocols(&given),
            ["https", "rtsp", "mms", "javascript"]
        );
    }

    #[test]
    fn an_empty_protocol_list_falls_back_to_the_three_it_started_with() {
        assert_eq!(
            clean_url_protocols(&["not a url".to_string()]),
            DEFAULT_URL_PROTOCOLS
                .iter()
                .map(|protocol| protocol.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_quality_choice_survives_a_save_and_a_load() {
        let dir = temp_dir("quality");
        let path = dir.join(FILE_NAME);
        let mut prefs = DesktopPreferences::default();
        prefs
            .video_quality
            .insert("example.org".to_string(), "720p".to_string());
        save(&path, &prefs).expect("a saved file");
        assert_eq!(
            load(&path)
                .video_quality
                .get("example.org")
                .map(String::as_str),
            Some("720p")
        );
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
                ..DesktopPreferences::default()
            },
            "the missing choices keep the defaults, the tray ones included"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_protocol_list_from_an_earlier_file_is_kept_as_written() {
        let dir = temp_dir("protocols");
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, br#"{"urlProtocols":["https","mms"]}"#).expect("a written file");
        assert_eq!(
            load(&path).url_protocols,
            ["https".to_string(), "mms".to_string()]
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
                ..DesktopPreferences::default()
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
