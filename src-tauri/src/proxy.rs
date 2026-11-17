//! The proxy auto-config script: the file the reader points at, and the
//! script itself.
//!
//! A PAC file is a document like any other: the application fetches it (with
//! the same DNS and adapter the rest of the traffic uses) and hands the text
//! to the engine, which asks it for every address. The route is therefore
//! decided per address, the way a browser decides it — not once for the whole
//! run.
//!
//! The script is fetched again when the settings are applied, when the reader
//! asks for it, and now and then while the application runs, because a PAC
//! file is allowed to change its mind.

use std::time::Duration;

use serde::Serialize;
use tauri::State;

use idm_core::net::pac;

use crate::state::AppState;

/// How often the script is fetched again while the application runs.
const REFETCH: Duration = Duration::from_secs(600);

/// What the reader knows about the script.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptState {
    /// The address the script is fetched from; empty when there is none.
    pub url: String,
    /// Whether the script is in force right now.
    pub loaded: bool,
    /// The first line of the script's answer, or the reason it could not
    /// answer.
    pub note: String,
}

/// Fetch the script at `url` and put it in force.
///
/// The file is fetched before it is stored: an address that does not answer
/// is refused here rather than leaving every download without a route.
#[tauri::command]
pub async fn set_proxy_script(
    state: State<'_, AppState>,
    url: String,
) -> Result<ScriptState, String> {
    let trimmed = url.trim().to_string();
    if trimmed.is_empty() {
        pac::set_script(None);
        state.modify_settings(|settings| settings.proxy_script = String::new());
        return Ok(script_state(&state));
    }
    let script = fetch(&trimmed).await?;
    pac::set_script(Some(script));
    state.modify_settings(|settings| settings.proxy_script = trimmed);
    Ok(script_state(&state))
}

/// Fetch the script again, now.
#[tauri::command]
pub async fn reload_proxy_script(state: State<'_, AppState>) -> Result<ScriptState, String> {
    let url = state.engine_settings().proxy_script;
    if url.trim().is_empty() {
        return Ok(script_state(&state));
    }
    let script = fetch(url.trim()).await?;
    pac::set_script(Some(script));
    Ok(script_state(&state))
}

/// What the reader knows about the script.
#[tauri::command]
pub fn proxy_script_state(state: State<'_, AppState>) -> ScriptState {
    script_state(&state)
}

/// The state of the script, in the words the window shows.
fn script_state(state: &AppState) -> ScriptState {
    let url = state.engine_settings().proxy_script;
    let loaded = pac::script().is_some();
    let note = if loaded {
        "The script decides per address.".to_string()
    } else if url.trim().is_empty() {
        "No script. The proxy address below is used for every download.".to_string()
    } else {
        "The script could not be fetched.".to_string()
    };
    ScriptState { url, loaded, note }
}

/// Fetch the script at `url`.
async fn fetch(url: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| {
        "Use a full address, for example http://proxy.example.org/proxy.pac.".to_string()
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("The script must be fetched over http or https.".to_string());
    }
    // The script is fetched with the engine's own client, so the encrypted
    // DNS and the chosen adapter apply to it as they do to everything else.
    let client = idm_core::net::client_builder(None, None, None)?
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("the client could not be made: {error}"))?;
    let answer = client
        .get(parsed)
        .send()
        .await
        .map_err(|error| format!("the script could not be fetched: {error}"))?;
    if !answer.status().is_success() {
        return Err(format!(
            "the script could not be fetched: {}",
            answer.status()
        ));
    }
    let text = answer
        .text()
        .await
        .map_err(|error| format!("the script could not be read: {error}"))?;
    if text.trim().is_empty() {
        return Err("the script is empty".to_string());
    }
    Ok(text)
}

/// Fetch the script when the settings are applied, and now and then while the
/// application runs.
pub fn start(app: tauri::AppHandle) {
    use tauri::Manager;
    let url = app.state::<AppState>().engine_settings().proxy_script;
    if url.trim().is_empty() {
        return;
    }
    std::thread::spawn(move || loop {
        let url = app.state::<AppState>().engine_settings().proxy_script;
        if !url.trim().is_empty() {
            // The fetch is blocking, so it is done on the runtime's own
            // threads rather than in this one.
            let address = url.trim().to_string();
            let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = done.clone();
            tauri::async_runtime::spawn(async move {
                match fetch(&address).await {
                    Ok(script) => pac::set_script(Some(script)),
                    Err(error) => {
                        idm_core::log::write("warn", &format!("the proxy script: {error}"));
                    }
                }
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
            });
            // The thread waits for the answer, so the script is in force
            // before the next one is due.
            while !done.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        std::thread::sleep(REFETCH);
    });
}
