//! Which service handles a link, and whether the link should be unlocked.
//!
//! The rules are the app's own: the services know which file hosts they
//! take, but the app has to know before it asks. A link on the site of a
//! service belongs to that service, a magnet link belongs to Real-Debrid,
//! and a link from anywhere else is nobody's.

use crate::credential::AccountStore;
use crate::ServiceKind;

/// The domains each service answers on.
///
/// A host is matched on its own domain and on everything under it, so
/// `api.real-debrid.com` is Real-Debrid's.
fn domains(kind: ServiceKind) -> &'static [&'static str] {
    match kind {
        ServiceKind::RealDebrid => &["real-debrid.com"],
        ServiceKind::AllDebrid => &["alldebrid.com"],
        ServiceKind::Premiumize => &["premiumize.me"],
    }
}

/// Whether a link is a magnet link.
pub fn is_magnet(url: &str) -> bool {
    url.trim().to_ascii_lowercase().starts_with("magnet:")
}

/// The host of a link, in lower case and without its port.
fn host_of(url: &str) -> Option<String> {
    let rest = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    let host = authority.split(':').next().unwrap_or_default().trim();
    let host = host.to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// The service that handles a link.
///
/// A magnet link belongs to Real-Debrid, which is the only service of the
/// three that takes one. A link on the site of a service belongs to that
/// service. A link from anywhere else belongs to nobody here.
pub fn service_for(url: &str) -> Option<ServiceKind> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    if is_magnet(url) {
        return Some(ServiceKind::RealDebrid);
    }
    let host = host_of(url)?;
    ServiceKind::ALL.into_iter().find(|kind| {
        domains(*kind)
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    })
}

/// Whether a link should be unlocked with the account of a service.
///
/// Only an account that is signed in and still has paid time left may
/// unlock: an account without paid time would start a slow download
/// instead of a fast one.
pub fn should_unlock(url: &str, signed_in: &AccountStore) -> bool {
    let Some(kind) = service_for(url) else {
        return false;
    };
    signed_in.can_unlock(kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::{Account, AccountState, Credential};
    use crate::service::AccountInfo;
    use std::time::Duration;

    fn store_with(kind: ServiceKind, premium: bool) -> AccountStore {
        let store = AccountStore::new();
        store.set(Account {
            kind,
            credential: Credential::token("key"),
        });
        store.set_state(AccountState::new(
            kind,
            &AccountInfo {
                user: "reader".into(),
                premium,
                premium_left: premium.then(|| Duration::from_secs(86_400)),
            },
        ));
        store
    }

    #[test]
    fn each_service_answers_on_its_own_domains() {
        assert_eq!(
            service_for("https://real-debrid.com/d/ABC/file.rar"),
            Some(ServiceKind::RealDebrid)
        );
        assert_eq!(
            service_for("https://api.real-debrid.com/rest/1.0/user"),
            Some(ServiceKind::RealDebrid)
        );
        assert_eq!(
            service_for("https://abc.download.real-debrid.com/d/ABC/file.rar"),
            Some(ServiceKind::RealDebrid)
        );
        assert_eq!(
            service_for("https://alldebrid.com/service/link"),
            Some(ServiceKind::AllDebrid)
        );
        assert_eq!(
            service_for("https://www.premiumize.me/dl/abc"),
            Some(ServiceKind::Premiumize)
        );
    }

    #[test]
    fn the_host_is_matched_without_its_case_or_its_port() {
        assert_eq!(
            service_for("HTTPS://API.Real-Debrid.com/rest/1.0/user"),
            Some(ServiceKind::RealDebrid)
        );
        assert_eq!(
            service_for("https://real-debrid.com:443/d/ABC/file.rar"),
            Some(ServiceKind::RealDebrid)
        );
    }

    #[test]
    fn a_link_of_another_host_belongs_to_nobody() {
        assert_eq!(service_for("https://rapidgator.net/file/abc"), None);
        assert_eq!(service_for("https://notreal-debrid.com/file"), None);
        assert_eq!(service_for(""), None);
    }

    #[test]
    fn a_magnet_belongs_to_real_debrid() {
        assert_eq!(
            service_for("magnet:?xt=urn:btih:0123456789abcdef&dn=file"),
            Some(ServiceKind::RealDebrid)
        );
        assert_eq!(
            service_for("MAGNET:?xt=urn:btih:0123456789abcdef"),
            Some(ServiceKind::RealDebrid)
        );
    }

    #[test]
    fn a_link_is_unlocked_only_with_a_paid_account() {
        let premium = store_with(ServiceKind::RealDebrid, true);
        assert!(should_unlock(
            "magnet:?xt=urn:btih:0123456789abcdef",
            &premium
        ));
        assert!(should_unlock("https://real-debrid.com/d/ABC/f.rar", &premium));

        let free = store_with(ServiceKind::RealDebrid, false);
        assert!(!should_unlock(
            "magnet:?xt=urn:btih:0123456789abcdef",
            &free
        ));
    }

    #[test]
    fn a_link_of_another_service_is_not_unlocked_with_this_account() {
        let store = store_with(ServiceKind::RealDebrid, true);
        assert!(!should_unlock("https://alldebrid.com/service/link", &store));
        assert!(!should_unlock("https://rapidgator.net/file/abc", &store));
    }

    #[test]
    fn an_account_that_is_not_signed_in_unlocks_nothing() {
        let store = AccountStore::new();
        assert!(!should_unlock(
            "magnet:?xt=urn:btih:0123456789abcdef",
            &store
        ));
    }
}
