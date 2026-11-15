//! The debrid accounts, and the links they unlock.
//!
//! A key lives in the credential store of the operating system and nowhere
//! else: not in the settings, not in the index of the downloads and not in
//! a log line. The window is shown the name of the account and how much
//! paid time is left, and nothing else.
//!
//! The accounts themselves, the request budget of each service and the
//! switch that unlocks every matching link live in the crate, so every part
//! of the application shares one budget and one set of accounts.

use serde::Serialize;

use idm_debrid::{AccountState, Secret, ServiceKind};

/// What the settings page shows about one service.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    /// The short name of the service ("real-debrid").
    pub id: String,
    /// The name the service writes on itself ("Real-Debrid").
    pub name: String,
    /// Whether the reader is signed in.
    pub signed_in: bool,
    /// Whether the account may use the paid features.
    pub premium: bool,
    /// The name the service gave the account.
    pub user: Option<String>,
    /// The paid days left.
    pub premium_days: Option<u64>,
    /// The page where the reader finds the key of the account.
    pub token_page: String,
}

/// The service of a short name, when the name is one of ours.
fn kind_of(id: &str) -> Result<ServiceKind, String> {
    ServiceKind::from_id(id).ok_or_else(|| "unknown service".to_string())
}

/// The session of the debrid accounts.
fn session() -> Result<&'static idm_debrid::Session, String> {
    idm_debrid::Session::shared().map_err(|error| error.sentence())
}

/// The services the app knows, and what the reader is signed in to.
#[tauri::command]
pub async fn debrid_services() -> Vec<ServiceInfo> {
    let Ok(session) = session() else {
        return Vec::new();
    };
    ServiceKind::ALL
        .into_iter()
        .map(|kind| {
            let state = session.debrid().state(kind);
            ServiceInfo {
                id: kind.id().to_string(),
                name: kind.name().to_string(),
                signed_in: session.debrid().accounts().is_signed_in(kind),
                premium: state.as_ref().is_some_and(|state| state.premium),
                user: state.as_ref().map(|state| state.user.clone()),
                premium_days: state.and_then(|state| state.premium_days),
                token_page: kind.token_page().to_string(),
            }
        })
        .collect()
}

/// Sign in to a service with the key the reader pasted.
///
/// The key is checked against the service before it is kept, and it is kept
/// in the credential store of the operating system. It is never sent back
/// to the window and never written in a message.
#[tauri::command]
pub async fn debrid_sign_in(kind: String, token: String) -> Result<AccountState, String> {
    let kind = kind_of(&kind)?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("the key is empty.".to_string());
    }
    let session = session()?;
    let info = session
        .debrid()
        .sign_in_with_key(kind, Secret::new(token.clone()))
        .await
        .map_err(|error| idm_debrid::scrub(&error.sentence(), &token))?;
    Ok(AccountState::new(kind, &info))
}

/// Sign out of a service, and forget its key.
#[tauri::command]
pub async fn debrid_sign_out(kind: String) -> Result<(), String> {
    let kind = kind_of(&kind)?;
    let session = session()?;
    session
        .debrid()
        .sign_out(kind)
        .map_err(|error| error.sentence())
}

/// The account of a service, without its key.
#[tauri::command]
pub async fn debrid_account(kind: String) -> Result<AccountState, String> {
    let kind = kind_of(&kind)?;
    let session = session()?;
    if !session.debrid().accounts().is_signed_in(kind) {
        return Err(format!("the app is not signed in to {}.", kind.name()));
    }
    session
        .debrid()
        .account_state(kind)
        .await
        .map_err(|error| error.sentence())
}

/// Check that the key of a service still works.
///
/// A service that says the app is asking too often reads as a sentence
/// about slowing down, not as a failed download.
#[tauri::command]
pub async fn debrid_test(kind: String) -> Result<(), String> {
    let kind = kind_of(&kind)?;
    let session = session()?;
    if !session.debrid().accounts().is_signed_in(kind) {
        return Err(format!("the app is not signed in to {}.", kind.name()));
    }
    session
        .debrid()
        .account_info(kind)
        .await
        .map(|_| ())
        .map_err(|error| error.sentence())
}

/// The direct link a service gives for a host link.
///
/// `None` means that no account of the app could take the link, so the
/// link is fetched as it is.
#[tauri::command]
pub async fn debrid_unrestrict(url: String) -> Result<Option<String>, String> {
    let session = session()?;
    let link = session
        .unlock(url.trim(), true)
        .await
        .map_err(|error| error.sentence())?;
    Ok(link.map(|link| link.url))
}

/// Turn the automatic switch on or off.
///
/// With the switch on, every link a service of the app can take is
/// unlocked before the download starts. The choice is kept in the engine's
/// settings file, so a browser hand-over obeys it with no window open.
#[tauri::command]
pub async fn debrid_set_auto(
    state: tauri::State<'_, crate::state::AppState>,
    enabled: bool,
) -> Result<(), String> {
    if let Ok(session) = session() {
        session.set_automatic(enabled);
    }
    state.modify_settings(|settings| {
        settings.debrid_auto_unlock = enabled;
    });
    state.save_engine_settings();
    Ok(())
}

/// Whether every host link a signed-in service can take is unlocked without
/// being asked.
#[tauri::command]
pub fn debrid_auto(state: tauri::State<'_, crate::state::AppState>) -> bool {
    state.engine_settings().debrid_auto_unlock
}

/// Remember that the reader asked for this link to be unlocked.
///
/// The add dialog asks for the link before the download starts, so the
/// start path knows the link is one the reader wants unlocked.
#[tauri::command]
pub async fn debrid_ask_unlock(url: String) -> Result<(), String> {
    let session = session()?;
    session.ask(&url);
    Ok(())
}

/// Unlock a link before the engine is asked to start it.
///
/// `automatic` is the engine's own switch: with it on, every link a
/// signed-in service can take is unlocked without being asked.
///
/// The answer of the service is waited for on a thread of the debrid
/// module's own, with a deadline. The start itself runs on the application's
/// runtime, and a service that never answers must not be able to hold it:
/// the reader asked for a download, and the link can be fetched without the
/// service.
pub fn prepare(url: &str, automatic: bool) -> Result<Option<idm_debrid::Prepared>, String> {
    if automatic {
        if let Ok(session) = session() {
            session.set_automatic(true);
        }
    }
    // A run that never touched the debrid accounts never opens their
    // credential store.
    if !idm_debrid::Session::exists() {
        return Ok(None);
    }
    let session = session()?;
    let asked = session.asked(url);
    let url = url.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    // The thread is the module's own, and the runtime on it is too: the
    // application must not have to drive the service that answers.
    std::thread::spawn(move || {
        let answer = runtime().block_on(session.prepare(&url));
        let _ = tx.send(answer);
    });
    let prepared = match rx.recv_timeout(ANSWER_WAIT) {
        Ok(answer) => answer,
        Err(_) => {
            // A service that does not answer is not a reason to refuse the
            // download. The link is fetched as it is.
            idm_core::log::write(
                "warn",
                "the debrid services did not answer in time; the link is fetched as it is",
            );
            return Ok(None);
        }
    };
    match prepared {
        Ok(prepared) => Ok(prepared),
        Err(error) => {
            // An account without paid time left is always a refusal: a slow
            // download is not what the reader asked for. A service that is
            // down is not: the reader asked for this download, and the link
            // can still be fetched as it is.
            if asked && !matches!(error, idm_debrid::DebridError::NotPremium { .. }) {
                return Ok(None);
            }
            Err(error.sentence())
        }
    }
}

/// How long the debrid services may hold a start.
///
/// The reader is waiting on the button that says "Starting…", so a service
/// that does not answer may hold the start for this long and no longer.
const ANSWER_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// A runtime of the debrid module's own.
///
/// The services are asked over the network, so they need a runtime of their
/// own: the application's runtime is busy with the start that is waiting for
/// the answer, and a task of that runtime cannot be driven while the start
/// holds it.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the runtime of the debrid services")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A link the reader did not ask to unlock has to be prepared at once.
    ///
    /// The add dialog builds the session when it opens, so by the time the
    /// reader presses Download the session is there and the start must not
    /// wait for a service that has nothing to do with the link.
    #[test]
    fn a_link_nobody_asked_for_is_prepared_at_once() {
        // The window opens the add dialog and asks the services who the
        // reader is signed in to; that is what builds the session.
        let _ = tauri::async_runtime::block_on(debrid_services());

        let started = Instant::now();
        let prepared = prepare(
            "https://raw.githubusercontent.com/microsoft/vscode/main/LICENSE.txt",
            false,
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the start waited {}s for a service that has nothing to do with the link",
            started.elapsed().as_secs()
        );
        assert!(
            prepared.expect("a link nobody asked for is prepared").is_none(),
            "the link is fetched as it is"
        );
    }

    /// A service may hold the start for a while, never for minutes.
    ///
    /// The reader is looking at a button that says "Starting…", so the wait
    /// is short and it is short on purpose.
    #[test]
    fn a_service_may_not_hold_a_start_for_long() {
        assert!(
            ANSWER_WAIT <= std::time::Duration::from_secs(15),
            "the start waits {ANSWER_WAIT:?}, which is too long for a reader to sit and wait"
        );
    }
}
