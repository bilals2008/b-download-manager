//! The network adapter the connections leave through, and the kill switch that
//! stops the downloads when it goes away.
//!
//! The reader chooses an adapter (a VPN, usually) and every client the
//! application builds leaves through it. The choice is kept by the adapter's
//! identity, not by its name, so a renamed connection is still the one that
//! was chosen.
//!
//! When the adapter goes away — a VPN that was closed, a cable that was
//! pulled — the route becomes "down": new downloads are refused and the
//! running ones stop with one sentence saying why. That is the kill switch.
//! It fails closed: the traffic never quietly takes the normal route.

use std::sync::{Condvar, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use idm_core::downloader::Status;
use idm_core::net::adapter::{self, Route};

use crate::state::AppState;

/// The event the windows listen to: the chosen adapter's state changed.
pub const ADAPTER_EVENT: &str = "network://adapter";

/// An adapter of this computer, as the settings page shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterInfo {
    /// The system's identity for it: what the settings file keeps.
    pub id: String,
    /// Its name as the system shows it ("Ethernet", "ProtonVPN").
    pub name: String,
    /// Its first address, empty when it has none.
    pub address: String,
    /// Whether the system says it is up.
    pub up: bool,
}

/// The chosen adapter's state, as the status bar shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterState {
    /// The adapter's name; empty when no adapter is chosen.
    pub label: String,
    /// "none", "connected" or "lost".
    pub state: String,
}

/// The adapters of this computer, for the settings page.
#[tauri::command]
pub fn network_adapters() -> Vec<AdapterInfo> {
    adapter::adapters()
        .into_iter()
        .map(|adapter| AdapterInfo {
            id: adapter.id.clone(),
            name: adapter.name.clone(),
            address: adapter
                .v4
                .first()
                .map(|address| address.to_string())
                .or_else(|| adapter.v6.first().map(|address| address.to_string()))
                .unwrap_or_default(),
            up: adapter.up,
        })
        .collect()
}

/// The adapter the reader chose, and whether it is carrying anything.
#[tauri::command]
pub fn network_state() -> AdapterState {
    state_of(&adapter::route())
}

/// Choose the adapter every connection leaves through.
///
/// An empty choice means the system's own route, which is what the reader
/// had before. A choice that is not there is refused by the engine when the
/// route is built, and the downloads stop rather than falling back.
#[tauri::command]
pub fn set_network_adapter(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<AdapterState, String> {
    let choice = id.trim().to_string();
    state.modify_settings(|settings| settings.network_adapter = choice);
    Ok(state_of(&adapter::route()))
}

/// The state of a route, in the words the window shows.
fn state_of(route: &Route) -> AdapterState {
    match route {
        Route::Any => AdapterState {
            label: String::new(),
            state: "none".to_string(),
        },
        Route::Bound(binding) => AdapterState {
            label: binding.label.clone(),
            state: "connected".to_string(),
        },
        Route::Down { label } => AdapterState {
            label: label.clone(),
            state: "lost".to_string(),
        },
    }
}

/// The flag the watcher waits on: the system said something about the
/// adapters.
static CHANGED: Mutex<bool> = Mutex::new(false);
static CHANGE: Condvar = Condvar::new();

/// Watch the adapters, and fail closed when the chosen one goes away.
///
/// The system tells us when an adapter changes; we look again and put the
/// route in force. A chosen adapter that is gone is a [`Route::Down`]: the
/// running downloads stop with one sentence, and new ones are refused.
pub fn watch(app: AppHandle) {
    let _ = notify();
    std::thread::spawn(move || loop {
        let mut changed = CHANGED.lock().expect("change lock poisoned");
        while !*changed {
            changed = CHANGE
                .wait(changed)
                .expect("change lock poisoned");
        }
        *changed = false;
        drop(changed);
        check(&app);
    });
}

/// Look at the adapters again and put the route in force.
fn check(app: &AppHandle) {
    let state = app.state::<AppState>();
    let choice = state.engine_settings().network_adapter;
    let route = adapter::route_for(&choice, &adapter::adapters());
    let was = adapter::route();
    adapter::set_route(route.clone());
    if same_route(&was, &route) {
        return;
    }
    let label = route.label().unwrap_or_default().to_string();
    if let Route::Down { .. } = route {
        // The adapter is gone: the running downloads stop, and they say why.
        let reason = format!(
            "the network adapter {label} is not connected, so the download was stopped"
        );
        for download in state.list() {
            if download.status() == Status::Running {
                download.fail_with(reason.clone());
                crate::actions::announce(&app, &download);
            }
        }
        idm_core::log::write("warn", &reason);
    }
    // The window shows the state, so the reader sees an adapter that is lost
    // instead of having to infer it from a download that stopped.
    let _ = app.emit(ADAPTER_EVENT, state_of(&route));
}

/// Whether two routes say the same thing.
fn same_route(one: &Route, other: &Route) -> bool {
    match (one, other) {
        (Route::Any, Route::Any) => true,
        (Route::Bound(a), Route::Bound(b)) => a.label == b.label && a.v4 == b.v4 && a.v6 == b.v6,
        (Route::Down { label: a }, Route::Down { label: b }) => a == b,
        _ => false,
    }
}

/// Ask the system to tell us when the adapters change.
#[cfg(windows)]
fn notify() -> Result<(), String> {
    use std::sync::OnceLock;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE, NotifyIpInterfaceChange,
    };
    use windows_sys::Win32::Networking::WinSock::AF_UNSPEC;

    /// The handle the notification was registered with; the system keeps it
    /// for as long as the application runs.
    static HANDLE: OnceLock<usize> = OnceLock::new();

    unsafe extern "system" fn changed(
        _context: *const std::ffi::c_void,
        _row: *const MIB_IPINTERFACE_ROW,
        _kind: MIB_NOTIFICATION_TYPE,
    ) {
        let mut flag = CHANGED.lock().expect("change lock poisoned");
        *flag = true;
        CHANGE.notify_all();
    }

    unsafe {
        let mut handle: windows_sys::Win32::Foundation::HANDLE = std::ptr::null_mut();
        let result = NotifyIpInterfaceChange(
            AF_UNSPEC,
            Some(changed),
            std::ptr::null(),
            false,
            &mut handle,
        );
        if result != windows_sys::Win32::Foundation::ERROR_SUCCESS {
            return Err(format!("the network watcher could not be started: {result}"));
        }
        let _ = HANDLE.set(handle as usize);
    }
    Ok(())
}

/// Ask the system to tell us when the adapters change.
#[cfg(not(windows))]
fn notify() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The adapters of this computer are listed with an identity each, and
    /// the identity is what the settings file keeps.
    #[test]
    fn the_computers_adapters_are_listed_with_an_identity() {
        let adapters = network_adapters();
        if cfg!(windows) {
            assert!(!adapters.is_empty(), "Windows always has adapters");
            for adapter in &adapters {
                assert!(!adapter.id.is_empty(), "{:?} has no identity", adapter.name);
                assert!(!adapter.name.is_empty(), "{:?} has no name", adapter.id);
            }
            // The identities are what the choice is made of, so two adapters
            // may not share one.
            let mut ids: Vec<&str> = adapters.iter().map(|a| a.id.as_str()).collect();
            ids.sort_unstable();
            let count = ids.len();
            ids.dedup();
            assert_eq!(ids.len(), count, "two adapters share an identity");
        }
    }

    /// The watcher only acts when the route really changed, so an address
    /// that comes and goes does not stop the world for nothing.
    #[test]
    fn the_watcher_acts_only_when_the_route_changed() {
        let bound = Route::Bound(idm_core::net::adapter::Binding {
            label: "Wi-Fi".into(),
            v4: Some("192.168.1.11".parse().unwrap()),
            ..Default::default()
        });
        let same = Route::Bound(idm_core::net::adapter::Binding {
            label: "Wi-Fi".into(),
            v4: Some("192.168.1.11".parse().unwrap()),
            ..Default::default()
        });
        let other = Route::Bound(idm_core::net::adapter::Binding {
            label: "Ethernet".into(),
            v4: Some("10.0.0.2".parse().unwrap()),
            ..Default::default()
        });
        assert!(same_route(&bound, &same), "the same adapter is the same route");
        assert!(!same_route(&bound, &other), "another adapter is another route");
        assert!(!same_route(&bound, &Route::Any));
        assert!(!same_route(
            &Route::Down {
                label: "Wi-Fi".into()
            },
            &Route::Down {
                label: "Ethernet".into()
            }
        ));
    }

    /// The state of a route, in the words the window shows.
    #[test]
    fn the_state_of_a_route_is_what_the_window_shows() {        let none = state_of(&Route::Any);
        assert_eq!(none.state, "none");
        assert!(none.label.is_empty());
        let down = state_of(&Route::Down {
            label: "VPN".into(),
        });
        assert_eq!(down.state, "lost");
        assert_eq!(down.label, "VPN");
        let bound = state_of(&Route::Bound(idm_core::net::adapter::Binding {
            label: "Ethernet".into(),
            ..Default::default()
        }));
        assert_eq!(bound.state, "connected");
        assert_eq!(bound.label, "Ethernet");
    }
}
