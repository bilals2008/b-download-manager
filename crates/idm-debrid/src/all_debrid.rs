//! AllDebrid's API v4 and v4.1.
//!
//! Only what a download manager needs: the account behind a key, the direct
//! link a host link turns into, and the way in (a PIN the user types on the
//! site).

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::credential::Credential;
use crate::service::{AccountInfo, BoxFuture, DebridError, Http, Service, UnlockedLink};
use crate::ServiceKind;

/// Where the API answers.
const BASE: &str = "https://api.alldebrid.com/";

/// How long the app waits between two polls of a link that is not ready.
const POLL: Duration = Duration::from_secs(5);

/// How long the app waits for a link that is being made.
const PATIENCE: Duration = Duration::from_secs(600);

/// Every answer of the API is an envelope: the data, or the error.
#[derive(Debug, Deserialize)]
struct Envelope<T> {
    status: String,
    data: Option<T>,
    error: Option<ErrorBody>,
}

/// The error half of the envelope.
#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

/// The account behind a key.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    /// The name of the account.
    pub username: String,
    /// Whether the account may use the paid features.
    #[serde(default)]
    pub is_premium: bool,
    /// When the premium runs out, as a unix time.
    #[serde(default)]
    pub premium_until: i64,
}

/// What the service says about a host link.
#[derive(Clone, Debug, Deserialize)]
pub struct Unlock {
    /// The direct link.
    #[serde(default)]
    pub link: String,
    /// The name of the file.
    pub filename: Option<String>,
    /// The size in bytes; 0 when the service does not know.
    #[serde(default)]
    pub filesize: i64,
    /// Set when the link is still being made: the id to ask about it.
    pub delayed: Option<i64>,
}

/// The state of a link that is still being made.
#[derive(Clone, Debug, Deserialize)]
pub struct Delayed {
    /// 1 while it is made, 2 when it is ready, 3 when it failed.
    pub status: i64,
    /// How many seconds are left.
    #[serde(default)]
    pub time_left: i64,
    /// The direct link, once it is ready.
    pub link: Option<String>,
}

/// The PIN the user types on the site.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Pin {
    /// The PIN the user types.
    pub pin: String,
    /// The token the app polls with.
    pub check: String,
    /// How long the PIN lives, in seconds.
    #[serde(default)]
    pub expires_in: u64,
    /// The page the user goes to.
    pub user_url: String,
}

/// The answer to a poll of the PIN.
#[derive(Debug, Deserialize)]
struct PinCheck {
    /// Whether the user already typed it.
    #[serde(default)]
    activated: bool,
    /// The key, once the user typed the PIN.
    apikey: Option<String>,
}

/// What an error code means, in the service's own words.
///
/// The service names its faults (`LINK_DOWN`, `AUTH_BAD_APIKEY`); the app
/// only has to notice the two that change what it does next.
fn fault_of(code: &str, message: &str) -> DebridError {
    match code {
        "AUTH_MISSING_APIKEY" | "AUTH_BAD_APIKEY" | "AUTH_USER_BANNED" | "ACCOUNT_INVALID" => {
            DebridError::NotSignedIn {
                kind: ServiceKind::AllDebrid,
                message: message.to_owned(),
            }
        }
        "LINK_HOST_NOT_SUPPORTED" | "LINK_NOT_SUPPORTED" => DebridError::Unsupported {
            kind: ServiceKind::AllDebrid,
            message: message.to_owned(),
        },
        _ => DebridError::Service {
            kind: ServiceKind::AllDebrid,
            message: if message.is_empty() {
                code.to_owned()
            } else {
                message.to_owned()
            },
        },
    }
}

/// Read an answer: the data of the envelope, or what its error means.
fn read<T: serde::de::DeserializeOwned>(status: u16, body: &[u8]) -> Result<T, DebridError> {
    if status == 429 {
        return Err(DebridError::RateLimited {
            kind: ServiceKind::AllDebrid,
        });
    }
    let envelope: Envelope<T> = serde_json::from_slice(body).map_err(|_| DebridError::Service {
        kind: ServiceKind::AllDebrid,
        message: format!("HTTP {status}"),
    })?;
    match (envelope.status.as_str(), envelope.data, envelope.error) {
        ("success", Some(data), _) => Ok(data),
        (_, _, Some(error)) => Err(fault_of(&error.code, &error.message)),
        _ => Err(DebridError::Service {
            kind: ServiceKind::AllDebrid,
            message: format!("HTTP {status}"),
        }),
    }
}

/// AllDebrid, with the credential of one account.
pub struct AllDebrid {
    http: Arc<Http>,
    credential: Credential,
}

impl AllDebrid {
    /// The service, with the credential of one account.
    pub fn new(http: Arc<Http>, credential: Credential) -> Self {
        Self { http, credential }
    }

    /// The key the calls carry.
    fn api_key(&self) -> Result<&str, DebridError> {
        let Credential::Token { token } = &self.credential else {
            return Err(DebridError::NotSignedIn {
                kind: ServiceKind::AllDebrid,
                message: "no key".into(),
            });
        };
        Ok(token.expose())
    }

    /// The account behind the credential.
    pub async fn user(&self) -> Result<User, DebridError> {
        #[derive(Deserialize)]
        struct Data {
            user: User,
        }
        let key = self.api_key()?;
        let reply = self
            .http
            .send(
                ServiceKind::AllDebrid,
                self.http.get(&format!("{BASE}v4/user")).bearer_auth(key),
            )
            .await?;
        let data: Data = read(reply.status, &reply.body)?;
        Ok(data.user)
    }
}

impl Service for AllDebrid {
    fn kind(&self) -> ServiceKind {
        ServiceKind::AllDebrid
    }

    fn account_info<'a>(&'a self) -> BoxFuture<'a, Result<AccountInfo, DebridError>> {
        Box::pin(async move {
            let user = self.user().await?;
            let left = user.premium_until - crate::service::unix_now();
            Ok(AccountInfo {
                user: user.username,
                premium: user.is_premium,
                premium_left: (user.is_premium && left > 0)
                    .then(|| Duration::from_secs(left as u64)),
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
                    ServiceKind::AllDebrid,
                    self.http
                        .post_form(&format!("{BASE}v4/link/unlock"), &[("link", link.as_str())])
                        .bearer_auth(key),
                )
                .await?;
            let unlock: Unlock = read(reply.status, &reply.body)?;

            // A link that is being made is asked about again and again: the
            // service is not slow on purpose, the file is simply large.
            let direct = match unlock.delayed {
                None => unlock.link,
                Some(id) => self.wait_for(id).await?,
            };
            if direct.is_empty() {
                return Err(DebridError::Service {
                    kind: ServiceKind::AllDebrid,
                    message: "the service gave no file for this link".into(),
                });
            }
            Ok(UnlockedLink {
                url: direct,
                file_name: unlock.filename.filter(|n| !n.trim().is_empty()),
                size: (unlock.filesize > 0).then_some(unlock.filesize as u64),
                max_connections: None,
            })
        })
    }
}

impl AllDebrid {
    /// Ask about a link until the service has it ready.
    async fn wait_for(&self, id: i64) -> Result<String, DebridError> {
        let key = self.api_key()?;
        let until = Instant::now() + PATIENCE;
        loop {
            let reply = self
                .http
                .send(
                    ServiceKind::AllDebrid,
                    self.http
                        .post_form(
                            &format!("{BASE}v4/link/delayed"),
                            &[("id", &id.to_string())],
                        )
                        .bearer_auth(key),
                )
                .await?;
            let delayed: Delayed = read(reply.status, &reply.body)?;
            match delayed.status {
                2 => return Ok(delayed.link.unwrap_or_default()),
                3 => {
                    return Err(DebridError::Service {
                        kind: ServiceKind::AllDebrid,
                        message: "the service could not make the link".into(),
                    })
                }
                _ => {}
            }
            if Instant::now() >= until {
                return Err(DebridError::Service {
                    kind: ServiceKind::AllDebrid,
                    message: "the service did not make the link in time".into(),
                });
            }
            tokio::time::sleep(POLL).await;
        }
    }
}

/// The PIN the user types on the site: step one of signing in.
pub async fn pin(http: &Http) -> Result<Pin, DebridError> {
    let reply = http
        .send(
            ServiceKind::AllDebrid,
            http.get(&format!("{BASE}v4.1/pin/get")),
        )
        .await?;
    read(reply.status, &reply.body)
}

/// The key, once the user typed the PIN.
pub async fn pin_check(http: &Http, pin: &Pin) -> Result<Option<String>, DebridError> {
    let reply = http
        .send(
            ServiceKind::AllDebrid,
            http.post_form(
                &format!("{BASE}v4/pin/check"),
                &[("check", pin.check.as_str()), ("pin", pin.pin.as_str())],
            ),
        )
        .await?;
    let check: PinCheck = read(reply.status, &reply.body)?;
    Ok(check
        .apikey
        .filter(|key| check.activated && !key.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_documented_envelopes_read() {
        let ok = br#"{"status":"success","data":{"link":"http://ombfyx.debrid.it/dl/abcdefgh12/somefile.txt",
            "host":"example","filename":"somefile.txt","paws":false,"filesize":699400192,"streams":[],
            "id":"abcdefgh12","hostDomain":"example.com"}}"#;
        let u: Unlock = read(200, ok).unwrap();
        assert_eq!(
            (u.filename.as_deref(), u.filesize, u.delayed),
            (Some("somefile.txt"), 699400192, None)
        );
        let err = br#"{"status":"error","error":{"code":"LINK_DOWN","message":"This link is not available"}}"#;
        assert_eq!(
            read::<Unlock>(200, err).unwrap_err(),
            DebridError::Service {
                kind: ServiceKind::AllDebrid,
                message: "This link is not available".into(),
            }
        );
    }

    #[test]
    fn a_bad_key_and_an_unknown_host_are_told_apart() {
        assert!(matches!(
            fault_of("AUTH_BAD_APIKEY", ""),
            DebridError::NotSignedIn { .. }
        ));
        assert!(matches!(
            fault_of("LINK_HOST_NOT_SUPPORTED", ""),
            DebridError::Unsupported { .. }
        ));
        assert_eq!(
            fault_of("SOMETHING_NEW", "Its words"),
            DebridError::Service {
                kind: ServiceKind::AllDebrid,
                message: "Its words".into(),
            }
        );
        assert!(matches!(
            read::<Unlock>(429, b"").unwrap_err(),
            DebridError::RateLimited { .. }
        ));
    }
}
