//!
//! A media address is not a file. It is a permission: one client identity, one
//! representation, one window of time, sometimes a set of browser headers that
//! were only ever meant for the site that issued them. A bare `String` loses
//! every one of those facts, and a download that keeps the string after the
//! permission is gone either fails at 87% or — worse — stitches bytes of a
//! different representation onto the ones already on disk.
//!
//! [`RequestLease`] carries those facts with the address:
//!
//! * who issued it ([`Authority`]: the page's player, or the app's own
//!   resolver), and the client profile it was signed for;
//! * which representation it serves ([`RepresentationFingerprint`]), so bytes
//!   of one version are never reused for another;
//! * when it stops working, so a refusal can be answered with a refresh
//!   instead of a blind retry;
//! * which headers may travel with it, and to which origin.
//!
//! Nothing here prints a signed query value. [`RequestLease::describe`] is the
//! only rendering meant for a log or an error, and it masks every parameter a
//! provider signs.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Url;

/// Who issued the address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// The page's own player fetched it, and the extension passed it on with
    /// the browser's headers.
    Browser,
    /// The app asked a provider for it through [`super::yt`].
    Native,
}

impl Authority {
    pub fn name(self) -> &'static str {
        match self {
            Authority::Browser => "browser",
            Authority::Native => "resolver",
        }
    }
}

/// How the bytes arrive, as far as the lease knows.
///
/// A playlist's segments and a plain file are both one address serving one
/// representation; only the media layer above knows which shape the list of
/// addresses took, so the lease does not try to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// One address serving the whole track, or one address per segment.
    Progressive,
}

impl Transport {
    pub fn name(self) -> &'static str {
        match self {
            Transport::Progressive => "progressive",
        }
    }
}

/// What one track's bytes are, independent of the address that serves them.
///
/// The address changes on every refresh — a new signature, a new expiry, a
/// different CDN host — while the representation stays the same, and a resume
/// is only safe when the representation stayed the same too. `key` is what the
/// engine's resume guard compares; it must never contain a signed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepresentationFingerprint {
    /// The provider's name for this stream: an itag, a representation id.
    pub id: String,
    pub container: String,
    pub codec: Option<String>,
    /// The whole representation's size, when the provider stated one.
    pub bytes: Option<u64>,
}

impl RepresentationFingerprint {
    /// The stable text the resume guard writes beside the bytes.
    pub fn key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.id,
            self.container,
            self.codec.as_deref().unwrap_or("-"),
            self.bytes
                .map(|bytes| bytes.to_string())
                .unwrap_or_else(|| "-".to_string())
        )
    }

    /// The shortest honest description, for a log line.
    pub fn describe(&self) -> String {
        let mut text = format!("{} {}", self.container, self.id);
        if let Some(codec) = &self.codec {
            text.push(' ');
            text.push_str(codec);
        }
        if let Some(bytes) = self.bytes {
            text.push_str(&format!(" ({bytes} bytes)"));
        }
        text
    }
}

/// Which headers of the page may travel with a lease, and to which origin.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Credentials {
    /// The only origin a page header or cookie may be sent to.
    pub origin: String,
    /// Whether a cookie was offered and consented to. A native lease never
    /// carries one: the resolver has no session of the reader's to reuse.
    pub cookies: bool,
}

impl Credentials {
    pub fn at(origin: impl Into<String>) -> Self {
        Self {
            origin: origin.into(),
            cookies: false,
        }
    }

    /// Whether a header of `origin` may be attached to `target`.
    pub fn admits(&self, target: &str) -> bool {
        !self.origin.is_empty() && origin_of(target).eq_ignore_ascii_case(&self.origin)
    }
}

/// One address, with everything needed to use it honestly and to replace it.
#[derive(Debug, Clone)]
pub struct RequestLease {
    pub url: String,
    /// The origin of `url`, kept not as a restatement of the URL but as the
    /// only place credentials may go.
    pub origin: String,
    pub authority: Authority,
    /// The client the address was signed for: `iOS`, `ANDROID_VR`, `browser`.
    pub client: String,
    pub client_version: Option<String>,
    /// The identity a server that binds an address to its client expects.
    pub user_agent: Option<String>,
    pub credentials: Credentials,
    pub transport: Transport,
    pub representation: RepresentationFingerprint,
    /// When the provider said the address stops working. `None` when it did
    /// not say; the lease is then only replaced on an actual refusal.
    pub expires_at: Option<SystemTime>,
    /// A diagnostic note that does not identify the address: an untransformed
    /// throttling parameter, a replaced client profile.
    pub note: Option<String>,
}

impl RequestLease {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        url: impl Into<String>,
        authority: Authority,
        client: impl Into<String>,
        client_version: Option<String>,
        user_agent: Option<String>,
        credentials: Credentials,
        transport: Transport,
        representation: RepresentationFingerprint,
        expires_at: Option<SystemTime>,
    ) -> Self {
        let url = url.into();
        let origin = origin_of(&url);
        Self {
            url,
            origin,
            authority,
            client: client.into(),
            client_version,
            user_agent,
            credentials,
            transport,
            representation,
            expires_at,
            note: None,
        }
    }

    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }

    /// Whether the provider's own deadline leaves no time to work with at
    /// `now`.
    ///
    /// A run that starts inside `margin` would reach the server after the
    /// address had stopped being accepted, so the margin is what makes a
    /// renewal worth doing *before* the first segment instead of after the
    /// first refusal.
    pub fn expiring_at(&self, margin: Duration, now: SystemTime) -> bool {
        self.expires_at.is_some_and(|at| match at.duration_since(now) {
            Ok(left) => left <= margin,
            // The deadline is already behind us, which is no time at all.
            Err(_) => true,
        })
    }

    /// Whether another lease serves the same bytes.
    pub fn same_bytes(&self, other: &RequestLease) -> bool {
        self.representation == other.representation
    }

    /// The headers this lease adds to a request, and the page headers that may
    /// travel with it.
    ///
    /// A page header is attached only when the lease's origin admits the
    /// target *and* the page origin the header was collected from is the same
    /// origin. A cookie or an authorization value needs the reader's consent
    /// as well: those are the reader's session, not a property of the page. A
    /// `user-agent` from the page is replaced, never duplicated — a server
    /// that signed an address for one identity refuses a request that names
    /// two.
    pub fn headers_for(
        &self,
        target: &str,
        page_headers: &[(String, String)],
        page_origin: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut headers: Vec<(String, String)> = Vec::new();
        // The lease is only ever used on its own origin. An address may redirect
        // to another host, and the page's headers must not follow it there.
        let home = self.credentials.admits(&self.origin) && self.credentials.admits(target);
        let admitted = home && page_origin.is_some_and(|origin| self.credentials.admits(origin));
        if admitted {
            for (name, value) in page_headers {
                let lower = name.to_ascii_lowercase();
                if lower == "user-agent" || lower == "host" || lower == "range" {
                    continue;
                }
                if (lower == "cookie" || lower == "authorization") && !self.credentials.cookies {
                    continue;
                }
                headers.push((name.clone(), value.clone()));
            }
        }
        if let Some(user_agent) = &self.user_agent {
            headers.push(("user-agent".to_string(), user_agent.clone()));
        }
        headers
    }

    /// The lease as a log line: origin and path, every signed value masked.
    ///
    /// The client and its version are what a refusal is usually about, and
    /// neither is a secret, so both are named while the address itself is not.
    pub fn describe(&self) -> String {
        let version = self
            .client_version
            .as_deref()
            .map(|version| format!("/{version}"))
            .unwrap_or_default();
        format!(
            "{} {} [{} {}{}] {}",
            redacted_target(&self.url),
            self.representation.describe(),
            self.authority.name(),
            self.client,
            version,
            self.transport.name()
        )
    }
}

impl fmt::Display for RequestLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.describe())
    }
}

/// Query parameters a provider signs, and therefore never prints.
const SIGNED: &[&str] = &[
    "sig",
    "signature",
    "lsig",
    "n",
    "pot",
    "expire",
    "expires",
    "ei",
    "token",
    "key",
    "ip",
    "cpn",
];

/// An address safe to print: the origin and path, with every signed query
/// value replaced by a marker and every other value kept.
pub fn redacted_target(url: &str) -> String {
    let Ok(parsed) = Url::parse(url) else {
        return "an unreadable address".to_string();
    };
    let mut shown = String::new();
    shown.push_str(&format!(
        "{}://{}{}",
        parsed.scheme(),
        parsed.host_str().unwrap_or(""),
        parsed.path()
    ));
    let mut first = true;
    for (name, value) in parsed.query_pairs() {
        shown.push(if first { '?' } else { '&' });
        first = false;
        if SIGNED.iter().any(|signed| name.eq_ignore_ascii_case(signed)) {
            shown.push_str(&name);
            shown.push_str("=…");
        } else {
            // A value that is itself a URL (a redirect target) is masked
            // whole: its own query may carry the signature.
            shown.push_str(&name);
            shown.push('=');
            if value.starts_with("http") {
                shown.push('…');
            } else {
                shown.push_str(&value);
            }
        }
    }
    shown
}

/// The scheme and host of an address, lowercased; empty when unreadable.
pub fn origin_of(url: &str) -> String {
    let Ok(parsed) = Url::parse(url) else {
        return String::new();
    };
    match (parsed.scheme(), parsed.host_str(), parsed.port()) {
        (scheme, Some(host), Some(port)) => format!("{}://{}:{port}", scheme, host.to_ascii_lowercase()),
        (scheme, Some(host), None) => format!("{}://{}", scheme, host.to_ascii_lowercase()),
        _ => String::new(),
    }
}

/// When a provider-stated `expire` value points at, in Unix seconds.
///
/// YouTube's signed addresses carry `expire` in seconds since the epoch. A
/// value that is not a plausible Unix time is not a deadline this build can
/// trust, so it is not used.
pub fn expiry_from_seconds(value: Option<&str>) -> Option<SystemTime> {
    let seconds: u64 = value?.trim().parse().ok()?;
    // Anything before 2020 or after 2100 is not a timestamp this provider
    // means, so a malformed parameter cannot make a lease look immortal.
    if !(1_577_836_800..=4_102_444_800).contains(&seconds) {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}

/// How long before its stated end a lease is treated as too old to rely on.
pub const EXPIRY_MARGIN: Duration = Duration::from_secs(30);

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(url: &str) -> RequestLease {
        RequestLease::new(
            url,
            Authority::Native,
            "iOS",
            Some("20.11.6".to_string()),
            Some("com.google.ios.youtube/20.11.6".to_string()),
            Credentials::at(origin_of(url)),
            Transport::Progressive,
            RepresentationFingerprint {
                id: "137".to_string(),
                container: "mp4".to_string(),
                codec: Some("avc1.640028".to_string()),
                bytes: Some(24_000_000),
            },
            None,
        )
    }

    #[test]
    fn a_signed_value_never_reaches_the_log() {
        let text = lease(
            "https://rr1.googlevideo.com/videoplayback?itag=137&expire=1700000000&sig=abc&n=x&ei=z",
        )
        .describe();
        for secret in ["abc", "1700000000", "n=x", "ei=z"] {
            assert!(!text.contains(secret), "{text} leaked {secret}");
        }
        assert!(text.contains("itag=137"), "{text} kept the harmless value");
        assert!(text.contains("rr1.googlevideo.com/videoplayback"));
    }

    #[test]
    fn a_nested_signed_url_is_masked_whole() {
        let text = redacted_target(
            "https://cdn.example.org/redirect?u=https%3A%2F%2Fother.example%2Fa%3Fsig%3Dsecret",
        );
        assert!(!text.contains("secret"), "{text}");
        assert!(text.contains("u=…"));
    }

    #[test]
    fn an_unreadable_address_is_not_echoed() {
        assert_eq!(redacted_target("not a url"), "an unreadable address");
    }

    #[test]
    fn a_fingerprint_separates_two_representations() {
        let one = lease("https://cdn.example.org/a").representation;
        let mut two = one.clone();
        assert_eq!(one.key(), two.key());
        two.id = "251".to_string();
        assert_ne!(one.key(), two.key());
        two = one.clone();
        two.bytes = Some(1);
        assert_ne!(one.key(), two.key());
    }

    #[test]
    fn page_headers_stay_on_their_origin_and_never_duplicate_the_client() {
        let lease = lease("https://rr1.googlevideo.com/videoplayback?itag=137");
        let page = vec![
            ("cookie".to_string(), "SID=secret".to_string()),
            (
                "user-agent".to_string(),
                "Mozilla/5.0 (browser)".to_string(),
            ),
            ("range".to_string(), "bytes=0-1".to_string()),
        ];
        let same = lease.headers_for(
            "https://rr1.googlevideo.com/videoplayback?itag=137",
            &page,
            Some("https://rr1.googlevideo.com"),
        );
        // A native lease was signed for its own client, so the page's headers
        // were never collected for this origin and are not admitted.
        assert!(same
            .iter()
            .all(|(name, _)| name.eq_ignore_ascii_case("user-agent")));
        assert!(same
            .iter()
            .any(|(_, value)| value.starts_with("com.google.ios.youtube")));

        let mut captured = lease.clone();
        captured.authority = Authority::Browser;
        captured.user_agent = None;
        captured.credentials = Credentials {
            origin: "https://rr1.googlevideo.com".to_string(),
            cookies: true,
        };
        let browser = captured.headers_for(
            "https://rr1.googlevideo.com/videoplayback?itag=137",
            &page,
            Some("https://rr1.googlevideo.com"),
        );
        assert!(browser.iter().any(|(name, _)| name == "cookie"));
        assert!(
            !browser.iter().any(|(name, _)| name == "range"),
            "a header the engine owns is never forwarded"
        );
        // A look-alike origin is refused.
        let elsewhere = captured.headers_for(
            "https://evil.example.org/x",
            &page,
            Some("https://rr1.googlevideo.com"),
        );
        assert!(elsewhere.is_empty());
    }

    #[test]
    fn an_expiry_is_only_read_when_it_is_a_real_date() {
        assert_eq!(expiry_from_seconds(Some("0")), None);
        assert_eq!(expiry_from_seconds(Some("nonsense")), None);
        assert_eq!(expiry_from_seconds(None), None);
        let at = expiry_from_seconds(Some("1893456000")).expect("a date in 2030");
        assert!(at > UNIX_EPOCH);
        let lease = RequestLease {
            expires_at: Some(at),
            ..lease("https://cdn.example.org/a")
        };
        assert!(!lease.expiring_at(EXPIRY_MARGIN, UNIX_EPOCH));
        assert!(
            lease.expiring_at(EXPIRY_MARGIN, at - EXPIRY_MARGIN),
            "the margin is what makes a renewal early rather than late"
        );
        assert!(lease.expiring_at(Duration::ZERO, at + Duration::from_secs(1)));
    }

    #[test]
    fn a_lease_without_a_deadline_never_expires() {
        let lease = lease("https://cdn.example.org/a");
        assert!(lease.expires_at.is_none());
        assert!(!lease.expiring_at(EXPIRY_MARGIN, UNIX_EPOCH));
        assert!(!lease.expiring_at(Duration::ZERO, SystemTime::now()));
    }
}
