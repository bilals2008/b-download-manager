//! Premiumize's API.
//!
//! Only what a download manager needs: the account behind a key, the direct
//! link a host link turns into. Premiumize has no code to type: its key is
//! on the account page, and the user pastes it.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

use crate::credential::Credential;
use crate::service::{AccountInfo, BoxFuture, DebridError, Http, Service, UnlockedLink};
use crate::ServiceKind;

/// Where the API answers.
const BASE: &str = "https://www.premiumize.me/api/";

/// Every answer carries its status: "success", or "error" with a message.
#[derive(Debug, Deserialize)]
struct Status {
    status: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    code: String,
}

/// The account behind a key.
#[derive(Clone, Debug, Deserialize)]
pub struct Account {
    /// The customer id; Premiumize gives no name.
    #[serde(default)]
    pub customer_id: serde_json::Value,
    /// When the premium runs out, as a unix time; none for a free account.
    pub premium_until: Option<i64>,
}

/// What the service says about a host link.
#[derive(Clone, Debug, Deserialize)]
pub struct Direct {
    /// The files behind the link; one for a file, more for a folder.
    #[serde(default)]
    pub content: Vec<Entry>,
}

/// One file of the answer.
#[derive(Clone, Debug, Deserialize)]
pub struct Entry {
    /// The path of the file inside the link.
    #[serde(default)]
    pub path: String,
    /// The size in bytes.
    #[serde(default)]
    pub size: i64,
    /// The direct link.
    #[serde(default)]
    pub link: String,
}

/// What an error code means, in the service's own words.
fn fault_of(code: &str, message: &str) -> DebridError {
    match code {
        "authentication_failed" => DebridError::NotSignedIn {
            kind: ServiceKind::Premiumize,
            message: message.to_owned(),
        },
        "service_unsupported" => DebridError::Unsupported {
            kind: ServiceKind::Premiumize,
            message: message.to_owned(),
        },
        _ => DebridError::Service {
            kind: ServiceKind::Premiumize,
            message: if message.is_empty() {
                code.to_owned()
            } else {
                message.to_owned()
            },
        },
    }
}

/// Read an answer: the data of the answer, or what its error means.
fn read<T: serde::de::DeserializeOwned>(status: u16, body: &[u8]) -> Result<T, DebridError> {
    if status == 429 {
        return Err(DebridError::Service {
            kind: ServiceKind::Premiumize,
            message: "too many requests".into(),
        });
    }
    let parsed: Status = serde_json::from_slice(body).map_err(|_| DebridError::Service {
        kind: ServiceKind::Premiumize,
        message: format!("HTTP {status}"),
    })?;
    if parsed.status != "success" {
        return Err(fault_of(&parsed.code, &parsed.message));
    }
    serde_json::from_slice(body).map_err(|_| DebridError::Service {
        kind: ServiceKind::Premiumize,
        message: "an answer the app cannot read".into(),
    })
}

/// Premiumize, with the credential of one account.
pub struct Premiumize {
    http: Arc<Http>,
    credential: Credential,
}

impl Premiumize {
    /// The service, with the credential of one account.
    pub fn new(http: Arc<Http>, credential: Credential) -> Self {
        Self { http, credential }
    }

    /// The key the calls carry.
    fn api_key(&self) -> Result<&str, DebridError> {
        let Credential::Token { token } = &self.credential else {
            return Err(DebridError::NotSignedIn {
                kind: ServiceKind::Premiumize,
                message: "no key".into(),
            });
        };
        Ok(token.expose())
    }

    /// The account behind the credential.
    pub async fn account(&self) -> Result<Account, DebridError> {
        let key = self.api_key()?;
        let reply = self
            .http
            .send(
                ServiceKind::Premiumize,
                self.http
                    .get(&format!("{BASE}account/info"))
                    .bearer_auth(key),
            )
            .await?;
        read(reply.status, &reply.body)
    }
}

impl Service for Premiumize {
    fn kind(&self) -> ServiceKind {
        ServiceKind::Premiumize
    }

    fn account_info<'a>(&'a self) -> BoxFuture<'a, Result<AccountInfo, DebridError>> {
        Box::pin(async move {
            let account = self.account().await?;
            let left = account
                .premium_until
                .map(|until| until - crate::service::unix_now())
                .filter(|left| *left > 0);
            Ok(AccountInfo {
                // Premiumize gives no name: its customer id stands for it.
                user: match account.customer_id {
                    serde_json::Value::String(id) => id,
                    serde_json::Value::Number(id) => id.to_string(),
                    _ => String::new(),
                },
                premium: left.is_some(),
                premium_left: left.map(|left| Duration::from_secs(left as u64)),
            })
        })
    }

    fn unlock<'a>(&'a self, host_url: &'a str) -> BoxFuture<'a, Result<UnlockedLink, DebridError>> {
        Box::pin(async move {
            let link = crate::service::checked_link(host_url)?;
            let key = self.api_key()?;
            let reply = self
                .http
                .send(
                    ServiceKind::Premiumize,
                    self.http
                        .post_form(
                            &format!("{BASE}transfer/directdl"),
                            &[("src", link.as_str())],
                        )
                        .bearer_auth(key),
                )
                .await?;
            let direct: Direct = read(reply.status, &reply.body)?;

            // One file per host link; a link to a folder has more, and the
            // app fetches the first file of it.
            let entry = direct
                .content
                .into_iter()
                .next()
                .ok_or_else(|| DebridError::Service {
                    kind: ServiceKind::Premiumize,
                    message: "the service gave no file for this link".into(),
                })?;
            if entry.link.is_empty() {
                return Err(DebridError::Service {
                    kind: ServiceKind::Premiumize,
                    message: "the service gave no file for this link".into(),
                });
            }
            let name = entry.path.rsplit('/').next().unwrap_or_default().to_owned();
            Ok(UnlockedLink {
                url: entry.link,
                file_name: Some(name).filter(|n| !n.trim().is_empty()),
                size: (entry.size > 0).then_some(entry.size as u64),
                max_connections: None,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_documented_answers_read() {
        let body = br#"{"status":"success","content":[{"path":"Folder/video1.mkv","size":123456789,"link":"https://x/1"}]}"#;
        let direct: Direct = read(200, body).unwrap();
        assert_eq!(direct.content[0].path, "Folder/video1.mkv");
        assert_eq!(direct.content[0].size, 123456789);
        let err = br#"{"status":"error","message":"Not found","code":"not_found"}"#;
        assert_eq!(
            read::<Direct>(200, err).unwrap_err(),
            DebridError::Service {
                kind: ServiceKind::Premiumize,
                message: "Not found".into(),
            }
        );
    }

    #[test]
    fn a_bad_key_and_an_unknown_host_are_told_apart() {
        assert!(matches!(
            fault_of("authentication_failed", ""),
            DebridError::NotSignedIn { .. }
        ));
        assert!(matches!(
            fault_of("service_unsupported", ""),
            DebridError::Unsupported { .. }
        ));
        assert_eq!(
            fault_of("new_code", "Its words"),
            DebridError::Service {
                kind: ServiceKind::Premiumize,
                message: "Its words".into(),
            }
        );
    }
}
