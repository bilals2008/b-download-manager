//! The pairing token and the request checks the loopback bridge applies.
//!
//! The extension is identified by a token the application generates and the
//! reader pastes into the extension once. The token is the real control: the
//! origin and client headers keep a web page out, but a local program can set
//! any header it likes, so only the token proves the caller is the extension
//! the reader paired.
//!
//! The checks are pure functions over one request, so the whole rule set is
//! tested without opening a socket.

use getrandom::fill;

use super::http::Request;

/// The header every request carries. A web page cannot set it on a cross-origin
/// request without a preflight, and the preflight is refused unless the origin
/// is the extension's.
pub const CLIENT_HEADER: &str = "x-bdm-client";
pub const CLIENT_VALUE: &str = "browser-extension";

/// The port the bridge listens on, which is the only port a `Host` header may
/// name. A name that resolves to loopback but is not the app's own address is
/// a DNS-rebinding attempt and is refused before the token is read.
const HOSTS: [&str; 2] = ["127.0.0.1:27500", "localhost:27500"];

/// A token is 32 random bytes written as hex.
const TOKEN_BYTES: usize = 32;
const TOKEN_LENGTH: usize = TOKEN_BYTES * 2;

/// One reason a request was refused, with the status and code the extension
/// turns into one short line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Denial {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}

impl Denial {
    const fn new(status: u16, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
        }
    }

    /// The token matched, but the extension that presented it is not the one
    /// this pairing already belongs to. A second extension that learned the
    /// secret gets this, and nothing else.
    pub const FOREIGN_EXTENSION: Denial = Denial::new(
        403,
        "forbidden_extension",
        "this extension is not the one paired with the application",
    );
}

/// Check everything that does not need the application's state.
///
/// `needs_token` is false for the ping, which the extension calls before it is
/// paired; every other route needs a matching bearer token. An `OPTIONS`
/// preflight carries neither the client header nor the token, so it is let
/// through here once its origin and host are known to be the extension's.
pub fn check(request: &Request, token: &str, needs_token: bool) -> Result<(), Denial> {
    if !host_allowed(request.header("host")) {
        return Err(Denial::new(
            403,
            "forbidden",
            "this address is not the application's bridge",
        ));
    }

    if let Some(origin) = request.header("origin") {
        if !is_extension_origin(origin) {
            return Err(Denial::new(
                403,
                "forbidden",
                "this origin is not the extension",
            ));
        }
    }

    if request.method == "OPTIONS" {
        return Ok(());
    }

    if request.header(CLIENT_HEADER) != Some(CLIENT_VALUE) {
        return Err(Denial::new(
            403,
            "forbidden",
            "the browser extension header is missing",
        ));
    }

    if needs_token && !is_authorized(request, token) {
        return Err(Denial::new(
            401,
            "unauthorized",
            "the extension is not paired with the application",
        ));
    }

    if request.method == "POST"
        && !request.body.is_empty()
        && !is_json(request.header("content-type"))
    {
        return Err(Denial::new(
            415,
            "unsupported_media_type",
            "a request body must be JSON",
        ));
    }

    Ok(())
}

/// Whether the request carries the token the application generated.
pub fn is_authorized(request: &Request, token: &str) -> bool {
    let Some(provided) = bearer(request.header("authorization")) else {
        return false;
    };
    token_matches(token, provided)
}

/// A fresh pairing token, written as lower-case hex. An empty answer means the
/// operating system's random source failed; the bridge then refuses every
/// authenticated request rather than fall back to a guessable token.
pub fn new_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    if fill(&mut bytes).is_err() {
        idm_core::log::write(
            "warn",
            "the pairing token could not be generated: no random source",
        );
        return String::new();
    }

    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut token = String::with_capacity(TOKEN_LENGTH);
    for byte in bytes {
        token.push(HEX[(byte >> 4) as usize] as char);
        token.push(HEX[(byte & 0x0f) as usize] as char);
    }
    token
}

/// Compare two tokens without returning early on the first different byte, so
/// the time taken does not say how much of a guess was right.
pub fn token_matches(expected: &str, provided: &str) -> bool {
    if expected.is_empty() || expected.len() != provided.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in expected.as_bytes().iter().zip(provided.as_bytes()) {
        difference |= left ^ right;
    }
    difference == 0
}

/// The token of an `Authorization: Bearer <token>` header.
pub fn bearer(header: Option<&str>) -> Option<&str> {
    let (scheme, value) = header?.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let value = value.trim();
    (!value.is_empty() && !value.contains(' ')).then_some(value)
}

/// An extension origin. Firefox uses `moz-extension://<uuid>`, Chromium
/// `chrome-extension://<id>`; nothing else may follow the scheme.
pub fn is_extension_origin(origin: &str) -> bool {
    let Some(rest) = origin
        .strip_prefix("moz-extension://")
        .or_else(|| origin.strip_prefix("chrome-extension://"))
    else {
        return false;
    };
    !rest.is_empty()
        && rest.len() <= 128
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Whether the `Host` header names the bridge itself.
pub fn host_allowed(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    HOSTS.iter().any(|known| host.eq_ignore_ascii_case(known))
}

/// Whether a `Content-Type` header names JSON. A charset parameter is fine.
pub fn is_json(header: Option<&str>) -> bool {
    let Some(value) = header else {
        return false;
    };
    value
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .eq_ignore_ascii_case("application/json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: &str, headers: &[(&str, &str)], body: &[u8]) -> Request {
        Request::for_test(method, "/v1/downloads", headers, body)
    }

    fn paired(method: &str) -> Request {
        request(
            method,
            &[
                ("host", "127.0.0.1:27500"),
                ("origin", "moz-extension://3c1e8f2a"),
                ("x-bdm-client", CLIENT_VALUE),
                ("authorization", "Bearer secret"),
            ],
            b"",
        )
    }

    #[test]
    fn a_fresh_token_is_hex_and_long_enough() {
        let token = new_token();
        assert_eq!(token.len(), TOKEN_LENGTH);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(token, new_token(), "two tokens must differ");
    }

    #[test]
    fn a_token_is_compared_whole() {
        assert!(token_matches("secret", "secret"));
        assert!(!token_matches("secret", "secreT"));
        assert!(!token_matches("secret", "secret2"));
        assert!(!token_matches("", ""));
        assert!(!token_matches("secret", ""), "an empty token never matches");
    }

    #[test]
    fn only_a_bearer_token_counts() {
        assert_eq!(bearer(Some("Bearer abc")), Some("abc"));
        assert_eq!(bearer(Some("bearer abc")), Some("abc"));
        assert_eq!(bearer(Some("Basic abc")), None);
        assert_eq!(bearer(Some("Bearer")), None);
        assert_eq!(bearer(Some("Bearer ")), None);
        assert_eq!(bearer(None), None);
    }

    #[test]
    fn only_an_extension_origin_is_an_extension_origin() {
        assert!(is_extension_origin("moz-extension://3c1e8f2a"));
        assert!(is_extension_origin("chrome-extension://abc"));
        assert!(!is_extension_origin("moz-extension://"));
        assert!(!is_extension_origin("moz-extension://a/path"));
        assert!(!is_extension_origin("moz-extension://a b"));
        assert!(!is_extension_origin("https://example.org"));
        assert!(!is_extension_origin("null"));
    }

    #[test]
    fn the_host_must_be_the_bridge_itself() {
        assert!(host_allowed(Some("127.0.0.1:27500")));
        assert!(host_allowed(Some("localhost:27500")));
        assert!(host_allowed(Some("LocalHost:27500")));
        assert!(!host_allowed(Some("evil.example:27500")));
        assert!(!host_allowed(Some("127.0.0.1:27501")));
        assert!(!host_allowed(None));
    }

    #[test]
    fn a_body_has_to_be_json() {
        assert!(is_json(Some("application/json")));
        assert!(is_json(Some("application/json; charset=utf-8")));
        assert!(!is_json(Some("text/plain")));
        assert!(!is_json(None));
    }

    #[test]
    fn a_paired_request_passes_every_check() {
        assert_eq!(check(&paired("GET"), "secret", true), Ok(()));
        assert_eq!(check(&paired("POST"), "secret", true), Ok(()));
    }

    #[test]
    fn a_request_without_the_token_is_unauthorized() {
        let request = request(
            "GET",
            &[("host", "127.0.0.1:27500"), ("x-bdm-client", CLIENT_VALUE)],
            b"",
        );
        assert_eq!(check(&request, "secret", true).unwrap_err().status, 401);
        assert_eq!(check(&request, "secret", false), Ok(()), "the ping is open");
    }

    #[test]
    fn a_foreign_origin_or_host_is_refused() {
        let origin = request(
            "GET",
            &[
                ("host", "127.0.0.1:27500"),
                ("origin", "https://example.org"),
                ("x-bdm-client", CLIENT_VALUE),
                ("authorization", "Bearer secret"),
            ],
            b"",
        );
        assert_eq!(check(&origin, "secret", true).unwrap_err().status, 403);

        let host = request(
            "GET",
            &[
                ("host", "evil.example:27500"),
                ("origin", "moz-extension://3c1e8f2a"),
                ("x-bdm-client", CLIENT_VALUE),
                ("authorization", "Bearer secret"),
            ],
            b"",
        );
        assert_eq!(check(&host, "secret", true).unwrap_err().status, 403);
    }

    #[test]
    fn a_preflight_needs_only_its_origin_and_host() {
        let preflight = request(
            "OPTIONS",
            &[
                ("host", "127.0.0.1:27500"),
                ("origin", "moz-extension://3c1e8f2a"),
            ],
            b"",
        );
        assert_eq!(check(&preflight, "secret", true), Ok(()));
    }

    #[test]
    fn a_post_with_a_body_must_say_it_is_json() {
        let request = request(
            "POST",
            &[
                ("host", "127.0.0.1:27500"),
                ("x-bdm-client", CLIENT_VALUE),
                ("authorization", "Bearer secret"),
                ("content-type", "text/plain"),
            ],
            b"{}",
        );
        assert_eq!(check(&request, "secret", true).unwrap_err().status, 415);
    }
}
