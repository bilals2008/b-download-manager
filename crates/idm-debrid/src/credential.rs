//! The account of one service: the secret it signs in with, and the store
//! that keeps it.
//!
//! A token that reaches a log file, a crash report or a settings file is a
//! token that has to be changed by the user, so the secrets here never print
//! themselves and the store keeps them in memory only.

use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;

use serde::Serialize;

use crate::service::AccountInfo;
use crate::ServiceKind;

/// A secret that never shows itself.
///
/// The one place a secret may be read is the request that carries it, so the
/// type is built to make reading it deliberate.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wrap a secret so it cannot be printed by accident.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The secret itself, for the one place that needs it: the request.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the secret is empty: an empty token signs in to nothing.
    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// What the app keeps to sign in to a service.
#[derive(Clone, PartialEq, Eq)]
pub enum Credential {
    /// A key the user pasted, or that a PIN gave: AllDebrid's API key,
    /// Premiumize's API key, Real-Debrid's personal API token.
    Token {
        /// The key.
        token: Secret,
    },
    /// The device flow's user-bound client and its refresh token
    /// (Real-Debrid). The access token is short-lived and comes from these.
    Oauth {
        /// The client the device flow bound to the user.
        client_id: Secret,
        /// The client's secret.
        client_secret: Secret,
        /// The token that buys a new access token.
        refresh_token: Secret,
    },
}

impl Credential {
    /// A key the user pasted.
    pub fn token(value: impl Into<String>) -> Self {
        Credential::Token {
            token: Secret::new(value),
        }
    }

    /// The device flow's credentials.
    pub fn oauth(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        refresh_token: impl Into<String>,
    ) -> Self {
        Credential::Oauth {
            client_id: Secret::new(client_id),
            client_secret: Secret::new(client_secret),
            refresh_token: Secret::new(refresh_token),
        }
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Credential::Token { .. } => f.write_str("Token(***)"),
            Credential::Oauth { .. } => f.write_str("Oauth(***)"),
        }
    }
}

impl fmt::Display for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// One account: which service it belongs to, and what signs in to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// The service this account signs in to.
    pub kind: ServiceKind,
    /// The secret it signs in with.
    pub credential: Credential,
}

/// What the window may show about an account.
///
/// The key is not part of it: the window never sees the secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountState {
    /// Which service the account belongs to.
    pub kind: ServiceKind,
    /// The name the service writes on itself.
    pub service: String,
    /// The name the service gave the account.
    pub user: String,
    /// Whether the account may use the paid features.
    pub premium: bool,
    /// The paid days left, when the service says how many there are.
    pub premium_days: Option<u64>,
}

impl AccountState {
    /// The state of an account, from what the service said about it.
    pub fn new(kind: ServiceKind, info: &AccountInfo) -> Self {
        Self {
            kind,
            service: kind.name().to_string(),
            user: info.user.clone(),
            premium: info.premium,
            premium_days: info
                .premium_left
                .map(|left| left.as_secs().div_ceil(86_400)),
        }
    }
}

/// The accounts the app is signed in to, one per service.
///
/// The store lives in memory: a token that is written down anywhere else is
/// a token that leaks. The operating system's own store keeps the token
/// between two runs; see [`crate::os_store`].
#[derive(Debug, Default)]
pub struct AccountStore {
    accounts: RwLock<HashMap<ServiceKind, Account>>,
    /// What the services last said about the accounts.
    states: RwLock<HashMap<ServiceKind, AccountState>>,
}

impl AccountStore {
    /// A store with no account in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember an account, replacing the one of the same service.
    ///
    /// What the service said about the old account is forgotten with it: a
    /// new key may belong to another account.
    pub fn set(&self, account: Account) {
        let kind = account.kind;
        self.write().insert(kind, account);
        self.write_states().remove(&kind);
    }

    /// Remember what a service said about its account.
    pub fn set_state(&self, state: AccountState) {
        self.write_states().insert(state.kind, state);
    }

    /// What the service last said about the account of one service.
    pub fn state(&self, kind: ServiceKind) -> Option<AccountState> {
        self.read_states().get(&kind).cloned()
    }

    /// Forget what a service said about its account.
    pub fn forget_state(&self, kind: ServiceKind) {
        self.write_states().remove(&kind);
    }

    /// Whether the account of a service may unlock a link.
    ///
    /// An account that is not signed in may not, and neither may one
    /// without paid time left: it would start a slow download.
    pub fn can_unlock(&self, kind: ServiceKind) -> bool {
        self.is_signed_in(kind) && self.state(kind).is_some_and(|state| state.premium)
    }

    /// The account of one service, when the app is signed in to it.
    pub fn get(&self, kind: ServiceKind) -> Option<Account> {
        self.read().get(&kind).cloned()
    }

    /// Forget the account of one service: the user signed out.
    pub fn remove(&self, kind: ServiceKind) {
        let mut accounts = self.write();
        accounts.remove(&kind);
        self.forget_state(kind);
    }

    /// The services the app is signed in to.
    pub fn kinds(&self) -> Vec<ServiceKind> {
        let accounts = self.read();
        ServiceKind::ALL
            .iter()
            .copied()
            .filter(|kind| accounts.contains_key(kind))
            .collect()
    }

    /// Whether the app is signed in to a service.
    pub fn is_signed_in(&self, kind: ServiceKind) -> bool {
        self.read().contains_key(&kind)
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, HashMap<ServiceKind, Account>> {
        match self.accounts.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<ServiceKind, Account>> {
        match self.accounts.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn read_states(&self) -> std::sync::RwLockReadGuard<'_, HashMap<ServiceKind, AccountState>> {
        match self.states.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write_states(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<ServiceKind, AccountState>> {
        match self.states.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Hide a secret that ended up in a message.
///
/// A service that answers with the key it was given would put it in front
/// of the reader, in a log and in a crash report.
pub fn scrub(message: &str, secret: &str) -> String {
    if secret.is_empty() {
        return message.to_string();
    }
    message.replace(secret, "***")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn no_secret_in_logs() {
        let secrets = [
            "TOKEN-abc123",
            "CLIENT-SECRET-abc123",
            "REFRESH-TOKEN-abc123",
        ];
        let credentials = [
            Credential::token(secrets[0]),
            Credential::oauth(secrets[1], secrets[1], secrets[2]),
        ];
        for credential in &credentials {
            let shown = format!("{credential:?} {credential}");
            for secret in secrets {
                assert!(!shown.contains(secret), "{shown}");
            }
            // The account and the store wrap the credential, so their own
            // output has to be safe as well.
            let account = Account {
                kind: ServiceKind::RealDebrid,
                credential: credential.clone(),
            };
            let shown = format!("{account:?}");
            for secret in secrets {
                assert!(!shown.contains(secret), "{shown}");
            }
        }
        let secret = Secret::new(secrets[0]);
        assert!(!format!("{secret:?} {secret}").contains(secrets[0]));
    }

    #[test]
    fn the_store_keeps_one_account_per_service() {
        let store = AccountStore::new();
        assert!(store.kinds().is_empty());
        store.set(Account {
            kind: ServiceKind::AllDebrid,
            credential: Credential::token("key"),
        });
        store.set(Account {
            kind: ServiceKind::Premiumize,
            credential: Credential::token("key"),
        });
        assert_eq!(
            store.kinds(),
            vec![ServiceKind::AllDebrid, ServiceKind::Premiumize]
        );
        assert!(store.is_signed_in(ServiceKind::AllDebrid));
        assert!(!store.is_signed_in(ServiceKind::RealDebrid));
        store.remove(ServiceKind::AllDebrid);
        assert!(!store.is_signed_in(ServiceKind::AllDebrid));
        assert_eq!(store.kinds(), vec![ServiceKind::Premiumize]);
    }

    #[test]
    fn a_pasted_key_never_prints() {
        let account = Account {
            kind: ServiceKind::Premiumize,
            credential: Credential::token("pm-api-key"),
        };
        assert!(!format!("{account:?}").contains("pm-api-key"));
    }

    fn state(kind: ServiceKind, premium: bool) -> AccountState {
        AccountState::new(
            kind,
            &AccountInfo {
                user: "reader@example.org".into(),
                premium,
                premium_left: premium.then(|| Duration::from_secs(3 * 86_400)),
            },
        )
    }

    #[test]
    fn the_window_sees_the_account_and_never_the_key() {
        let store = AccountStore::new();
        store.set(Account {
            kind: ServiceKind::RealDebrid,
            credential: Credential::token("rd-key"),
        });
        store.set_state(state(ServiceKind::RealDebrid, true));
        let shown = store.state(ServiceKind::RealDebrid).unwrap();
        assert_eq!(shown.user, "reader@example.org");
        assert_eq!(shown.service, "Real-Debrid");
        assert_eq!(shown.premium_days, Some(3));
        assert!(!format!("{shown:?}").contains("rd-key"));
    }

    #[test]
    fn only_a_paid_account_may_unlock() {
        let store = AccountStore::new();
        assert!(!store.can_unlock(ServiceKind::AllDebrid));
        store.set(Account {
            kind: ServiceKind::AllDebrid,
            credential: Credential::token("ad-key"),
        });
        // Signed in, but the service has not said yet what the account is.
        assert!(!store.can_unlock(ServiceKind::AllDebrid));
        store.set_state(state(ServiceKind::AllDebrid, false));
        assert!(!store.can_unlock(ServiceKind::AllDebrid));
        store.set_state(state(ServiceKind::AllDebrid, true));
        assert!(store.can_unlock(ServiceKind::AllDebrid));
    }

    #[test]
    fn a_new_key_forgets_what_the_service_said() {
        let store = AccountStore::new();
        store.set(Account {
            kind: ServiceKind::Premiumize,
            credential: Credential::token("first"),
        });
        store.set_state(state(ServiceKind::Premiumize, true));
        store.set(Account {
            kind: ServiceKind::Premiumize,
            credential: Credential::token("second"),
        });
        assert!(store.state(ServiceKind::Premiumize).is_none());
        assert!(!store.can_unlock(ServiceKind::Premiumize));
    }

    #[test]
    fn a_secret_in_a_message_is_hidden() {
        let message = scrub("the key rd-key was refused", "rd-key");
        assert!(!message.contains("rd-key"), "{message}");
        assert_eq!(message, "the key *** was refused");
        assert_eq!(scrub("nothing to hide", ""), "nothing to hide");
    }
}
