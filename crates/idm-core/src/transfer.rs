//! Where the bytes of a download come from.
//!
//! The engine drives every protocol the same way: it asks the source how big
//! the file is, it splits the file into ranges, and it opens a transfer for
//! the bytes of a range. HTTP and FTP differ only in how a range is asked
//! for and how big the file is.

use std::time::Duration;

use futures_util::Stream;

use crate::ftp;
use crate::http;

/// A transfer that failed. The extra fields drive the retry policy.
#[derive(Debug, Clone)]
pub struct TransferError {
    pub message: String,
    /// The status the server answered with, when there was one.
    pub status: Option<u16>,
    /// How long the server asked us to wait, when it said so.
    pub retry_after: Option<Duration>,
}

impl TransferError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: None,
            retry_after: None,
        }
    }
}

/// A stream of bytes, with the errors the source ran into.
pub type ByteStream = std::pin::Pin<Box<dyn Stream<Item = Result<Vec<u8>, String>> + Send>>;

/// What the server said about the file when we asked.
#[derive(Debug, Clone, Copy)]
pub struct Probe {
    /// Total size of the file. `None` means the server did not know it.
    pub total: Option<u64>,
    /// Whether the file can be read in ranges.
    pub ranges: bool,
}

/// A place to download from.
pub enum Source {
    Http {
        url: String,
        client: reqwest::Client,
    },
    Ftp {
        host: String,
        port: u16,
        user: String,
        password: String,
        path: String,
    },
}

impl Source {
    /// Read a URL and work out where it points.
    ///
    /// `proxy` is the proxy the user set, if there is one. Both HTTP and
    /// SOCKS proxies are addresses that reqwest understands.
    pub fn parse(url: &str, proxy: Option<&str>) -> Result<Self, String> {
        if let Some(rest) = url.strip_prefix("ftp://") {
            return Self::parse_ftp(rest);
        }
        if url.starts_with("http://") || url.starts_with("https://") {
            let mut builder = reqwest::Client::builder()
                .user_agent(concat!("my-idm-clone/", env!("CARGO_PKG_VERSION")))
                .redirect(reqwest::redirect::Policy::limited(10));

            if let Some(proxy) = proxy {
                let proxy = reqwest::Proxy::all(proxy)
                    .map_err(|e| format!("the proxy address is not usable: {e}"))?;
                builder = builder.proxy(proxy);
            }

            let client = builder.build().map_err(|e| e.to_string())?;
            return Ok(Source::Http {
                url: url.to_string(),
                client,
            });
        }
        Err("the URL must start with http://, https:// or ftp://".to_string())
    }

    /// `ftp://[user[:password]@]host[:port]/path`
    fn parse_ftp(rest: &str) -> Result<Self, String> {
        let (credentials, rest) = match rest.rsplit_once('@') {
            Some((credentials, rest)) => (Some(credentials), rest),
            None => (None, rest),
        };

        let (user, password) = match credentials {
            Some(credentials) => match credentials.split_once(':') {
                Some((user, password)) => (user.to_string(), password.to_string()),
                None => (credentials.to_string(), String::new()),
            },
            None => ("anonymous".to_string(), "anonymous@".to_string()),
        };

        let (host_port, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, "/"),
        };
        let (host, port) = match host_port.rsplit_once(':') {
            Some((host, port)) => (
                host.to_string(),
                port.parse::<u16>().map_err(|_| "the port must be a number")?,
            ),
            None => (host_port.to_string(), 21),
        };

        if host.is_empty() {
            return Err("the URL has no host".to_string());
        }

        Ok(Source::Ftp {
            host,
            port,
            user,
            password,
            path: path.to_string(),
        })
    }

    /// Ask the server how big the file is.
    pub async fn probe(&self) -> Result<Probe, TransferError> {
        match self {
            Source::Http { url, client } => http::probe(client, url).await,
            Source::Ftp {
                host,
                port,
                user,
                password,
                path,
            } => {
                let total = ftp::size(host, *port, user, password, path)
                    .await
                    .map_err(TransferError::msg)?;
                Ok(Probe {
                    total: Some(total),
                    ranges: true,
                })
            }
        }
    }

    /// Open a transfer that yields the bytes of a range.
    ///
    /// `to` of `None` means "to the end of the file".
    pub async fn open(
        &self,
        from: u64,
        to: Option<u64>,
        covers_all: bool,
    ) -> Result<ByteStream, TransferError> {
        match self {
            Source::Http { url, client } => http::open_range(client, url, from, to, covers_all).await,
            Source::Ftp {
                host,
                port,
                user,
                password,
                path,
            } => ftp::open_range(host, *port, user, password, path, from, to)
                .await
                .map_err(TransferError::msg),
        }
    }
}

/// The URL without the password.
///
/// The manifest of a download lies next to the file, which may sit in a
/// folder that is shared, so the password is not written there.
pub fn redacted(url: &str) -> String {
    match url.split_once("://") {
        Some(("ftp", rest)) => match rest.split_once('@') {
            Some((credentials, host)) => match credentials.split_once(':') {
                Some((user, _)) => format!("ftp://{user}:***@{host}"),
                None => url.to_string(),
            },
            None => url.to_string(),
        },
        _ => url.to_string(),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ftp_url_is_read() {
        let source = Source::parse("ftp://user:secret@example.com/pub/file.bin", None).expect("ftp url");
        match source {
            Source::Ftp {
                host,
                port,
                user,
                password,
                path,
            } => {
                assert_eq!(host, "example.com");
                assert_eq!(port, 21);
                assert_eq!(user, "user");
                assert_eq!(password, "secret");
                assert_eq!(path, "/pub/file.bin");
            }
            Source::Http { .. } => panic!("the url must be an ftp url"),
        }
    }

    #[test]
    fn an_ftp_url_without_a_port_uses_the_default_one() {
        let source = Source::parse("ftp://example.com/file.bin", None).expect("ftp url");
        match source {
            Source::Ftp { host, port, user, path, .. } => {
                assert_eq!(host, "example.com");
                assert_eq!(port, 21);
                assert_eq!(user, "anonymous");
                assert_eq!(path, "/file.bin");
            }
            Source::Http { .. } => panic!("the url must be an ftp url"),
        }
    }

    #[test]
    fn an_ftp_url_with_a_port_is_read() {
        let source = Source::parse("ftp://example.com:2121/file.bin", None).expect("ftp url");
        match source {
            Source::Ftp { port, .. } => assert_eq!(port, 2121),
            Source::Http { .. } => panic!("the url must be an ftp url"),
        }
    }

    #[test]
    fn a_url_of_another_scheme_is_refused() {
        assert!(Source::parse("magnet:?xt=urn:btih:1", None).is_err());
        assert!(Source::parse("file:///c:/windows", None).is_err());
    }

    #[test]
    fn an_http_url_is_read() {
        assert!(matches!(
            Source::parse("https://example.com/a.bin", None).expect("http url"),
            Source::Http { .. }
        ));
    }

    #[test]
    fn a_password_is_not_written_to_disk() {
        assert_eq!(
            redacted("ftp://user:secret@example.com/file.bin"),
            "ftp://user:***@example.com/file.bin"
        );
    }

    #[test]
    fn a_url_without_a_password_is_left_alone() {
        assert_eq!(
            redacted("https://example.com/file.bin"),
            "https://example.com/file.bin"
        );
        assert_eq!(
            redacted("ftp://example.com/file.bin"),
            "ftp://example.com/file.bin"
        );
    }

    #[test]
    fn a_proxy_that_cannot_be_used_is_reported() {
        let error = match Source::parse("https://example.com/a.bin", Some("not a proxy")) {
            Ok(_) => panic!("the proxy must be refused"),
            Err(error) => error,
        };
        assert!(error.contains("proxy"), "error was: {error}");
    }
}
