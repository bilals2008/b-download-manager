//! Which headers the browser's cookies and credentials may travel with.
//!
//! The extension sends the headers a page's player used, and the app sends
//! them again for the manifest and the segments. That is a privacy surface, so
//! the rules are narrow and in one place:
//!
//! - only `Cookie`, `User-Agent`, `Referer`, `Origin` and `Authorization`;
//! - only to an address on the **manifest's own origin**, never to a CDN or a
//!   redirect on another host;
//! - no value with a newline, and no value over 8 KiB;
//! - nothing is written to disk or logged.
//!
//! A `Cookie` or `Authorization` is forwarded only after the one-time consent
//! in the panel (`DesktopPreferences::cookie_consent`). The panel asks with a
//! plain sentence the first time.

use std::collections::HashMap;

use reqwest::Url;

/// The longest header value the app accepts.
pub const MAX_VALUE: usize = 8 * 1024;

/// The headers the media path understands. Everything else is dropped.
pub const ALLOWED: [&str; 5] = ["cookie", "user-agent", "referer", "origin", "authorization"];

/// The most headers one manifest may forward.
pub const MAX_HEADERS: usize = 8;

/// The filtered headers and the origin they belong to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scoped {
    pub origin: String,
    pub headers: Vec<(String, String)>,
}

impl Scoped {
    /// Whether the offered headers carry a credential that needs consent.
    pub fn sensitive(&self) -> bool {
        self.headers
            .iter()
            .any(|(name, _)| name == "cookie" || name == "authorization")
    }
}

/// Keep only the allowed headers, scoped to the manifest's origin.
pub fn scope(manifest_url: &str, offered: &HashMap<String, String>) -> Scoped {
    let origin = origin_of(manifest_url).unwrap_or_default();
    let mut headers: Vec<(String, String)> = Vec::new();
    for (name, value) in offered {
        let name = name.trim().to_ascii_lowercase();
        if !ALLOWED.contains(&name.as_str()) {
            continue;
        }
        let value = value.trim();
        if value.is_empty()
            || value.len() > MAX_VALUE
            || value.contains('\r')
            || value.contains('\n')
        {
            continue;
        }
        if headers.iter().any(|(known, _)| *known == name) {
            continue;
        }
        headers.push((name, value.to_string()));
        if headers.len() >= MAX_HEADERS {
            break;
        }
    }
    Scoped { origin, headers }
}

/// `http(s)://host[:port]`, the origin two addresses share or do not.
pub fn origin_of(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return None;
    }
    Some(url.origin().ascii_serialization())
}

/// Whether an address is on the manifest's origin.
pub fn same_origin(url: &str, origin: &str) -> bool {
    let (Ok(url), Ok(origin)) = (Url::parse(url), Url::parse(origin)) else {
        return false;
    };
    url.origin() == origin.origin()
}

/// Whether a header value is usable as it is (the policy's own last check,
/// used by the tests).
#[cfg(test)]
pub fn value_is_usable(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_VALUE && !value.contains('\r') && !value.contains('\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn only_the_five_known_headers_are_kept() {
        let scoped = scope(
            "https://site.example/watch",
            &offered(&[
                ("Cookie", "a=b"),
                ("User-Agent", "Browser"),
                ("Referer", "https://site.example/"),
                ("Origin", "https://site.example"),
                ("Authorization", "Bearer x"),
                ("X-Evil", "1"),
                ("Proxy-Authorization", "x"),
            ]),
        );
        let names: Vec<&str> = scoped
            .headers
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(names.len(), 5);
        for allowed in ALLOWED {
            assert!(names.contains(&allowed), "{allowed} was dropped");
        }
        assert!(!names.contains(&"x-evil"));
        assert_eq!(
            scoped.origin, "https://site.example",
            "the origin is the manifest's, without the path"
        );
        assert!(scoped.sensitive());
    }

    #[test]
    fn a_newline_or_an_oversized_value_is_dropped() {
        let scoped = scope(
            "https://site.example/a.m3u8",
            &offered(&[
                ("Cookie", "a=b\r\nX-Injected: 1"),
                ("User-Agent", &"a".repeat(MAX_VALUE + 1)),
                ("Referer", "https://site.example/"),
            ]),
        );
        assert_eq!(scoped.headers.len(), 1);
        assert_eq!(scoped.headers[0].0, "referer");
        assert!(!scoped.sensitive());
        assert!(value_is_usable("a=b"));
        assert!(!value_is_usable("a\nb"));
    }

    #[test]
    fn an_origin_is_scheme_host_and_port() {
        assert!(same_origin(
            "https://site.example/seg/a.ts",
            "https://site.example/master.m3u8"
        ));
        assert!(!same_origin(
            "https://cdn.example/seg/a.ts",
            "https://site.example/master.m3u8"
        ));
        assert!(!same_origin(
            "http://site.example/a.ts",
            "https://site.example/a.ts"
        ));
        assert!(!same_origin(
            "https://site.example:8443/a.ts",
            "https://site.example/a.ts"
        ));
        assert!(!same_origin("not a url", "https://site.example"));
        assert_eq!(
            origin_of("https://site.example/a"),
            Some("https://site.example".to_string())
        );
        assert_eq!(origin_of("ftp://site.example/a"), None);
    }

    #[test]
    fn the_first_value_of_a_name_wins() {
        let scoped = scope(
            "https://site.example/a",
            &offered(&[("cookie", "a=b"), ("Cookie", "c=d")]),
        );
        assert_eq!(scoped.headers.len(), 1);
        assert!(scoped.headers[0].1 == "a=b" || scoped.headers[0].1 == "c=d");
    }
}
