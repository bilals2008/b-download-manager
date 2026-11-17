//! What the app remembers about the debrid accounts between two calls.
//!
//! One session holds the accounts the app is signed in to, the switch that
//! unlocks every matching link without being asked, and the links the
//! reader asked to unlock in the add dialog. It is shared by every part of
//! the application, so the request budget of a service is one budget and
//! not one per caller.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::rules::service_for;
use crate::service::{DebridError, UnlockedLink};
use crate::{Debrid, ServiceKind};

/// The one session of the application.
static SESSION: OnceLock<Session> = OnceLock::new();

/// What the engine needs to know about a link before it is asked to start it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// The link to fetch.
    pub url: String,
    /// The name the service gave the file.
    pub file_name: Option<String>,
    /// The size the service gave the file.
    pub size: Option<u64>,
    /// How many connections the service allows for the file.
    pub max_connections: Option<u32>,
}

impl From<UnlockedLink> for Prepared {
    fn from(link: UnlockedLink) -> Self {
        Self {
            url: link.url,
            file_name: link.file_name,
            size: link.size,
            max_connections: link.max_connections,
        }
    }
}

/// The debrid accounts of the application.
pub struct Session {
    debrid: Debrid,
    /// Whether every link a service can take is unlocked without being asked.
    automatic: AtomicBool,
    /// The links the reader asked to unlock in the add dialog.
    asked: Mutex<HashSet<String>>,
}

impl Session {
    /// The session of the application, with the accounts of the last run
    /// brought back from the operating system's store.
    pub fn shared() -> Result<&'static Session, DebridError> {
        if let Some(session) = SESSION.get() {
            return Ok(session);
        }
        let debrid = Debrid::new()?;
        debrid.restore();
        let session = Session {
            debrid,
            automatic: AtomicBool::new(false),
            asked: Mutex::new(HashSet::new()),
        };
        // The first one in wins; the other one is dropped and its accounts
        // are forgotten.
        let _ = SESSION.set(session);
        SESSION.get().ok_or_else(|| DebridError::Internal {
            message: "the debrid session could not be built".into(),
        })
    }

    /// Whether the application has a debrid session yet.
    ///
    /// The session is built when the reader first touches the debrid
    /// accounts, so a run that never uses them never opens the credential
    /// store.
    pub fn exists() -> bool {
        SESSION.get().is_some()
    }

    /// The accounts the application is signed in to.
    pub fn debrid(&self) -> &Debrid {
        &self.debrid
    }

    /// Whether every link a service can take is unlocked without being asked.
    pub fn set_automatic(&self, on: bool) {
        self.automatic.store(on, Ordering::Relaxed);
    }

    /// Whether the automatic switch is on.
    pub fn automatic(&self) -> bool {
        self.automatic.load(Ordering::Relaxed)
    }

    /// Remember that the reader asked for this link to be unlocked.
    pub fn ask(&self, url: &str) {
        let url = url.trim();
        if url.is_empty() {
            return;
        }
        if let Ok(mut asked) = self.asked.lock() {
            asked.insert(url.to_string());
        }
    }

    /// Whether the reader asked for this link to be unlocked.
    pub fn asked(&self, url: &str) -> bool {
        self.asked
            .lock()
            .map(|asked| asked.contains(url.trim()))
            .unwrap_or(false)
    }

    /// Whether the reader asked for this link to be unlocked, and forget it.
    pub fn take_asked(&self, url: &str) -> bool {
        self.asked
            .lock()
            .map(|mut asked| asked.remove(url.trim()))
            .unwrap_or(false)
    }

    /// Unlock a link before the engine is asked to start it.
    ///
    /// Nothing happens when the reader did not ask for this link and the
    /// automatic switch is off: the link is then fetched as it is.
    pub async fn prepare(&self, url: &str) -> Result<Option<Prepared>, DebridError> {
        let url = url.trim();
        if url.is_empty() {
            return Ok(None);
        }
        let asked = self.take_asked(url);
        if !asked && !self.automatic() {
            return Ok(None);
        }
        let link = self.unlock(url, asked).await?;
        Ok(link.map(Prepared::from))
    }

    /// Unlock a link with one of the accounts the application has.
    ///
    /// With `any` every signed-in account is asked, the first one that takes
    /// the link wins. Without it only the service that owns the link is
    /// asked, which is what the automatic switch means.
    pub async fn unlock(&self, url: &str, any: bool) -> Result<Option<UnlockedLink>, DebridError> {
        let url = url.trim();
        let candidates: Vec<ServiceKind> = if any {
            ServiceKind::ALL
                .into_iter()
                .filter(|kind| self.debrid.accounts().is_signed_in(*kind))
                .collect()
        } else {
            service_for(url)
                .into_iter()
                .filter(|kind| self.debrid.accounts().is_signed_in(*kind))
                .collect()
        };
        if candidates.is_empty() {
            return Ok(None);
        }
        for kind in candidates {
            if !self.paid(kind).await? {
                return Err(DebridError::NotPremium { kind });
            }
            match self.debrid.unlock(kind, url).await {
                Ok(link) => return Ok(Some(link)),
                // This service does not take this host; another one may.
                Err(DebridError::Unsupported { .. }) => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(None)
    }

    /// Whether the account of a service may use the paid features.
    ///
    /// The service is asked once, when nothing is known about the account
    /// yet; the answer is kept until the account changes.
    async fn paid(&self, kind: ServiceKind) -> Result<bool, DebridError> {
        if let Some(state) = self.debrid.state(kind) {
            return Ok(state.premium);
        }
        let state = self.debrid.account_state(kind).await?;
        Ok(state.premium)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session of its own, so the tests never touch the accounts of the
    /// reader's machine.
    fn session() -> Session {
        Session {
            debrid: Debrid::new().expect("the services"),
            automatic: AtomicBool::new(false),
            asked: Mutex::new(HashSet::new()),
        }
    }

    #[test]
    fn a_link_the_reader_asked_for_is_remembered() {
        let session = session();
        session.ask(" https://rapidgator.net/file/abc ");
        assert!(session.asked("https://rapidgator.net/file/abc"));
        assert!(session.take_asked("https://rapidgator.net/file/abc"));
        assert!(!session.asked("https://rapidgator.net/file/abc"));
    }

    #[tokio::test]
    async fn a_link_nobody_asked_for_is_left_alone() {
        let session = session();
        let prepared = session
            .prepare("https://rapidgator.net/file/abc")
            .await
            .expect("the services");
        assert!(prepared.is_none());
    }

    #[tokio::test]
    async fn a_link_without_an_account_is_left_alone() {
        let session = session();
        session.set_automatic(true);
        let prepared = session
            .prepare("magnet:?xt=urn:btih:0123456789abcdef")
            .await
            .expect("the services");
        assert!(prepared.is_none());
    }
}
