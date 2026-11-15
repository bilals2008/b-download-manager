//! The operating system's own place for a secret.
//!
//! Windows keeps it in the Credential Manager, macOS in the Keychain, Linux
//! in the kernel keyring. The app never writes the secret into a file of
//! its own, so it cannot land in the settings, in the index of the
//! downloads or in a log line.

use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::credential::{Credential, Secret};
use crate::ServiceKind;

/// The name the entries are filed under.
const SERVICE: &str = "idm-debrid";

/// The secret of one account, as it is written down.
#[derive(Serialize, Deserialize)]
enum Stored {
    Token {
        token: String,
    },
    Oauth {
        client_id: String,
        client_secret: String,
        refresh_token: String,
    },
}

impl From<&Credential> for Stored {
    fn from(credential: &Credential) -> Self {
        match credential {
            Credential::Token { token } => Stored::Token {
                token: token.expose().to_string(),
            },
            Credential::Oauth {
                client_id,
                client_secret,
                refresh_token,
            } => Stored::Oauth {
                client_id: client_id.expose().to_string(),
                client_secret: client_secret.expose().to_string(),
                refresh_token: refresh_token.expose().to_string(),
            },
        }
    }
}

impl From<Stored> for Credential {
    fn from(stored: Stored) -> Self {
        match stored {
            Stored::Token { token } => Credential::Token {
                token: Secret::new(token),
            },
            Stored::Oauth {
                client_id,
                client_secret,
                refresh_token,
            } => Credential::Oauth {
                client_id: Secret::new(client_id),
                client_secret: Secret::new(client_secret),
                refresh_token: Secret::new(refresh_token),
            },
        }
    }
}

/// The entry of one service.
fn entry(kind: ServiceKind) -> Result<Entry, String> {
    Entry::new(SERVICE, kind.id()).map_err(|error| error.to_string())
}

/// Keep the secret of an account in the operating system's store.
pub fn save(kind: ServiceKind, credential: &Credential) -> Result<(), String> {
    let body = serde_json::to_string(&Stored::from(credential))
        .map_err(|_| "the key could not be written down".to_string())?;
    entry(kind)?
        .set_password(&body)
        .map_err(|error| error.to_string())
}

/// The secret of an account, when the operating system's store has one.
pub fn load(kind: ServiceKind) -> Option<Credential> {
    let body = entry(kind).ok()?.get_password().ok()?;
    let stored: Stored = serde_json::from_str(&body).ok()?;
    Some(Credential::from(stored))
}

/// Forget the secret of an account.
pub fn remove(kind: ServiceKind) -> Result<(), String> {
    match entry(kind)?.delete_credential() {
        Ok(()) => Ok(()),
        // An entry that was never there is already forgotten.
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_survives_the_trip_through_the_store() {
        let credential = Credential::token("rd-key");
        let body = serde_json::to_string(&Stored::from(&credential)).unwrap();
        let back = Credential::from(serde_json::from_str::<Stored>(&body).unwrap());
        assert_eq!(back, credential);
    }

    #[test]
    fn the_device_flow_credentials_survive_the_trip() {
        let credential = Credential::oauth("id", "secret", "refresh");
        let body = serde_json::to_string(&Stored::from(&credential)).unwrap();
        let back = Credential::from(serde_json::from_str::<Stored>(&body).unwrap());
        assert_eq!(back, credential);
    }

    #[test]
    fn the_written_form_holds_the_key_and_prints_nothing() {
        let credential = Credential::token("rd-key");
        let body = serde_json::to_string(&Stored::from(&credential)).unwrap();
        assert!(body.contains("rd-key"), "the body has to hold the key");
        // The credential itself never shows the key.
        assert!(!format!("{credential:?}").contains("rd-key"));
    }
}
