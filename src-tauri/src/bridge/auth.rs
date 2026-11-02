//! The request checks the loopback bridge applies.
//!
//! There is no code to copy and no pairing to remember: the bridge is reached
//! only from this machine, only under its own address, and only by a caller that
//! says it is the browser extension. A web page cannot do that — its `Origin` is
//! the page's own address, which is refused, and a page cannot name an
//! extension's origin. That is the whole rule set, and it is what lets an
//! ordinary reader install the app, install the extension, and have them work
//! together without a single setting.
//!
//! The checks are pure functions over one request, so the whole rule set is
//! tested without opening a socket.

use super::http::Request;

/// The header every request carries. A web page cannot set it on a cross-origin
/// request without a preflight, and the preflight is refused unless the origin
/// is the extension's.
pub const CLIENT_HEADER: &str = "x-bdm-client";
pub const CLIENT_VALUE: &str = "browser-extension";

/// The port the bridge listens on, which is the only port a `Host` header may
/// name. A name that resolves to loopback but is not the app's own address is a
/// DNS-rebinding attempt and is refused at once.
const HOSTS: [&str; 2] = ["127.0.0.1:27500", "localhost:27500"];

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

    /// Nothing about this caller is the extension.
    pub const FORBIDDEN: Denial = Denial::new(403, "forbidden", "this is not the extension");
}

/// Check one request against the rules above.
///
/// An `OPTIONS` preflight carries neither the client header nor a body, so it is
/// let through once its origin and host are known to be the extension's.
pub fn check(request: &Request) -> Result<(), Denial> {
    if !host_allowed(request.header("host")) {
        return Err(Denial::new(
            403,
            "forbidden",
            "this address is not the application's bridge",
        ));
    }

    if let Some(origin) = request.header("origin") {
        if !is_extension_origin(origin) {
            return Err(Denial::FORBIDDEN);
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

    fn extension(method: &str) -> Request {
        request(
            method,
            &[
                ("host", "127.0.0.1:27500"),
                ("origin", "moz-extension://3c1e8f2a"),
                ("x-bdm-client", CLIENT_VALUE),
            ],
            b"",
        )
    }

    #[test]
    fn an_extension_request_passes_every_check() {
        assert_eq!(check(&extension("GET")), Ok(()));
        assert_eq!(check(&extension("POST")), Ok(()));
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
    fn a_page_origin_is_refused() {
        let page = request(
            "GET",
            &[
                ("host", "127.0.0.1:27500"),
                ("origin", "https://example.org"),
                ("x-bdm-client", CLIENT_VALUE),
            ],
            b"",
        );
        assert_eq!(check(&page).unwrap_err().status, 403);
    }

    #[test]
    fn a_rebinding_host_is_refused() {
        let host = request(
            "GET",
            &[
                ("host", "evil.example:27500"),
                ("origin", "moz-extension://3c1e8f2a"),
                ("x-bdm-client", CLIENT_VALUE),
            ],
            b"",
        );
        assert_eq!(check(&host).unwrap_err().status, 403);
    }

    #[test]
    fn the_extension_header_is_not_optional() {
        let nameless = request("GET", &[("host", "127.0.0.1:27500")], b"");
        assert_eq!(check(&nameless).unwrap_err().code, "forbidden");
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
        assert_eq!(check(&preflight), Ok(()));
    }

    #[test]
    fn a_post_with_a_body_must_say_it_is_json() {
        let headers = vec![
            ("host", "127.0.0.1:27500"),
            ("origin", "moz-extension://3c1e8f2a"),
            ("x-bdm-client", CLIENT_VALUE),
            ("content-type", "text/plain"),
        ];
        let request = request("POST", &headers, b"{}");
        assert_eq!(check(&request).unwrap_err().status, 415);
    }
}
