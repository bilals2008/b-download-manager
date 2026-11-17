//! The plumbing the three services share: one HTTP client, one request
//! budget per service, and one way of saying what went wrong.
//!
//! The services themselves only differ in the words they use: every request
//! goes through here, so a rate limit, a transport failure or a refusal is
//! reported the same way whichever service refused.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use url::Url;

use crate::rate_limit::RateLimiter;
use crate::ServiceKind;

/// The name the requests carry, so a service can tell a download manager
/// from a browser that happens to ask.
const USER_AGENT: &str = concat!("idm-debrid/", env!("CARGO_PKG_VERSION"));

/// A future a service hands back, boxed so that a trait can hold one.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Why a call to a service did not give a link.
///
/// The message in `Service` is the service's own wording: the services
/// explain their refusals better than this crate can, and a user who sees
/// the service's words can act on them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebridError {
    /// The service refused, and says why.
    Service {
        /// Which service refused.
        kind: ServiceKind,
        /// The service's own words.
        message: String,
    },
    /// The service did not take the token: it has to be signed in again.
    NotSignedIn {
        /// Which service refused.
        kind: ServiceKind,
        /// The service's own words.
        message: String,
    },
    /// The service does not take this file host, so the link is fetched as it
    /// is instead of being unlocked.
    Unsupported {
        /// Which service refused.
        kind: ServiceKind,
        /// The service's own words.
        message: String,
    },
    /// The service could not be reached.
    Transport {
        /// What went wrong on the way.
        message: String,
    },
    /// The text is not a link a service could take.
    InvalidLink {
        /// What was wrong with it.
        message: String,
    },
    /// The service says the app is asking too often.
    ///
    /// This is not a failed download: the service is asking the app to
    /// slow down, and the app does.
    RateLimited {
        /// Which service asked.
        kind: ServiceKind,
    },
    /// The account has no paid time left.
    ///
    /// Unlocking with it would start a slow download, so it is not used.
    NotPremium {
        /// Which service the account belongs to.
        kind: ServiceKind,
    },
    /// The app could not prepare the services.
    Internal {
        /// What went wrong.
        message: String,
    },
}

impl std::fmt::Display for DebridError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DebridError::Service { kind, message } => {
                write!(f, "{} refused: {}", kind.name(), message)
            }
            DebridError::NotSignedIn { kind, message } => {
                write!(f, "{} did not take the token: {}", kind.name(), message)
            }
            DebridError::Unsupported { kind, message } => {
                write!(
                    f,
                    "{} does not take this file host: {}",
                    kind.name(),
                    message
                )
            }
            DebridError::Transport { message } => write!(f, "cannot reach the service: {message}"),
            DebridError::InvalidLink { message } => write!(f, "not a link to unlock: {message}"),
            DebridError::RateLimited { kind } => {
                write!(f, "{} is asking the app to slow down", kind.name())
            }
            DebridError::NotPremium { kind } => {
                write!(f, "the {} account has no paid time left", kind.name())
            }
            DebridError::Internal { message } => write!(f, "{message}"),
        }
    }
}

impl DebridError {
    /// The sentence a reader sees.
    ///
    /// A refusal reads as a sentence and not as the service's own wording:
    /// the reader has to know what to do next.
    pub fn sentence(&self) -> String {
        match self {
            DebridError::RateLimited { kind } => format!(
                "{} is asking us to slow down. Try again in a minute.",
                kind.name()
            ),
            DebridError::NotPremium { kind } => {
                format!("the {} account has no paid time left.", kind.name())
            }
            DebridError::NotSignedIn { kind, .. } => {
                format!("{} did not take the key. Sign in again.", kind.name())
            }
            DebridError::Unsupported { kind, .. } => {
                format!("{} does not take this file host.", kind.name())
            }
            DebridError::Transport { .. } => {
                "the service could not be reached. Try again later.".to_string()
            }
            DebridError::InvalidLink { .. } => "that is not a link to unlock.".to_string(),
            DebridError::Service { kind, message } => {
                format!("{} refused: {}", kind.name(), message)
            }
            DebridError::Internal { message } => message.clone(),
        }
    }
}

impl std::error::Error for DebridError {}

/// A direct link, with what the service said about the file behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlockedLink {
    /// The address the file is fetched from.
    pub url: String,
    /// The name of the file, when the service gave one.
    pub file_name: Option<String>,
    /// The size of the file in bytes, when the service gave one.
    pub size: Option<u64>,
    /// How many connections may fetch this file at once, when the service
    /// said so.
    pub max_connections: Option<u32>,
}

/// What the service says about the account behind a credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountInfo {
    /// The name of the account, or what the service calls it.
    pub user: String,
    /// Whether the account may use the paid features.
    pub premium: bool,
    /// How much paid time is left, when the service says.
    pub premium_left: Option<Duration>,
}

/// What a service can do for a link.
///
/// The three services answer with different words and different shapes; this
/// trait is the one shape the rest of the app knows.
pub trait Service: Send + Sync {
    /// Which service this is.
    fn kind(&self) -> ServiceKind;

    /// Turn a link from a file host into a direct link.
    ///
    /// The link is not a host link when the service does not take that host:
    /// the answer is then `Unsupported`, and the caller fetches the original
    /// link instead.
    fn unlock<'a>(&'a self, host_url: &'a str) -> BoxFuture<'a, Result<UnlockedLink, DebridError>>;

    /// What the service says about the account behind the credential.
    ///
    /// A credential that is not good enough is refused here, before the app
    /// shows the account as signed in.
    fn account_info<'a>(&'a self) -> BoxFuture<'a, Result<AccountInfo, DebridError>>;
}

/// What a service answered: the status, the body and what it asked us to
/// wait for.
#[derive(Debug, Clone)]
pub struct Reply {
    /// The HTTP status.
    pub status: u16,
    /// The whole body, because these services put their answers in it.
    pub body: Vec<u8>,
    /// How long the service asked us to wait, when it said so.
    pub retry_after: Option<Duration>,
}

/// The HTTP client the services share, with a request budget per service.
///
/// The budget is per service and not per account, because the limit the
/// services publish counts everything that comes from the address, not what
/// one account does with it.
pub struct Http {
    client: reqwest::Client,
    limiters: Mutex<HashMap<ServiceKind, Arc<Budget>>>,
}

/// The request budget of one service.
///
/// A service that publishes a limit for the second as well gets a second
/// window, so a burst of unlocks cannot spend the minute's share in one
/// go.
#[derive(Debug)]
struct Budget {
    minute: RateLimiter,
    second: Option<RateLimiter>,
}

impl Budget {
    /// The budget of one service, from what it publishes.
    fn new(kind: ServiceKind) -> Self {
        Self {
            minute: RateLimiter::new(kind.per_minute()),
            second: kind
                .per_second()
                .map(|limit| RateLimiter::with_window(limit, Duration::from_secs(1))),
        }
    }

    /// Take a slot of the budget, waiting for one when it is full.
    async fn acquire(&self) {
        self.minute.acquire().await;
        if let Some(second) = &self.second {
            second.acquire().await;
        }
    }

    /// The service said "too many requests": nothing goes to it until then.
    fn pause(&self, wait: Duration) {
        self.minute.pause(wait);
        if let Some(second) = &self.second {
            second.pause(wait);
        }
    }
}

impl Http {
    /// A client with the default settings of the system.
    pub fn new() -> Result<Self, DebridError> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| DebridError::Transport {
                message: e.to_string(),
            })?;
        Ok(Self {
            client,
            limiters: Mutex::new(HashMap::new()),
        })
    }

    /// A GET request, before it takes a slot of the service's budget.
    pub fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.client.get(url)
    }

    /// A POST request with a form body, the way these services want their
    /// parameters.
    pub fn post_form(&self, url: &str, pairs: &[(&str, &str)]) -> reqwest::RequestBuilder {
        self.client
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(encode_pairs(pairs))
    }

    /// Send a request within the service's budget and read the whole answer.
    ///
    /// A service that says "too many requests" is given no more requests
    /// until it lets the app back in: a client that keeps asking after a
    /// refusal is the one that gets the account closed.
    pub async fn send(
        &self,
        kind: ServiceKind,
        request: reqwest::RequestBuilder,
    ) -> Result<Reply, DebridError> {
        let limiter = self.limiter(kind);
        limiter.acquire().await;
        let response = request.send().await.map_err(|e| DebridError::Transport {
            message: e.to_string(),
        })?;
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let body = response
            .bytes()
            .await
            .map_err(|e| DebridError::Transport {
                message: e.to_string(),
            })?
            .to_vec();
        if status == 429 {
            limiter.pause(retry_after.unwrap_or(Duration::from_secs(60)));
        }
        Ok(Reply {
            status,
            body,
            retry_after,
        })
    }

    /// The budget of one service, made on first use.
    fn limiter(&self, kind: ServiceKind) -> Arc<Budget> {
        let mut limiters = match self.limiters.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        Arc::clone(
            limiters
                .entry(kind)
                .or_insert_with(|| Arc::new(Budget::new(kind))),
        )
    }
}

/// Encode name and value pairs the way a form body or a query string wants
/// them.
pub(crate) fn encode_pairs(pairs: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(pairs.iter().copied());
    serializer.finish()
}

/// The current time as a unix time, which is how two of the services count
/// the paid time of an account.
pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The link a service could take: an http address with a host in it.
///
/// A link that is not one is refused here rather than by the service, so the
/// user is told the link is wrong instead of hearing that a service said no.
pub(crate) fn checked_link(host_url: &str) -> Result<String, DebridError> {
    let parsed = Url::parse(host_url).map_err(|e| DebridError::InvalidLink {
        message: format!("{host_url}: {e}"),
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(DebridError::InvalidLink {
            message: format!("{host_url} is not an http link"),
        });
    }
    if parsed.host_str().is_none() {
        return Err(DebridError::InvalidLink {
            message: format!("{host_url} has no host"),
        });
    }
    Ok(host_url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_is_checked_before_it_is_asked_for() {
        assert_eq!(
            checked_link("https://host.example/file.rar").unwrap(),
            "https://host.example/file.rar"
        );
        assert!(matches!(
            checked_link("not a link"),
            Err(DebridError::InvalidLink { .. })
        ));
        assert!(matches!(
            checked_link("ftp://host.example/file.rar"),
            Err(DebridError::InvalidLink { .. })
        ));
    }

    #[test]
    fn pairs_are_encoded_the_way_a_form_wants_them() {
        let body = encode_pairs(&[("link", "https://host.example/a?b=c&d=e"), ("remote", "0")]);
        assert_eq!(
            body,
            "link=https%3A%2F%2Fhost.example%2Fa%3Fb%3Dc%26d%3De&remote=0"
        );
    }

    #[test]
    fn a_refusal_carries_the_service_own_words() {
        let error = DebridError::Service {
            kind: ServiceKind::AllDebrid,
            message: "LINK_DOWN".into(),
        };
        assert_eq!(error.to_string(), "AllDebrid refused: LINK_DOWN");
    }
}
