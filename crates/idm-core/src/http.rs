//! Everything that speaks HTTP.
//!
//! IDM opens raw sockets and terminates TLS itself, but the wire format is
//! what matters here: every request carries an explicit `Range:` header and
//! `Accept-Encoding: identity`, so the bytes on the wire line up with the byte
//! offsets in the file. See `ANALYSIS.md` for the evidence.

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use reqwest::header::{ACCEPT_ENCODING, CONTENT_RANGE, RANGE, RETRY_AFTER};

use crate::transfer::{ByteStream, Probe, TransferError};

/// IDM asks for an identity encoding so that the byte offsets on the wire
/// stay equal to the byte offsets in the file.
pub const IDENTITY_ENCODING: &str = "identity";

/// The User-Agent IDM sends, read out of the string table of `IDMan.exe`.
///
/// A server that only serves browsers answers `403 Forbidden` to anything
/// else: the video links of Google are the everyday example, and a download
/// manager that identifies itself honestly is refused there.
pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 6.1; WOW64) AppleWebKit/537.36 (KHTML, like Gecko)";

/// Ask the server for the first byte only.
///
/// The answer reveals the total size and proves whether the server honours
/// `Range` requests. This is the same probe IDM performs before it decides
/// how many connections it may open.
pub async fn probe(client: &reqwest::Client, url: &str) -> Result<Probe, TransferError> {
    let response = client
        .get(url)
        .header(RANGE, "bytes=0-0")
        .header(ACCEPT_ENCODING, IDENTITY_ENCODING)
        .send()
        .await
        .map_err(|e| TransferError::msg(format!("cannot reach the server: {e}")))?;

    let status = response.status();
    let retry_after = parse_retry_after(response.headers());

    // A server that says no has not told us the size of the file. The body
    // of such an answer is a message, not the file: a server that is busy
    // answers `429` with a body of one byte, and that byte is not the size
    // of the file.
    if !status.is_success() {
        return Err(TransferError {
            message: status_message(status.as_u16()),
            status: Some(status.as_u16()),
            retry_after,
        });
    }

    let ranges = status == reqwest::StatusCode::PARTIAL_CONTENT;

    let mut total = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range_total);

    // A `206` without a usable `Content-Range` tells us nothing about the
    // size, and its `Content-Length` is the length of the one byte we asked
    // for. Only a full `200` carries the length of the whole file.
    if total.is_none() && !ranges {
        total = response.content_length();
    }

    Ok(Probe { total, ranges })
}

/// Open a transfer that yields the bytes of a range.
///
/// `to` of `None` means "to the end of the file". `covers_all` says whether
/// this range is the whole file: a server that answers `200` instead of
/// `206` sent the whole file, and that is only usable when the whole file is
/// what this range holds.
pub async fn open_range(
    client: &reqwest::Client,
    url: &str,
    from: u64,
    to: Option<u64>,
    covers_all: bool,
) -> Result<ByteStream, TransferError> {
    // IDM always asks for an explicit range and always asks for an identity
    // encoding so the bytes on the wire line up with the file offsets.
    let response = client
        .get(url)
        .header(RANGE, range_header(from, to))
        .header(ACCEPT_ENCODING, IDENTITY_ENCODING)
        .send()
        .await
        .map_err(|e| TransferError::msg(e.to_string()))?;

    let status = response.status();
    let retry_after = parse_retry_after(response.headers());

    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        // The range is already complete.
        return Ok(empty_stream());
    }
    if !status.is_success() {
        return Err(TransferError {
            message: status_message(status.as_u16()),
            status: Some(status.as_u16()),
            retry_after,
        });
    }
    if status == reqwest::StatusCode::OK && !covers_all {
        // The server ignored the range and sent the whole file. The download
        // would end up short.
        return Err(TransferError::msg("the server ignored the Range request"));
    }

    // Verify the server really served the range we asked for. IDM checks the
    // `Content-Range` reply in `HTTPConnection::GetRetrRangeReply()`.
    if let Some(value) = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(served_start) = parse_content_range_start(value) {
            if served_start != from {
                return Err(TransferError::msg(format!(
                    "the server served bytes from {served_start} instead of {from}"
                )));
            }
        }
    }

    let stream = response
        .bytes_stream()
        .map(|item| item.map(|bytes| bytes.to_vec()).map_err(|e| e.to_string()))
        .boxed();

    Ok(stream)
}

/// A transfer that is over before it began.
fn empty_stream() -> ByteStream {
    Box::pin(futures_util::stream::empty())
}

/// The `Range` header value for a byte range.
///
/// `to` of `None` means "to the end of the file", which is what a server that
/// does not know the size needs.
pub fn range_header(from: u64, to: Option<u64>) -> String {
    match to {
        Some(to) => format!("bytes={from}-{to}"),
        None => format!("bytes={from}-"),
    }
}

/// Read the total size out of a `Content-Range` value.
///
/// `bytes 0-0/1234` gives `Some(1234)`. `bytes 0-0/*` gives `None`, which is
/// how a server says it does not know the size.
pub fn parse_content_range_total(value: &str) -> Option<u64> {
    let (_, spec) = value.trim().split_once(' ')?;
    let (_, total) = spec.trim().split_once('/')?;
    let total = total.trim();
    if total == "*" {
        None
    } else {
        total.parse().ok()
    }
}

/// Read the first byte offset out of a `Content-Range` value.
///
/// IDM checks the reply of a range request against the range it asked for
/// (`HTTPConnection::GetRetrRangeReply()`), so the engine has to be able to
/// read back what the server actually served.
pub fn parse_content_range_start(value: &str) -> Option<u64> {
    let (_, spec) = value.trim().split_once(' ')?;
    let (range, _) = spec.trim().split_once('/')?;
    let (start, _) = range.trim().split_once('-')?;
    start.trim().parse().ok()
}

/// How long the server asked us to wait before trying again.
///
/// shortcut: only the delta-seconds form of `Retry-After` is honoured. The
/// HTTP-date form needs a date parser this crate does not carry.
pub fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let seconds: u64 = value.parse().ok()?;
    Some(Duration::from_secs(seconds))
}

/// The wait before the next attempt after a failure.
///
/// IDM waits a fixed 400 ms between attempts. An exponential wait with a cap
/// costs nothing to implement and does not hammer a struggling server.
pub fn backoff(attempt: u32) -> Duration {
    const BASE_MS: u64 = 400;
    const CAP_MS: u64 = 30_000;

    let shift = attempt.saturating_sub(1).min(7);
    Duration::from_millis(BASE_MS.saturating_mul(1u64 << shift).min(CAP_MS))
}

/// What the answer of the server means, in words a user can act on.
///
/// The status code alone tells a user nothing. "The link may have expired"
/// and "the site only answers a browser" are the two answers that matter
/// most in practice, and both are refusals.
pub fn status_message(status: u16) -> String {
    match status {
        401 => "the server asked us to sign in (401)".to_string(),
        403 => "the server refused the request (403); the link may have expired, or the site only answers a browser".to_string(),
        404 => "the file is not on the server any more (404)".to_string(),
        410 => "the file is gone for good (410)".to_string(),
        416 => "the server has no such range of bytes (416)".to_string(),
        429 => "the server is busy (429) and asked us to come back later".to_string(),
        500..=599 => format!("the server had a problem ({status})"),
        _ => format!("the server answered {status}"),
    }
}

/// Whether the server told us to come back later, so the wait is the
/// server's choice rather than ours.
pub fn is_retryable(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn the_total_size_is_read_out_of_a_content_range() {
        assert_eq!(parse_content_range_total("bytes 0-0/1234"), Some(1234));
        assert_eq!(parse_content_range_total("bytes 100-199/1234"), Some(1234));
    }

    #[test]
    fn an_unknown_size_is_not_a_number() {
        assert_eq!(parse_content_range_total("bytes 0-0/*"), None);
        assert_eq!(parse_content_range_total("nonsense"), None);
    }

    #[test]
    fn the_start_offset_is_read_out_of_a_content_range() {
        assert_eq!(parse_content_range_start("bytes 4096-8191/1234"), Some(4096));
        assert_eq!(parse_content_range_start("bytes */1234"), None);
    }

    #[test]
    fn a_range_to_the_end_has_no_end() {
        assert_eq!(range_header(10, Some(20)), "bytes=10-20");
        assert_eq!(range_header(10, None), "bytes=10-");
    }

    #[test]
    fn retry_after_is_read_in_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("120"));
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(120)));
    }

    #[test]
    fn a_missing_retry_after_is_not_an_error() {
        assert_eq!(parse_retry_after(&HeaderMap::new()), None);
    }

    #[test]
    fn the_backoff_grows_and_then_stops_growing() {
        assert_eq!(backoff(1), Duration::from_millis(400));
        assert_eq!(backoff(2), Duration::from_millis(800));
        assert_eq!(backoff(3), Duration::from_millis(1600));
        assert_eq!(backoff(20), Duration::from_millis(30_000));
    }

    #[test]
    fn only_transient_statuses_are_retryable() {
        assert!(is_retryable(429));
        assert!(is_retryable(503));
        assert!(!is_retryable(404));
        assert!(!is_retryable(416));
    }

    #[test]
    fn a_refusal_says_what_the_user_can_do() {
        let message = status_message(403);
        assert!(message.contains("403"), "the message was: {message}");
        assert!(
            message.contains("expired") || message.contains("browser"),
            "the message was: {message}"
        );
        assert!(status_message(404).contains("not on the server"));
    }
}
