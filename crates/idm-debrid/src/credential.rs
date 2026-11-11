//! The account of one service: the secret it signs in with, and the store
//! that keeps it.
//!
//! A token that reaches a log file, a crash report or a settings file is a
//! token that has to be changed by the user, so the secrets here never print
//! themselves and the store keeps them in memory only.

use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;

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

/// The accounts the app is signed in to, one per service.
///
/// The store lives in memory: a token that is written down anywhere else is
/// a token that leaks, and the app asks the user for it again after a
/// restart.
#[derive(Debug, Default)]
pub struct AccountStore {
    accounts: RwLock<HashMap<ServiceKind, Account>>,
}

impl AccountStore {
    /// A store with no account in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember an account, replacing the one of the same service.
    pub fn set(&self, account: Account) {
        let mut accounts = self.write();
        accounts.insert(account.kind, account);
    }

    /// The account of one service, when the app is signed in to it.
    pub fn get(&self, kind: ServiceKind) -> Option<Account> {
        self.read().get(&kind).cloned()
    }

    /// Forget the account of one service: the user signed out.
    pub fn remove(&self, kind: ServiceKind) {
        let mut accounts = self.write();
        accounts.remove(&kind);
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
