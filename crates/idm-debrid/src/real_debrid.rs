//! Real-Debrid's REST API and its OAuth device flow.
//!
//! Only what a download manager needs: the account behind a token, the
//! direct link a host link turns into, and the way in (a code the user
//! types on the site, or a token the user pastes).

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::credential::Credential;
use crate::service::{AccountInfo, BoxFuture, DebridError, Http, Service, UnlockedLink};
use crate::ServiceKind;

/// Where the REST API answers.
const REST: &str = "https://api.real-debrid.com/rest/1.0/";

/// Where the OAuth device flow answers.
const OAUTH: &str = "https://api.real-debrid.com/oauth/v2/";

/// The client id Real-Debrid gives to programs that ask for one.
pub const OPEN_SOURCE_CLIENT_ID: &str = "X245A4XAIBGVM";

/// The grant the device flow uses, for the first token and for the next ones.
const DEVICE_GRANT: &str = "http://oauth.net/grant_type/device/1.0";

/// The account behind a token.
#[derive(Clone, Debug, Deserialize)]
pub struct User {
    /// The name of the account.
    pub username: String,
    /// "premium" or "free".
    #[serde(rename = "type")]
    pub kind: String,
    /// Premium seconds left.
    #[serde(default)]
    pub premium: i64,
}

/// What the service says about a host link.
#[derive(Clone, Debug, Deserialize)]
pub struct Unrestricted {
    /// The name of the file.
    pub filename: Option<String>,
    /// The size in bytes; 0 when the service does not know.
    #[serde(default)]
    pub filesize: i64,
    /// How many connections may fetch the file at once.
    #[serde(default)]
    pub chunks: i64,
    /// The direct link.
    pub download: Option<String>,
}

/// The code the user types on the site.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct DeviceCode {
    /// The code the app polls with.
    pub device_code: String,
    /// The code the user types.
    pub user_code: String,
    /// How often the app may poll, in seconds.
    #[serde(default)]
    pub interval: u64,
    /// How long the code lives, in seconds.
    #[serde(default)]
    pub expires_in: u64,
    /// The page the user goes to.
    pub verification_url: String,
}

/// The client the device flow bound to the user.
#[derive(Clone, Debug, Deserialize)]
pub struct DeviceCredentials {
    /// The client id.
    pub client_id: String,
    /// The client secret.
    pub client_secret: String,
}

/// A token, and how long it lives.
#[derive(Clone, Debug, Deserialize)]
pub struct Token {
    /// The token the calls carry.
    pub access_token: String,
    /// How long it lives, in seconds.
    #[serde(default)]
    pub expires_in: u64,
    /// The token that buys a new access token.
    #[serde(default)]
    pub refresh_token: String,
}

/// The error body of the API: `{"error": "...", "error_code": N}`.
#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: String,
    error_code: Option<i64>,
}

/// What an error answer means, in the service's own words.
///
/// The service names its faults (`hoster_unsupported`, `bad_token`); the
/// app only has to notice the two that change what it does next.
fn fault(status: u16, body: &[u8]) -> DebridError {
    let parsed: Option<ErrorBody> = serde_json::from_slice(body).ok();
    let code = parsed.as_ref().and_then(|e| e.error_code);
    let message = parsed
        .as_ref()
        .map(|e| e.error.clone())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| format!("HTTP {status}"));
    match code {
        Some(8) => DebridError::NotSignedIn {
            kind: ServiceKind::RealDebrid,
            message,
        },
        Some(16) => DebridError::Unsupported {
            kind: ServiceKind::RealDebrid,
            message,
        },
        _ if status == 401 => DebridError::NotSignedIn {
            kind: ServiceKind::RealDebrid,
            message,
        },
        _ => DebridError::Service {
            kind: ServiceKind::RealDebrid,
            message,
        },
    }
}

/// Real-Debrid, with the credential of one account.
pub struct RealDebrid {
    http: Arc<Http>,
    credential: Credential,
    /// The access token of the device flow, and the moment it stops working.
    access: tokio::sync::Mutex<Option<(String, Instant)>>,
}

impl RealDebrid {
    /// The service, with the credential of one account.
    pub fn new(http: Arc<Http>, credential: Credential) -> Self {
        Self {
            http,
            credential,
            access: tokio::sync::Mutex::new(None),
        }
    }

    /// The token the calls carry.
    ///
    /// A pasted token is good forever; a device flow's access token is not,
    /// so a new one is asked for before it runs out.
    async fn access_token(&self) -> Result<String, DebridError> {
        let Credential::Oauth {
            client_id,
            client_secret,
            refresh_token,
        } = &self.credential
        else {
            let Credential::Token { token } = &self.credential else {
                return Err(DebridError::NotSignedIn {
                    kind: ServiceKind::RealDebrid,
                    message: "no credential".into(),
                });
            };
            return Ok(token.expose().to_string());
        };

        {
            let access = self.access.lock().await;
            if let Some((token, until)) = access.as_ref() {
                if *until > Instant::now() {
                    return Ok(token.clone());
                }
            }
        }

        let token = token(
            &self.http,
            client_id.expose(),
            client_secret.expose(),
            refresh_token.expose(),
        )
        .await?;
        // Five minutes of margin, so a token never runs out between the check
        // and the call it guards.
        let until =
            Instant::now() + Duration::from_secs(token.expires_in.saturating_sub(300).max(60));
        let mut access = self.access.lock().await;
        *access = Some((token.access_token.clone(), until));
        Ok(token.access_token)
    }

    /// The account behind the credential.
    pub async fn user(&self) -> Result<User, DebridError> {
        let token = self.access_token().await?;
        let reply = self
            .http
            .send(
                ServiceKind::RealDebrid,
                self.http.get(&format!("{REST}user")).bearer_auth(token),
            )
            .await?;
        if !(200..300).contains(&reply.status) {
            return Err(fault(reply.status, &reply.body));
        }
        serde_json::from_slice(&reply.body).map_err(|_| DebridError::Service {
            kind: ServiceKind::RealDebrid,
            message: "an answer the app cannot read".into(),
        })
    }
}
impl Service for RealDebrid {
    fn kind(&self) -> ServiceKind {
        ServiceKind::RealDebrid
    }

    fn account_info<'a>(&'a self) -> BoxFuture<'a, Result<AccountInfo, DebridError>> {
        Box::pin(async move {
            let user = self.user().await?;
            let premium = user.kind == "premium" && user.premium > 0;
            Ok(AccountInfo {
                user: user.username,
                premium,
                premium_left: (user.premium > 0).then(|| Duration::from_secs(user.premium as u64)),
            })
        })
    }

    fn unlock<'a>(&'a self, host_url: &'a str) -> BoxFuture<'a, Result<UnlockedLink, DebridError>> {
        Box::pin(async move {
            let link = crate::service::checked_link(host_url)?;
            let token = self.access_token().await?;
            let reply = self
                .http
                .send(
                    ServiceKind::RealDebrid,
                    self.http
                        .post_form(
                            &format!("{REST}unrestrict/link"),
                            &[("link", link.as_str()), ("remote", "0")],
                        )
                        .bearer_auth(token),
                )
                .await?;
            if !(200..300).contains(&reply.status) {
                return Err(fault(reply.status, &reply.body));
            }
            let unrestricted: Unrestricted =
                serde_json::from_slice(&reply.body).map_err(|_| DebridError::Service {
                    kind: ServiceKind::RealDebrid,
                    message: "an answer the app cannot read".into(),
                })?;
            // A link to a video site answers with several files and no single
            // download: there is nothing for the engine to fetch.
            let url = unrestricted.download.ok_or_else(|| DebridError::Service {
                kind: ServiceKind::RealDebrid,
                message: "the service gave no file for this link".into(),
            })?;
            Ok(UnlockedLink {
                url,
                file_name: unrestricted.filename.filter(|n| !n.trim().is_empty()),
                size: (unrestricted.filesize > 0).then_some(unrestricted.filesize as u64),
                max_connections: (unrestricted.chunks >= 1).then_some(unrestricted.chunks as u32),
            })
        })
    }
}

/// The code the user types on the site: step one of signing in.
pub async fn device_code(http: &Http) -> Result<DeviceCode, DebridError> {
    let url = format!(
        "{OAUTH}device/code?client_id={}&new_credentials=yes",
        OPEN_SOURCE_CLIENT_ID
    );
    let reply = http.send(ServiceKind::RealDebrid, http.get(&url)).await?;
    if !(200..300).contains(&reply.status) {
        return Err(fault(reply.status, &reply.body));
    }
    serde_json::from_slice(&reply.body).map_err(|_| DebridError::Service {
        kind: ServiceKind::RealDebrid,
        message: "an answer the app cannot read".into(),
    })
}

/// The client the user allowed, once they allowed it.
pub async fn device_credentials(
    http: &Http,
    device_code: &str,
) -> Result<Option<DeviceCredentials>, DebridError> {
    let url = format!(
        "{OAUTH}device/credentials?client_id={}&code={device_code}",
        OPEN_SOURCE_CLIENT_ID
    );
    let reply = http.send(ServiceKind::RealDebrid, http.get(&url)).await?;
    if (200..300).contains(&reply.status) {
        return serde_json::from_slice(&reply.body)
            .map(Some)
            .map_err(|_| DebridError::Service {
                kind: ServiceKind::RealDebrid,
                message: "an answer the app cannot read".into(),
            });
    }
    // Until the user allows the app, the service answers with an error whose
    // wording is not documented: that is the waiting, not a failure.
    if reply.status == 400 || reply.status == 403 || reply.status == 404 {
        return Ok(None);
    }
    Err(fault(reply.status, &reply.body))
}

/// A token for the device flow, and the next ones.
///
/// The same call buys the first token (with the device code) and the ones
/// after it (with the refresh token): that is what the grant is for.
pub async fn token(
    http: &Http,
    client_id: &str,
    client_secret: &str,
    code: &str,
) -> Result<Token, DebridError> {
    let reply = http
        .send(
            ServiceKind::RealDebrid,
            http.post_form(
                &format!("{OAUTH}token"),
                &[
                    ("client_id", client_id),
                    ("client_secret", client_secret),
                    ("code", code),
                    ("grant_type", DEVICE_GRANT),
                ],
            ),
        )
        .await?;
    if !(200..300).contains(&reply.status) {
        return Err(fault(reply.status, &reply.body));
    }
    serde_json::from_slice(&reply.body).map_err(|_| DebridError::Service {
        kind: ServiceKind::RealDebrid,
        message: "an answer the app cannot read".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fault_carries_the_service_own_words() {
        let f = fault(403, br#"{"error":"hoster_unavailable","error_code":19}"#);
        assert_eq!(
            f,
            DebridError::Service {
                kind: ServiceKind::RealDebrid,
                message: "hoster_unavailable".into(),
            }
        );
    }

    #[test]
    fn a_bad_token_and_an_unknown_host_are_told_apart() {
        assert!(matches!(
            fault(401, br#"{"error":"bad_token","error_code":8}"#),
            DebridError::NotSignedIn { .. }
        ));
        assert!(matches!(
            fault(503, br#"{"error":"hoster_unsupported","error_code":16}"#),
            DebridError::Unsupported { .. }
        ));
        // An answer that is not the service's own words still says something.
        assert_eq!(
            fault(500, b"not json"),
            DebridError::Service {
                kind: ServiceKind::RealDebrid,
                message: "HTTP 500".into(),
            }
        );
    }

    #[test]
    fn the_documented_answer_reads() {
        let body =
            br#"{"id":"ABC","filename":"x.rar","mimeType":"application/x-rar","filesize":1048576,
            "link":"https://host/file/x","host":"host.com","chunks":16,"crc":1,
            "download":"https://abc.download.real-debrid.com/d/ABC/x.rar","streamable":0}"#;
        let u: Unrestricted = serde_json::from_slice(body).unwrap();
        assert_eq!(u.filename.as_deref(), Some("x.rar"));
        assert_eq!((u.filesize, u.chunks), (1048576, 16));
        assert!(u.download.unwrap().starts_with("https://abc.download"));
    }
}
