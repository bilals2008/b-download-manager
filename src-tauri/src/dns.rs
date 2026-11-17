//! The DNS the engine asks names with, and the settings window's page for it.
//!
//! One resolver governs the whole application: the one the reader chose —
//! the system's, DNS over HTTPS, DNS over TLS or a NextDNS profile — is
//! built when the settings are applied and every client built afterwards
//! looks names up through it. Nothing here asks a name except the test
//! button, which asks one name so the reader can see whether the resolver
//! they chose answers at all.

use serde::Serialize;

use idm_core::net::dns::DnsSettings;

/// The name the test button asks for. `example.com` is reserved for exactly
/// this: it exists, and it is nobody's.
const TEST_NAME: &str = "example.com";

/// What the test button found.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsTest {
    /// How long the lookup took, in milliseconds.
    pub milliseconds: u64,
    /// The addresses the resolver answered with.
    pub addresses: Vec<String>,
}

/// The DNS the engine asks, as the settings window offers it.
///
/// The values are checked before they are stored: an endpoint that cannot be
/// asked is refused here rather than saved and failed at the first download.
/// What is kept is written to the engine's settings file and put in force
/// for every client from that moment on.
#[tauri::command]
pub fn set_dns(
    state: tauri::State<'_, crate::state::AppState>,
    dns: DnsSettings,
) -> Result<DnsSettings, String> {
    let clean = dns.cleaned()?;
    state.modify_settings(|settings| settings.dns = clean.clone());
    Ok(clean)
}

/// The DNS in force, as the settings window shows it.
#[tauri::command]
pub fn dns_settings(state: tauri::State<'_, crate::state::AppState>) -> DnsSettings {
    state.engine_settings().dns
}

/// Ask the resolver the reader chose for one name, and say how long it took.
///
/// The chosen resolver answers or it does not: the system's may not answer
/// instead, because a hidden fallback is how a dead resolver goes unnoticed.
/// A certificate that does not check out reads as "the resolver could not be
/// reached", never as a failed download.
#[tauri::command]
pub async fn test_dns(dns: DnsSettings) -> Result<DnsTest, String> {
    let clean = dns.cleaned()?;
    let choice = clean.choice();
    let (took, addresses) = idm_core::net::resolver::test(&choice, TEST_NAME, &[]).await?;
    Ok(DnsTest {
        milliseconds: took.as_millis() as u64,
        addresses: addresses
            .iter()
            .map(|address| address.to_string())
            .collect(),
    })
}
