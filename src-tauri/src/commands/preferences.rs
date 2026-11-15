//! The saved choices: connections, tray, browser policy, categories.

use std::path::PathBuf;

use tauri::{AppHandle, State};

use idm_core::transfer::Source;

use crate::bridge;
use crate::prefs::{self, DesktopPreferences};
use crate::settings_store;
use crate::state::{self, AppState, BrowserPolicy};
use crate::tray;

/// The proxy that new downloads go through. `None` means no proxy.
#[tauri::command]
pub(crate) fn set_proxy(state: State<'_, AppState>, url: Option<String>) -> Result<(), String> {
    let cleaned = match url {
        Some(value) => validate_proxy(value)?,
        None => String::new(),
    };
    state.modify_settings(|settings| settings.proxy_url = cleaned.clone());
    Ok(())
}

/// Check a proxy address before it is stored or used.
///
/// Only the schemes the engine can actually dial are accepted, the port has to
/// make sense, and a username or password is refused: the preference lives in
/// plain local storage, so it must never carry a credential. Proxy
/// authentication is not part of this build.
#[tauri::command]
pub(crate) fn validate_proxy(url: String) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|_| "Use a full address, for example socks5://127.0.0.1:1080.".to_string())?;
    let scheme = parsed.scheme();
    if !matches!(scheme, "http" | "https" | "socks5" | "socks5h") {
        return Err(format!(
            "{scheme} is not a proxy this build can use. Use http, https, socks5 or socks5h."
        ));
    }
    if parsed.host_str().is_none() {
        return Err("The proxy address has no host.".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(
            "Proxy authentication is not supported. Remove the username and password.".to_string(),
        );
    }
    // The engine builds its client from this exact string, so refuse what it
    // would refuse instead of storing an address that fails at the first
    // download.
    Source::parse("https://example.com/", Some(trimmed), None)
        .map_err(|error| format!("The proxy address is not usable: {error}"))?;
    Ok(trimmed.to_string())
}

/// The number of connections one site may use.
#[tauri::command]
pub(crate) fn set_site_connections(state: State<'_, AppState>, host: String, connections: u32) {
    state.limits().sites.set(&host, connections);
}

/// The number of connections one site may use, when there is an override.
#[tauri::command]
pub(crate) fn site_connections(state: State<'_, AppState>, host: String) -> Option<u32> {
    state.limits().sites.get(&host)
}

/// Forget what this host allowed last time, so the next download starts from
/// the reader's own count instead of the learned cap.
#[tauri::command]
pub(crate) fn reset_learned_host(state: State<'_, AppState>, host: String) {
    let host = host.trim();
    if !host.is_empty() {
        state.limits().learned.reset(host);
    }
}

/// How many connections the whole application may have open at once.
#[tauri::command]
pub(crate) fn set_max_connections(state: State<'_, AppState>, max: u32) -> u32 {
    state.modify_settings(|settings| settings.max_connections = max);
    state.engine_settings().max_connections
}

/// The choices the tray and the close button follow.
#[tauri::command]
pub(crate) fn desktop_preferences(state: State<'_, AppState>) -> DesktopPreferences {
    state.preferences()
}

/// Change one saved choice and write the file again.
///
/// The whole object goes back and forth, so two choices that belong together
/// cannot be saved one after the other and leave the file between them.
pub(crate) fn rewrite_preferences(
    state: &AppState,
    change: impl FnOnce(&mut DesktopPreferences),
) -> DesktopPreferences {
    let mut preferences = state.preferences();
    change(&mut preferences);
    let saved = preferences.clone();
    state.set_preferences(preferences);
    state.save_preferences();
    saved
}

/// Whether the minimize button hides the window into the tray.
#[tauri::command]
pub(crate) fn set_minimize_to_tray(state: State<'_, AppState>, on: bool) -> bool {
    rewrite_preferences(&state, |preferences| preferences.minimize_to_tray = on).minimize_to_tray
}

/// Whether closing the window keeps the application in the tray.
#[tauri::command]
pub(crate) fn set_close_to_tray(state: State<'_, AppState>, on: bool) -> bool {
    rewrite_preferences(&state, |preferences| preferences.close_to_tray = on).close_to_tray
}

/// Take the tray icon out of the notification area, or put it back.
///
/// The answer is the choice as saved. Whether the icon is really there is what
/// `desktop_preferences` says next time the window reads it; a shell that
/// refuses an icon is written down in the log.
#[tauri::command]
pub(crate) fn set_tray_hidden(app: AppHandle, state: State<'_, AppState>, on: bool) -> bool {
    rewrite_preferences(&state, |preferences| preferences.tray_hidden = on);
    tray::set_visible(&app, !on);
    on
}

/// Whether the tray icon follows the appearance of the window.
#[tauri::command]
pub(crate) fn set_tray_dark_mode(app: AppHandle, state: State<'_, AppState>, on: bool) -> bool {
    rewrite_preferences(&state, |preferences| preferences.tray_dark_mode = on);
    tray::repaint(&app);
    on
}

/// Which appearance the window is drawn in, so the icon can follow it.
///
/// The webview owns the appearance; the tray only knows what it is told. A
/// report of the appearance it already has repaints nothing.
#[tauri::command]
pub(crate) fn set_tray_appearance(app: AppHandle, state: State<'_, AppState>, light: bool) -> bool {
    if state.appearance_is_light() == light {
        return light;
    }
    state.set_appearance_is_light(light);
    tray::repaint(&app);
    light
}

/// The protocols a copied address has to start with to be offered as a
/// download. The list comes back as it was stored, so the window can show what
/// the backend will actually match.
#[tauri::command]
pub(crate) fn set_url_protocols(state: State<'_, AppState>, protocols: Vec<String>) -> Vec<String> {
    rewrite_preferences(&state, |preferences| {
        preferences.url_protocols = prefs::clean_url_protocols(&protocols);
    })
    .url_protocols
}

/// Whether a finished download gets its own window.
#[tauri::command]
pub(crate) fn set_show_complete_dialog(state: State<'_, AppState>, on: bool) -> bool {
    state.modify_settings(|settings| settings.show_complete_dialog = on);
    state.show_complete_dialog()
}

/// Whether a new download opens its progress window.
#[tauri::command]
pub(crate) fn set_show_progress_on_start(state: State<'_, AppState>, on: bool) -> bool {
    state.modify_settings(|settings| settings.show_progress_on_start = on);
    state.show_progress_on_start()
}

/// Whether a video download opens the quality window that follows it.
#[tauri::command]
pub(crate) fn set_show_video_window_on_start(state: State<'_, AppState>, on: bool) -> bool {
    state.modify_settings(|settings| settings.show_video_window_on_start = on);
    state.show_video_window_on_start()
}

/// The connections a browser hand-over gets, since it has no dialog to ask.
#[tauri::command]
pub(crate) fn set_browser_connections(state: State<'_, AppState>, count: u32) -> u32 {
    state.modify_settings(|settings| settings.default_connections = count);
    state.browser_connections()
}

/// Whether a finished archive is unpacked automatically.
#[tauri::command]
pub(crate) fn set_extract_after_done(state: State<'_, AppState>, on: bool) -> bool {
    state.modify_settings(|settings| settings.extract_after_done = on);
    state.engine_settings().extract_after_done
}

/// Whether the archive is removed after it was unpacked cleanly.
#[tauri::command]
pub(crate) fn set_delete_archive_after_extract(state: State<'_, AppState>, on: bool) -> bool {
    state.modify_settings(|settings| settings.delete_archive_after_extract = on);
    state.engine_settings().delete_archive_after_extract
}

/// Whether the clipboard is watched for download links.
#[tauri::command]
pub(crate) fn set_clipboard_capture(state: State<'_, AppState>, on: bool) -> bool {
    state.modify_settings(|settings| settings.clipboard_capture = on);
    state.clipboard_capture()
}

/// Register or remove the operating system's startup entry.
#[tauri::command]
pub(crate) fn set_launch_on_startup(app: AppHandle, on: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();
    if on {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|error| error.to_string())?;
    manager.is_enabled().map_err(|error| error.to_string())
}

/// The capture rules the extension follows while this application runs.
///
/// The lists are cleaned here as well as in the window: this is the value the
/// extension acts on, so it is the boundary that has to be safe.
#[tauri::command]
pub(crate) fn set_browser_policy(state: State<'_, AppState>, policy: BrowserPolicy) -> BrowserPolicy {
    let policy = normalize_policy(policy);
    state.modify_settings(|settings| settings.browser_policy = policy.clone());
    policy
}

/// The appearance of the floating video download button the extension draws.
#[tauri::command]
pub(crate) fn set_video_button_settings(state: State<'_, AppState>, video_button: state::VideoButtonSettings) {
    state.modify_settings(|settings| settings.video_button = video_button.clone());
}

/// The appearance of the floating video download button, served to the
/// extension over the loopback bridge.
#[tauri::command]
pub(crate) fn video_button_settings(state: State<'_, AppState>) -> state::VideoButtonSettings {
    state.video_button_settings()
}

/// Publish the appearance in force, so the browser panel uses the same palette.
#[tauri::command]
pub(crate) fn set_browser_theme(
    state: State<'_, AppState>,
    theme: String,
    light: bool,
    palette: std::collections::HashMap<String, String>,
) {
    state.set_browser_theme(state::BrowserTheme {
        theme,
        light,
        palette,
    });
}

/// What the window can say about the browser bridge without guessing.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserStatus {
    /// Whether the bridge bound its port.
    bridge_running: bool,
    port: u16,
    api: u32,
    /// The last extension request, in milliseconds since the Unix epoch.
    /// `None` means the extension has not reached it yet.
    last_contact_ms: Option<u64>,
}

#[tauri::command]
pub(crate) fn browser_status(state: State<'_, AppState>) -> BrowserStatus {
    BrowserStatus {
        bridge_running: state.bridge_running(),
        port: bridge::PORT,
        api: bridge::API_VERSION,
        last_contact_ms: state.last_extension_contact().map(|seconds| seconds * 1000),
    }
}

/// The longest list one policy carries, matching the window's limit.
const MAX_RULES: usize = 64;

pub(crate) fn normalize_policy(mut policy: BrowserPolicy) -> BrowserPolicy {
    policy.types = clean_list(policy.types, clean_file_type);
    policy.skip_sites = clean_list(policy.skip_sites, clean_site);
    policy
}

pub(crate) fn clean_list(values: Vec<String>, clean: fn(&str) -> Option<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in values {
        let Some(cleaned) = clean(&value) else {
            continue;
        };
        if !out.contains(&cleaned) {
            out.push(cleaned);
        }
        if out.len() >= MAX_RULES {
            break;
        }
    }
    out
}

pub(crate) fn clean_file_type(value: &str) -> Option<String> {
    let cleaned = value
        .trim()
        .to_ascii_lowercase()
        .trim_start_matches('*')
        .trim_start_matches('.')
        .to_string();
    if cleaned.is_empty() || cleaned.len() > 16 {
        return None;
    }
    let mut chars = cleaned.chars();
    if !chars.next()?.is_ascii_alphanumeric() {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '_' | '-')) {
        return None;
    }
    Some(cleaned)
}

/// A host out of anything the reader may have typed, without a scheme or a
/// path. Junk becomes `None`.
pub(crate) fn clean_site(value: &str) -> Option<String> {
    let text = value.trim().trim_start_matches('*').trim_start_matches('.');
    if text.is_empty() {
        return None;
    }
    let with_scheme = if text.contains("://") {
        text.to_string()
    } else {
        format!("http://{text}")
    };
    let url = reqwest::Url::parse(&with_scheme).ok()?;
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" {
        return Some(host);
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return None;
    }
    let valid = labels.iter().all(|label| {
        !label.is_empty()
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    });
    valid.then_some(host)
}

/// The folder a download handed over by the browser is saved to.
///
/// The options window owns this preference and pushes it here, so a link
/// caught by the extension lands where the user expects. An empty folder
/// means the system Downloads folder.
#[tauri::command]
pub(crate) fn set_browser_folder(state: State<'_, AppState>, folder: String) -> String {
    let trimmed = folder.trim().to_string();
    state.modify_settings(|settings| settings.default_folder = trimmed.clone());
    trimmed
}

/// The folder each category saves to.
///
/// The options window owns this list; the backend applies it, so a browser
/// hand-over lands where the reader expects. The folders for temporary,
/// staged bytes are not a setting: the application owns them and setup
/// resolved them before this command can be reached, so the window cannot
/// send a download's parts back into the user's way. The category rules are
/// written to the engine's settings file, so a restart restores them before
/// any window opens.
#[tauri::command]
pub(crate) fn set_category_rules(
    state: State<'_, AppState>,
    categories: Vec<state::CategoryRule>,
) -> Vec<state::CategoryRule> {
    let rules = normalize_category_rules(categories);
    state.modify_settings(|settings| settings.categories = rules.clone());
    rules
}

/// The engine's settings as the settings file holds them, for a window that
/// opens as the cache's owner rather than on stale values.
///
/// `None` when there is no file yet: a first run or an upgrade has the
/// webview's own copy as the seed, and the window must not mistake the
/// defaults for what the reader chose.
#[tauri::command]
pub(crate) fn saved_settings(state: State<'_, AppState>) -> Option<settings_store::EngineSettings> {
    state
        .settings_file_exists()
        .then(|| state.engine_settings())
}

/// Clean the rules a window sends. A name is required, the extensions are the
/// engine's file types, and only an absolute folder is kept: a bad rule is
/// dropped rather than failing the push, so one typo cannot stop the rest.
pub(crate) fn normalize_category_rules(rules: Vec<state::CategoryRule>) -> Vec<state::CategoryRule> {
    let mut out = Vec::new();
    for rule in rules {
        let name = rule.name.trim();
        if name.is_empty() {
            continue;
        }
        let mut extensions: Vec<String> = Vec::new();
        for extension in rule.extensions {
            let Some(cleaned) = clean_file_type(&extension) else {
                continue;
            };
            if !extensions.contains(&cleaned) {
                extensions.push(cleaned);
            }
            if extensions.len() >= MAX_RULES {
                break;
            }
        }
        let folder = rule.folder.trim();
        let folder = if folder.is_empty() || !PathBuf::from(folder).is_absolute() {
            String::new()
        } else {
            folder.to_string()
        };
        out.push(state::CategoryRule {
            name: name.to_string(),
            extensions,
            folder,
            builtin: rule.builtin,
            fallback: rule.fallback,
        });
        if out.len() >= MAX_RULES {
            break;
        }
    }
    out
}
