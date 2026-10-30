//! Toolbar packs: the files that change the look of the button bar.
//!
//! A pack is one JSON file in the application's data folder. The file is
//! checked before it is stored and again before it is read, so a broken or
//! hostile pack becomes a sentence in the toolbars dialog rather than a bar
//! that cannot be drawn.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// The file format this build writes and reads.
const FORMAT_VERSION: u32 = 1;

/// A pack larger than this is not a toolbar.
const MAX_PACK_BYTES: u64 = 512 * 1024;

const MAX_ICONS: usize = 32;
const MAX_ICON_BYTES: usize = 16 * 1024;
const MAX_BUTTONS: usize = 64;
const MAX_ID: usize = 48;
const MAX_NAME: usize = 48;
const MAX_VERSION: usize = 24;
const MAX_AUTHOR: usize = 64;
const MAX_DESCRIPTION: usize = 240;

/// Ids the application owns; an installed file may not take them.
const RESERVED_IDS: [&str; 5] = ["default", "3d", "classic", "isometric", "blueprint"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PackStyle {
    pub background: Option<String>,
    pub text: Option<String>,
    pub accent: Option<String>,
    pub hover: Option<String>,
    pub active: Option<String>,
    pub line: Option<String>,
    pub glow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PackManifest {
    pub format: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub style: PackStyle,
    pub icons: BTreeMap<String, String>,
    pub buttons: Vec<String>,
}

impl Default for PackManifest {
    fn default() -> Self {
        Self {
            format: FORMAT_VERSION,
            id: String::new(),
            name: String::new(),
            version: String::new(),
            author: String::new(),
            description: String::new(),
            style: PackStyle::default(),
            icons: BTreeMap::new(),
            buttons: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackProblem {
    pub file: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacksSnapshot {
    pub folder: String,
    pub packs: Vec<PackManifest>,
    pub problems: Vec<PackProblem>,
}

/// A refusal the toolbars dialog can paint: what happened and, for a name
/// clash, which bar already holds the name.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackError {
    pub kind: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

fn failure(kind: &'static str, message: impl Into<String>) -> PackError {
    PackError {
        kind,
        message: message.into(),
        name: None,
    }
}

fn invalid(message: impl Into<String>) -> PackError {
    failure("invalid", message)
}

fn io(message: impl Into<String>) -> PackError {
    failure("io", message)
}

/* ------------------------------------------------------------- validation */

fn valid_pack_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    value.len() <= MAX_ID
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

/// A command or an icon key: lower-case words joined by hyphens.
fn valid_key(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    value.len() <= MAX_ID
        && first.is_ascii_lowercase()
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn has_control(value: &str) -> bool {
    value.chars().any(|c| c.is_control() && c != '\n')
}

fn check_text(field: &str, value: &str, max: usize) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("The toolbar needs a {field}."));
    }
    if trimmed.chars().count() > max {
        return Err(format!("The {field} is longer than {max} characters."));
    }
    if has_control(trimmed) {
        return Err(format!("The {field} carries a control character."));
    }
    Ok(trimmed.to_string())
}

fn check_optional(field: &str, value: &mut String, max: usize) -> Result<(), String> {
    let trimmed = value.trim().to_string();
    if trimmed.chars().count() > max {
        return Err(format!("The {field} is longer than {max} characters."));
    }
    if has_control(&trimmed) {
        return Err(format!("The {field} carries a control character."));
    }
    *value = trimmed;
    Ok(())
}

fn check_color(field: &str, value: &mut Option<String>) -> Result<(), String> {
    if let Some(text) = value {
        let trimmed = text.trim();
        if !valid_color(trimmed) {
            return Err(format!(
                "The {field} colour must be a hex colour such as #22e7ff."
            ));
        }
        *value = Some(trimmed.to_string());
    }
    Ok(())
}

/// Clean a manifest in place, or say why it cannot be used.
fn validate(pack: &mut PackManifest) -> Result<(), String> {
    if pack.format != FORMAT_VERSION {
        return Err(format!(
            "This file is toolbar format {}, but this build reads format {FORMAT_VERSION}.",
            pack.format
        ));
    }

    pack.id = pack.id.trim().to_ascii_lowercase();
    if !valid_pack_id(&pack.id) {
        return Err(
            "The toolbar id must be lower-case letters, digits, dots, dashes or underscores."
                .to_string(),
        );
    }
    if RESERVED_IDS.contains(&pack.id.as_str()) {
        return Err(format!(
            "The id {} belongs to a toolbar that ships with the application.",
            pack.id
        ));
    }

    pack.name = check_text("name", &pack.name, MAX_NAME)?;
    check_optional("version", &mut pack.version, MAX_VERSION)?;
    check_optional("author", &mut pack.author, MAX_AUTHOR)?;
    check_optional("description", &mut pack.description, MAX_DESCRIPTION)?;

    check_color("background", &mut pack.style.background)?;
    check_color("text", &mut pack.style.text)?;
    check_color("accent", &mut pack.style.accent)?;
    check_color("hover", &mut pack.style.hover)?;
    check_color("active", &mut pack.style.active)?;
    check_color("line", &mut pack.style.line)?;

    if pack.icons.len() > MAX_ICONS {
        return Err(format!("A toolbar may carry at most {MAX_ICONS} icons."));
    }
    const BLOCKED: [&str; 12] = [
        "<script",
        "<style",
        "<foreignobject",
        "<iframe",
        "<object",
        "<embed",
        "<image",
        "javascript:",
        "onload",
        "onerror",
        "onclick",
        "onmouseover",
    ];
    for (command, markup) in pack.icons.iter_mut() {
        if !valid_key(command) {
            return Err(format!("{command} is not an icon name."));
        }
        let trimmed = markup.trim();
        if trimmed.len() > MAX_ICON_BYTES {
            return Err(format!(
                "The icon for {command} is larger than {} KB.",
                MAX_ICON_BYTES / 1024
            ));
        }
        let lower = trimmed.to_ascii_lowercase();
        if !lower.starts_with("<svg") && !(lower.starts_with("<?xml") && lower.contains("<svg")) {
            return Err(format!("The icon for {command} is not inline SVG markup."));
        }
        if let Some(blocked) = BLOCKED.iter().find(|needle| lower.contains(*needle)) {
            return Err(format!(
                "The icon for {command} carries {blocked}, which a toolbar may not use."
            ));
        }
        *markup = trimmed.to_string();
    }

    let mut buttons = Vec::new();
    let mut seen = BTreeSet::new();
    for token in pack.buttons.drain(..) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if token == "separator" {
            buttons.push(token.to_string());
        } else if valid_key(token) {
            if seen.insert(token.to_string()) {
                buttons.push(token.to_string());
            }
        } else {
            return Err(format!("{token} is not a toolbar button."));
        }
        if buttons.len() >= MAX_BUTTONS {
            break;
        }
    }
    pack.buttons = buttons;

    Ok(())
}

/* ------------------------------------------------------------------ files */

fn packs_folder(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("the application folder is not available: {error}"))?
        .join("toolbars");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("the toolbars folder could not be created: {error}"))?;
    Ok(dir)
}

fn read_pack(path: &Path) -> Result<PackManifest, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("could not be read: {error}"))?;
    if !metadata.is_file() {
        return Err("is not a file.".to_string());
    }
    if metadata.len() > MAX_PACK_BYTES {
        return Err(format!("is larger than {} KB.", MAX_PACK_BYTES / 1024));
    }
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("could not be read: {error}"))?;
    let mut pack: PackManifest =
        serde_json::from_str(&text).map_err(|error| format!("is not a valid toolbar: {error}"))?;
    validate(&mut pack)?;
    Ok(pack)
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// The file that holds a pack id, whatever it is named. A pack dropped into
/// the folder by hand is found as well as one this application installed.
fn find_installed(dir: &Path, id: &str) -> Option<(PathBuf, PackManifest)> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_json = path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("json"))
            .unwrap_or(false);
        if !is_json {
            continue;
        }
        if let Ok(pack) = read_pack(&path) {
            if pack.id == id {
                return Some((path, pack));
            }
        }
    }
    None
}

fn write_atomic(dir: &Path, target: &Path, pack: &PackManifest) -> Result<(), String> {
    let text = serde_json::to_string_pretty(pack)
        .map_err(|error| format!("the toolbar could not be written: {error}"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let temp = dir.join(format!(".install-{}-{stamp}.tmp", std::process::id()));
    std::fs::write(&temp, text)
        .map_err(|error| format!("the toolbar could not be written: {error}"))?;
    // Windows refuses to rename over a file that exists, so a replacement
    // removes the old copy first. A failure leaves the temp file behind and
    // the old pack in place.
    if target.exists() {
        if let Err(error) = std::fs::remove_file(target) {
            let _ = std::fs::remove_file(&temp);
            return Err(format!("the old toolbar could not be replaced: {error}"));
        }
    }
    std::fs::rename(&temp, target).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        format!("the toolbar could not be stored: {error}")
    })
}

/* --------------------------------------------------------------- commands */

#[tauri::command]
pub fn list_toolbar_packs(app: AppHandle) -> Result<PacksSnapshot, String> {
    let dir = packs_folder(&app)?;
    let shown = dir.to_string_lossy().to_string();

    let mut files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|error| format!("the toolbars folder could not be read: {error}"))?
    {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        let is_json = path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("json"))
            .unwrap_or(false);
        if is_json {
            files.push(path);
        }
    }
    files.sort_by_key(|path| file_label(path).to_lowercase());

    let mut packs = Vec::new();
    let mut problems = Vec::new();
    let mut seen = BTreeSet::new();
    for file in files {
        match read_pack(&file) {
            Ok(pack) => {
                if !seen.insert(pack.id.clone()) {
                    problems.push(PackProblem {
                        file: file_label(&file),
                        message: format!("Another toolbar already uses the id {}.", pack.id),
                    });
                    continue;
                }
                packs.push(pack);
            }
            Err(message) => problems.push(PackProblem {
                file: file_label(&file),
                message,
            }),
        }
    }
    packs.sort_by_key(|pack| pack.name.to_lowercase());

    Ok(PacksSnapshot {
        folder: shown,
        packs,
        problems,
    })
}

#[tauri::command]
pub fn install_toolbar_pack(
    app: AppHandle,
    path: String,
    replace: bool,
) -> Result<PackManifest, PackError> {
    let source = PathBuf::from(path.trim());
    if source.as_os_str().is_empty() {
        return Err(invalid("Choose a toolbar file to install."));
    }
    let is_json = source
        .extension()
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    if !is_json {
        return Err(invalid("A toolbar is installed from a .json file."));
    }

    let pack = read_pack(&source).map_err(invalid)?;
    let dir = packs_folder(&app).map_err(io)?;
    let existing = find_installed(&dir, &pack.id);

    if let Some((_, held)) = &existing {
        if !replace {
            return Err(PackError {
                kind: "exists",
                message: format!("{} is already installed.", held.name),
                name: Some(held.name.clone()),
            });
        }
    }

    // A replacement writes over the file that holds the id, even when the
    // reader dropped it in under another name. Otherwise the folder would end
    // up with two packs that claim the same id.
    let target = existing
        .map(|(path, _)| path)
        .unwrap_or_else(|| dir.join(format!("{}.json", pack.id)));

    write_atomic(&dir, &target, &pack).map_err(io)?;
    Ok(pack)
}

#[tauri::command]
pub fn remove_toolbar_pack(app: AppHandle, id: String) -> Result<(), PackError> {
    let id = id.trim().to_ascii_lowercase();
    if !valid_pack_id(&id) {
        return Err(invalid("That toolbar id is not valid."));
    }
    if RESERVED_IDS.contains(&id.as_str()) {
        return Err(PackError {
            kind: "builtin",
            message: "A toolbar that ships with the application cannot be removed.".to_string(),
            name: None,
        });
    }

    let dir = packs_folder(&app).map_err(io)?;
    if let Some((path, _)) = find_installed(&dir, &id) {
        return std::fs::remove_file(&path)
            .map_err(|error| io(format!("the toolbar could not be removed: {error}")));
    }

    Err(PackError {
        kind: "missing",
        message: "That toolbar is not installed.".to_string(),
        name: None,
    })
}

#[tauri::command]
pub fn reveal_toolbar_packs(app: AppHandle) -> Result<String, String> {
    let dir = packs_folder(&app)?;
    crate::open_in_os(&dir)?;
    Ok(dir.to_string_lossy().to_string())
}

/* ------------------------------------------------------------------ tests */

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read(value: serde_json::Value) -> Result<PackManifest, String> {
        let text = serde_json::to_string(&value).expect("a manifest");
        let mut pack: PackManifest =
            serde_json::from_str(&text).expect("a manifest of the right shape");
        validate(&mut pack)?;
        Ok(pack)
    }

    fn base() -> serde_json::Value {
        json!({
            "format": 1,
            "id": "glossy-two",
            "name": "Glossy Two",
            "version": "1.0.0",
            "author": "Someone",
            "description": "A second glow.",
            "style": { "accent": "#22e7ff", "glow": true },
            "icons": {},
            "buttons": ["resume", "stop"]
        })
    }

    #[test]
    fn a_complete_pack_reads() {
        let pack = read(base()).expect("a valid pack");
        assert_eq!(pack.id, "glossy-two");
        assert_eq!(pack.name, "Glossy Two");
        assert_eq!(pack.buttons, vec!["resume", "stop"]);
        assert!(pack.style.glow);
    }

    #[test]
    fn a_pack_without_optional_fields_still_reads() {
        let pack = read(json!({ "id": "plain", "name": "Plain" })).expect("a minimal pack");
        assert_eq!(pack.format, FORMAT_VERSION);
        assert!(pack.buttons.is_empty());
    }

    #[test]
    fn a_pack_cannot_take_a_built_in_id() {
        for id in ["default", "3d", "classic", "isometric", "blueprint"] {
            let mut value = base();
            value["id"] = json!(id);
            assert!(read(value).unwrap_err().contains("ships with"));
        }
    }

    #[test]
    fn a_newer_format_is_refused() {
        let mut value = base();
        value["format"] = json!(99);
        assert!(read(value).unwrap_err().contains("toolbar format 99"));
    }

    #[test]
    fn a_bad_colour_is_refused() {
        let mut value = base();
        value["style"] = json!({ "accent": "cyan" });
        assert!(read(value).unwrap_err().contains("hex colour"));
    }

    #[test]
    fn a_script_in_an_icon_is_refused() {
        let mut value = base();
        value["icons"] =
            json!({ "resume": "<svg viewBox=\"0 0 24 24\"><script>alert(1)</script></svg>" });
        assert!(read(value).unwrap_err().contains("<script"));
    }

    #[test]
    fn a_plain_icon_is_kept_and_trimmed() {
        let mut value = base();
        value["icons"] =
            json!({ "resume": "  <svg viewBox=\"0 0 24 24\"><path d=\"M0 0h1\"/></svg>  " });
        let pack = read(value).expect("a plain icon");
        assert!(pack.icons["resume"].starts_with("<svg"));
    }

    #[test]
    fn an_unknown_button_is_refused() {
        let mut value = base();
        value["buttons"] = json!(["resume", "not a button"]);
        assert!(read(value).unwrap_err().contains("not a toolbar button"));
    }

    #[test]
    fn a_repeated_button_is_kept_once() {
        let mut value = base();
        value["buttons"] = json!(["resume", "resume", "separator", "separator", "stop"]);
        let pack = read(value).expect("a repeated button");
        assert_eq!(
            pack.buttons,
            vec!["resume", "separator", "separator", "stop"]
        );
    }

    #[test]
    fn ids_and_keys_are_checked() {
        assert!(valid_pack_id("glossy-two"));
        assert!(valid_pack_id("3d"));
        assert!(valid_pack_id("a.b_c"));
        assert!(!valid_pack_id("Glossy"));
        assert!(!valid_pack_id("-glossy"));
        assert!(!valid_pack_id(""));
        assert!(valid_key("delete-completed"));
        assert!(!valid_key("delete completed"));
        assert!(!valid_key("Delete"));
    }

    #[test]
    fn colours_are_checked() {
        assert!(valid_color("#fff"));
        assert!(valid_color("#22e7ff80"));
        assert!(!valid_color("#22e7f"));
        assert!(!valid_color("22e7ff"));
        assert!(!valid_color("rgb(0 0 0)"));
    }
}
