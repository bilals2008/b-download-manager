//! The debrid services a link can be unlocked with.
//!
//! A debrid service turns a host link into a direct link that a plain HTTP
//! client can fetch. The engine asks the service before the first request,
//! and again when the direct link dies.
//!
//! The three services (Real-Debrid, AllDebrid, Premiumize) speak different
//! words but answer the same question, so they share one [`Service`] trait
//! and one HTTP client. The tokens stay in memory: they are never written
//! down and never printed.

pub mod all_debrid;
pub mod credential;
pub mod premiumize;
pub mod rate_limit;
pub mod real_debrid;
pub mod service;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub use credential::{Account, AccountStore, Credential, Secret};
pub use rate_limit::RateLimiter;
pub use service::{AccountInfo, DebridError, Http, Service, UnlockedLink};

/// The services the engine knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceKind {
    /// Real-Debrid.
    RealDebrid,
    /// AllDebrid.
    AllDebrid,
    /// Premiumize.
    Premiumize,
}

impl ServiceKind {
    /// Every service, in the order the settings list them.
    pub const ALL: [ServiceKind; 3] = [
        ServiceKind::RealDebrid,
        ServiceKind::AllDebrid,
        ServiceKind::Premiumize,
    ];

    /// The name the service writes on itself.
    pub fn name(self) -> &'static str {
        match self {
            ServiceKind::RealDebrid => "Real-Debrid",
            ServiceKind::AllDebrid => "AllDebrid",
            ServiceKind::Premiumize => "Premiumize",
        }
    }

    /// A short name, for keys in the settings and for the log.
    pub fn id(self) -> &'static str {
        match self {
            ServiceKind::RealDebrid => "real-debrid",
            ServiceKind::AllDebrid => "all-debrid",
            ServiceKind::Premiumize => "premiumize",
        }
    }

    /// The page where the user finds the key of their account.
    pub fn token_page(self) -> &'static str {
        match self {
            ServiceKind::RealDebrid => "https://real-debrid.com/apitoken",
            ServiceKind::AllDebrid => "https://alldebrid.com/apikeys",
            ServiceKind::Premiumize => "https://www.premiumize.me/account",
        }
    }

    /// Whether the service signs in with a code the user types in a browser.
    ///
    /// Premiumize does not: its key is on the account page and the user
    /// pastes it.
    pub fn signs_in_by_code(self) -> bool {
        !matches!(self, ServiceKind::Premiumize)
    }

    /// The requests a minute the app allows itself to this service.
    ///
    /// Well under what each service publishes, because a client that is cut
    /// off loses its access for a while.
    pub fn per_minute(self) -> usize {
        match self {
            ServiceKind::RealDebrid => 120,
            ServiceKind::AllDebrid => 300,
            ServiceKind::Premiumize => 60,
        }
    }
}

/// The app's handle on the services: the accounts it is signed in to, and the
/// HTTP client they share.
pub struct Debrid {
    http: Arc<Http>,
    accounts: AccountStore,
    /// The service of each account, built once so that a service keeps what
    /// it learned between two calls.
    services: RwLock<HashMap<ServiceKind, Arc<dyn Service>>>,
}

impl Debrid {
    /// A handle with no account signed in.
    pub fn new() -> Result<Self, DebridError> {
        Ok(Self {
            http: Arc::new(Http::new()?),
            accounts: AccountStore::new(),
            services: RwLock::new(HashMap::new()),
        })
    }

    /// The accounts the app is signed in to.
    pub fn accounts(&self) -> &AccountStore {
        &self.accounts
    }

    /// The HTTP client the services share.
    pub fn http(&self) -> &Arc<Http> {
        &self.http
    }

    /// Remember an account, and forget the service that was built from the
    /// old one.
    pub fn sign_in(&self, account: Account) {
        let kind = account.kind;
        self.accounts.set(account);
        self.write_services().remove(&kind);
    }

    /// Sign in with a key the user pasted, as Premiumize asks for.
    ///
    /// The key is checked against the service before it is kept, so the app
    /// never shows an account as signed in that cannot be used.
    pub async fn sign_in_with_key(
        &self,
        kind: ServiceKind,
        key: Secret,
    ) -> Result<AccountInfo, DebridError> {
        if key.is_empty() {
            return Err(DebridError::Service {
                kind,
                message: "the key is empty".into(),
            });
        }
        let service = build(
            kind,
            self.http.clone(),
            Credential::Token { token: key.clone() },
        );
        let info = service.account_info().await?;
        self.sign_in(Account {
            kind,
            credential: Credential::Token { token: key },
        });
        Ok(info)
    }

    /// The service of an account, when the app is signed in to it.
    pub fn service(&self, kind: ServiceKind) -> Option<Arc<dyn Service>> {
        let account = self.accounts.get(kind)?;
        let mut services = self.write_services();
        if let Some(service) = services.get(&kind) {
            return Some(service.clone());
        }
        let service = build(kind, self.http.clone(), account.credential);
        services.insert(kind, service.clone());
        Some(service)
    }

    /// Unlock a link with the account of a service.
    pub async fn unlock(
        &self,
        kind: ServiceKind,
        host_url: &str,
    ) -> Result<UnlockedLink, DebridError> {
        let service = self.service(kind).ok_or(DebridError::NotSignedIn {
            kind,
            message: "the app is not signed in".into(),
        })?;
        service.unlock(host_url).await
    }

    /// What the service says about the account the app is signed in with.
    pub async fn account_info(&self, kind: ServiceKind) -> Result<AccountInfo, DebridError> {
        let service = self.service(kind).ok_or(DebridError::NotSignedIn {
            kind,
            message: "the app is not signed in".into(),
        })?;
        service.account_info().await
    }

    fn write_services(
        &self,
    ) -> std::sync::RwLockWriteGuard<'_, HashMap<ServiceKind, Arc<dyn Service>>> {
        match self.services.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// The service of one service kind, with the credential of one account.
fn build(kind: ServiceKind, http: Arc<Http>, credential: Credential) -> Arc<dyn Service> {
    match kind {
        ServiceKind::RealDebrid => Arc::new(real_debrid::RealDebrid::new(http, credential)),
        ServiceKind::AllDebrid => Arc::new(all_debrid::AllDebrid::new(http, credential)),
        ServiceKind::Premiumize => Arc::new(premiumize::Premiumize::new(http, credential)),
    }
}

/// Signing in with a code the user types in a browser.
///
/// Real-Debrid's device flow and AllDebrid's PIN are the same two steps
/// seen from the outside: the app asks for a code, the user types it, the
/// app asks again until the user has.
pub mod signin {
    use super::*;

    /// A code waiting for the user.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Code {
        /// What the user types.
        pub shown: String,
        /// Where the user types it.
        pub page: String,
        /// Ask again every this many seconds.
        pub interval: u64,
        /// The code is good for this many seconds.
        pub expires_in: u64,
        inner: Inner,
    }

    /// What the app has to remember between the two steps.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Inner {
        Rd(real_debrid::DeviceCode),
        Ad(all_debrid::Pin),
    }

    /// Step one: the code the user has to type.
    pub async fn start(http: &Http, kind: ServiceKind) -> Result<Code, DebridError> {
        match kind {
            ServiceKind::RealDebrid => {
                let code = real_debrid::device_code(http).await?;
                Ok(Code {
                    shown: code.user_code.clone(),
                    page: code.verification_url.clone(),
                    interval: code.interval.max(1),
                    expires_in: code.expires_in,
                    inner: Inner::Rd(code),
                })
            }
            ServiceKind::AllDebrid => {
                let pin = all_debrid::pin(http).await?;
                Ok(Code {
                    shown: pin.pin.clone(),
                    page: pin.user_url.clone(),
                    // The documentation asks for one poll every five seconds.
                    interval: 5,
                    expires_in: pin.expires_in,
                    inner: Inner::Ad(pin),
                })
            }
            ServiceKind::Premiumize => Err(DebridError::Service {
                kind,
                message: "Premiumize signs in with a key the user pastes".into(),
            }),
        }
    }

    /// Step two, asked again every `interval` seconds: the credential, once
    /// the user allowed the app.
    pub async fn poll(http: &Http, code: &Code) -> Result<Option<Credential>, DebridError> {
        match &code.inner {
            Inner::Rd(device) => {
                let Some(credentials) =
                    real_debrid::device_credentials(http, &device.device_code).await?
                else {
                    return Ok(None);
                };
                let token = real_debrid::token(
                    http,
                    &credentials.client_id,
                    &credentials.client_secret,
                    &device.device_code,
                )
                .await?;
                Ok(Some(Credential::oauth(
                    credentials.client_id,
                    credentials.client_secret,
                    token.refresh_token,
                )))
            }
            Inner::Ad(pin) => Ok(all_debrid::pin_check(http, pin)
                .await?
                .map(Credential::token)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_names_itself() {
        assert_eq!(ServiceKind::RealDebrid.name(), "Real-Debrid");
        assert_eq!(ServiceKind::AllDebrid.name(), "AllDebrid");
        assert_eq!(ServiceKind::Premiumize.name(), "Premiumize");
        assert_eq!(ServiceKind::RealDebrid.id(), "real-debrid");
        assert_eq!(ServiceKind::ALL.len(), 3);
    }

    #[test]
    fn the_budget_is_under_what_the_services_publish() {
        // Real-Debrid publishes 240, AllDebrid 600, Premiumize 60.
        assert!(ServiceKind::RealDebrid.per_minute() < 240);
        assert!(ServiceKind::AllDebrid.per_minute() < 600);
        assert!(ServiceKind::Premiumize.per_minute() <= 60);
    }

    #[test]
    fn only_two_services_sign_in_with_a_code() {
        assert!(ServiceKind::RealDebrid.signs_in_by_code());
        assert!(ServiceKind::AllDebrid.signs_in_by_code());
        assert!(!ServiceKind::Premiumize.signs_in_by_code());
    }

    #[tokio::test]
    async fn a_service_is_built_once_and_kept() {
        let debrid = Debrid::new().unwrap();
        assert!(debrid.service(ServiceKind::RealDebrid).is_none());
        debrid.sign_in(Account {
            kind: ServiceKind::AllDebrid,
            credential: Credential::token("key"),
        });
        let first = debrid.service(ServiceKind::AllDebrid).unwrap();
        let second = debrid.service(ServiceKind::AllDebrid).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.kind(), ServiceKind::AllDebrid);
    }

    #[tokio::test]
    async fn signing_in_again_builds_a_new_service() {
        let debrid = Debrid::new().unwrap();
        debrid.sign_in(Account {
            kind: ServiceKind::Premiumize,
            credential: Credential::token("first"),
        });
        let first = debrid.service(ServiceKind::Premiumize).unwrap();
        debrid.sign_in(Account {
            kind: ServiceKind::Premiumize,
            credential: Credential::token("second"),
        });
        let second = debrid.service(ServiceKind::Premiumize).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
    }
}
