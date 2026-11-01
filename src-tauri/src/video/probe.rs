//! The one HTTP client the media path uses to read a manifest.
//!
//! A manifest is a page-sized text document fetched by a user-supplied address,
//! so it follows the Site Grabber's rules: http and https only, every hop
//! checked against the public-address policy, at most five redirects, and a
//! body cap. The redirects are followed here rather than by `reqwest`, so each
//! hop can be checked before it is requested.

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{ACCEPT_ENCODING, CONTENT_RANGE, RANGE, REFERER};
use reqwest::redirect::Policy;
use reqwest::Url;

use idm_core::http;
use idm_core::media::MAX_MANIFEST_BYTES;

use crate::grabber::policy as grabber_policy;

use super::policy;

/// How many redirects a manifest may travel through.
pub const MAX_REDIRECTS: usize = 5;

/// How long one manifest request may take, body included.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// A manifest as it arrived.
#[derive(Debug, Clone)]
pub struct Fetched {
    /// The address the bytes really came from, after redirects. Relative
    /// addresses inside the manifest resolve against this one.
    pub url: String,
    pub text: String,
}

/// What a `Range: bytes=0-0` answer says about a file candidate.
#[derive(Debug, Clone)]
pub struct FileProbe {
    pub url: String,
    pub total: Option<u64>,
    pub content_type: Option<String>,
}

/// Read a manifest with every hop checked.
pub async fn fetch(
    url: &str,
    referer: &str,
    headers: &[(String, String)],
    origin: Option<&str>,
) -> Result<Fetched, String> {
    let client = client()?;
    let mut current = Url::parse(url).map_err(|_| "the manifest address is not usable")?;
    let referer = web_url(referer);

    for _ in 0..=MAX_REDIRECTS {
        grabber_policy::check_public(&current).await?;
        let mut request = client
            .get(current.clone())
            .header(ACCEPT_ENCODING, http::IDENTITY_ENCODING);
        if let Some(referer) = referer.as_deref() {
            request = request.header(REFERER, referer);
        }
        request = with_headers(request, scoped(current.as_str(), headers, origin));

        let response = request
            .send()
            .await
            .map_err(|error| format!("the manifest could not be read: {error}"))?;
        let status = response.status();

        if status.is_redirection() {
            let Some(location) = response.headers().get(reqwest::header::LOCATION) else {
                return Err("the site answered a redirect without an address".to_string());
            };
            let location = location
                .to_str()
                .map_err(|_| "the redirect address is not readable".to_string())?;
            current = current
                .join(location)
                .map_err(|_| "the redirect address is not usable".to_string())?;
            continue;
        }
        if !status.is_success() {
            return Err(http::status_message(status.as_u16()));
        }

        if let Some(length) = response.content_length() {
            if length > MAX_MANIFEST_BYTES {
                return Err("the manifest is too large to read".to_string());
            }
        }
        let mut body: Vec<u8> = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(item) = stream.next().await {
            let bytes = item.map_err(|error| format!("the manifest ended early: {error}"))?;
            if body.len() as u64 + bytes.len() as u64 > MAX_MANIFEST_BYTES {
                return Err("the manifest is too large to read".to_string());
            }
            body.extend_from_slice(&bytes);
        }

        let text = String::from_utf8_lossy(&body).to_string();
        if text.trim().is_empty() {
            return Err("the site sent an empty manifest".to_string());
        }
        return Ok(Fetched {
            url: current.to_string(),
            text,
        });
    }

    Err(format!(
        "the site redirected more than {MAX_REDIRECTS} times"
    ))
}

/// Ask a file candidate for its size and type, without downloading it.
pub async fn probe_file(
    url: &str,
    referer: &str,
    headers: &[(String, String)],
    origin: Option<&str>,
) -> Result<FileProbe, String> {
    let client = client()?;
    let mut current = Url::parse(url).map_err(|_| "the address is not usable")?;
    let referer = web_url(referer);

    for _ in 0..=MAX_REDIRECTS {
        grabber_policy::check_public(&current).await?;
        let mut request = client
            .get(current.clone())
            .header(RANGE, "bytes=0-0")
            .header(ACCEPT_ENCODING, http::IDENTITY_ENCODING);
        if let Some(referer) = referer.as_deref() {
            request = request.header(REFERER, referer);
        }
        request = with_headers(request, scoped(current.as_str(), headers, origin));

        let response = request
            .send()
            .await
            .map_err(|error| format!("the file could not be reached: {error}"))?;
        let status = response.status();
        if status.is_redirection() {
            let Some(location) = response.headers().get(reqwest::header::LOCATION) else {
                return Err("the site answered a redirect without an address".to_string());
            };
            let location = location
                .to_str()
                .map_err(|_| "the redirect address is not readable".to_string())?;
            current = current
                .join(location)
                .map_err(|_| "the redirect address is not usable".to_string())?;
            continue;
        }
        if !status.is_success() && status.as_u16() != 416 {
            return Err(http::status_message(status.as_u16()));
        }

        let total = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(http::parse_content_range_total)
            .or_else(|| {
                status
                    .is_success()
                    .then(|| response.content_length())
                    .flatten()
            });

        return Ok(FileProbe {
            url: current.to_string(),
            total,
            content_type: header(&response, reqwest::header::CONTENT_TYPE),
        });
    }

    Err(format!(
        "the file redirected more than {MAX_REDIRECTS} times"
    ))
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(http::USER_AGENT)
        .redirect(Policy::none())
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| format!("the media client could not be made: {error}"))
}

fn web_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with("http://") || value.starts_with("https://") {
        Some(value.to_string())
    } else {
        None
    }
}

fn header(response: &reqwest::Response, name: reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The headers that may travel to this hop: only the ones the page sent for
/// the manifest's own origin.
fn scoped<'a>(
    url: &str,
    headers: &'a [(String, String)],
    origin: Option<&str>,
) -> &'a [(String, String)] {
    match origin {
        Some(origin) if policy::same_origin(url, origin) => headers,
        _ => &[],
    }
}

/// Add the origin-scoped headers the extension forwarded.
///
/// The filter that decides which headers may travel lives in `video::policy`;
/// this only writes the ones it let through.
fn with_headers(
    mut request: reqwest::RequestBuilder,
    headers: &[(String, String)],
) -> reqwest::RequestBuilder {
    for (name, value) in headers {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_bytes()),
            reqwest::header::HeaderValue::from_str(value),
        ) {
            request = request.header(name, value);
        }
    }
    request
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_web_address_is_a_referer() {
        assert!(web_url("https://example.org/watch").is_some());
        assert!(web_url("").is_none());
        assert!(web_url("javascript:alert(1)").is_none());
    }

    #[test]
    fn a_bad_manifest_address_is_refused_before_a_request() {
        assert!(Url::parse("not an address").is_err());
    }
}
