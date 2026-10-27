//! Everything that speaks HTTP.
//!
//! The reference download manager opens raw sockets and terminates TLS
//! itself, but the wire format is what matters here: every request carries an
//! explicit `Range:` header and `Accept-Encoding: identity`, so the bytes on
//! the wire line up with the byte offsets in the file. See `ANALYSIS.md` for
//! the evidence.

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use reqwest::header::{
    ACCEPT_ENCODING, CONTENT_DISPOSITION, CONTENT_RANGE, ETAG, IF_RANGE, LAST_MODIFIED, RANGE,
    RETRY_AFTER,
};

use crate::transfer::{
    ByteStream, FailureKind, Probe, TransferError, CHANGED_MESSAGE, RANGE_IGNORED_MESSAGE,
};

/// The reference binary asks for an identity encoding so that the byte
/// offsets on the wire stay equal to the byte offsets in the file.
pub const IDENTITY_ENCODING: &str = "identity";

/// The User-Agent copied from the string table of the reference binary.
///
/// A server that only serves browsers answers `403 Forbidden` to anything
/// else: the video links of Google are the everyday example, and a download
/// manager that identifies itself honestly is refused there.
pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 6.1; WOW64) AppleWebKit/537.36 (KHTML, like Gecko)";

/// Ask the server for the first byte only.
///
/// The answer reveals the total size and proves whether the server honours
/// `Range` requests. This is the same probe the reference download manager
/// performs before it decides how many connections it may open.
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
        return Err(answer_error(status.as_u16(), retry_after));
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

    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };

    Ok(Probe {
        total,
        ranges,
        name: response
            .headers()
            .get(CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .and_then(file_name_from_disposition),
        etag: header(ETAG),
        last_modified: header(LAST_MODIFIED),
    })
}

/// The header value that proves a later request gets the same bytes.
///
/// `If-Range` needs a strong validator. A weak entity tag (`W/"..."`) may
/// name a semantically equal copy that differs byte for byte, so it is not
/// used; the modification date is the fallback.
pub fn validator(etag: Option<&str>, last_modified: Option<&str>) -> Option<String> {
    let etag = etag.map(str::trim).filter(|etag| !etag.is_empty());
    if let Some(etag) = etag {
        if !etag.starts_with("W/") {
            return Some(etag.to_string());
        }
    }
    last_modified
        .map(str::trim)
        .filter(|date| !date.is_empty())
        .map(str::to_string)
}

/// The error for a status the server refused the request with.
fn answer_error(status: u16, retry_after: Option<Duration>) -> TransferError {
    TransferError {
        message: status_message(status),
        status: Some(status),
        retry_after,
        kind: if is_retryable(status) {
            FailureKind::Transient
        } else {
            FailureKind::Permanent
        },
    }
}

/// Open a transfer that yields the bytes of a range.
///
/// `to` of `None` means "to the end of the file". `covers_all` says whether
/// this range is the whole file: a server that answers `200` instead of
/// `206` sent the whole file, and that is only usable when the whole file is
/// what this range holds.
///
/// `if_range` is the validator the probe was answered with. `expected_total`
/// is the size the probe promised. Both are checked against the answer, so a
/// file that changed between the probe and the request is refused instead of
/// being mixed into the bytes already on disk.
pub async fn open_range(
    client: &reqwest::Client,
    url: &str,
    from: u64,
    to: Option<u64>,
    covers_all: bool,
    if_range: Option<&str>,
    expected_total: Option<u64>,
) -> Result<ByteStream, TransferError> {
    // The reference binary always asks for an explicit range and always asks
    // for an identity encoding so the bytes on the wire line up with the file
    // offsets.
    let mut request = client
        .get(url)
        .header(RANGE, range_header(from, to))
        .header(ACCEPT_ENCODING, IDENTITY_ENCODING);
    if let Some(validator) = if_range {
        request = request.header(IF_RANGE, validator);
    }

    let response = request
        .send()
        .await
        .map_err(|e| TransferError::msg(e.to_string()))?;

    let status = response.status();
    let retry_after = parse_retry_after(response.headers());

    if !status.is_success() {
        // A `416` is not "the range is complete": the engine never asks for
        // a range it already has. It means the resource no longer covers the
        // range, which asking again will not change.
        return Err(answer_error(status.as_u16(), retry_after));
    }
    if status == reqwest::StatusCode::OK && !covers_all {
        // A whole-file answer for a range that is not the whole file would
        // leave the download short. When `If-Range` was sent, the answer also
        // proves the server's copy is not the copy the probe saw.
        return Err(if if_range.is_some() {
            TransferError::changed(CHANGED_MESSAGE)
        } else {
            TransferError::range_ignored(RANGE_IGNORED_MESSAGE)
        });
    }

    // Verify the server really served the range we asked for. The reference
    // binary checks the `Content-Range` reply in
    // `HTTPConnection::GetRetrRangeReply()`.
    let content_range = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok());
    if status == reqwest::StatusCode::PARTIAL_CONTENT && content_range.is_none() {
        // A `206` without `Content-Range` does not say which bytes it is
        // sending, so they cannot be placed in the file.
        return Err(TransferError::permanent(
            "the server sent a partial answer without a Content-Range",
        ));
    }
    if let Some(value) = content_range {
        let Some((served_start, served_end, served_total)) = parse_content_range(value) else {
            return Err(TransferError::permanent(format!(
                "the server sent a Content-Range we cannot read: {value}"
            )));
        };
        if served_start != from {
            return Err(TransferError::msg(format!(
                "the server served bytes from {served_start} instead of {from}"
            )));
        }
        if served_end < served_start {
            return Err(TransferError::permanent(format!(
                "the server sent a Content-Range that ends before it starts: {value}"
            )));
        }
        // The size in the header is the proof that both ends of the download
        // are talking about the same file.
        if let (Some(expected), Some(actual)) = (expected_total, served_total) {
            if actual != expected {
                return Err(TransferError::changed(format!(
                    "the file changed on the server: it was {expected} bytes, it is {actual} now"
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

/// Read a `Content-Range` value: `bytes <start>-<end>/<total>`.
///
/// The total is `None` when the server wrote `*`: it does not know the size.
pub fn parse_content_range(value: &str) -> Option<(u64, u64, Option<u64>)> {
    let (unit, spec) = value.trim().split_once(' ')?;
    if !unit.eq_ignore_ascii_case("bytes") {
        return None;
    }
    let (range, total) = spec.trim().split_once('/')?;
    let (start, end) = range.trim().split_once('-')?;
    let start = start.trim().parse().ok()?;
    let end = end.trim().parse().ok()?;
    let total = total.trim();
    let total = if total == "*" {
        None
    } else {
        Some(total.parse().ok()?)
    };
    Some((start, end, total))
}

/// Read the total size out of a `Content-Range` value.
///
/// `bytes 0-0/1234` gives `Some(1234)`. `bytes 0-0/*` gives `None`, which is
/// how a server says it does not know the size.
pub fn parse_content_range_total(value: &str) -> Option<u64> {
    parse_content_range(value).and_then(|(_, _, total)| total)
}

/// Read the first byte offset out of a `Content-Range` value.
///
/// The reference binary checks the reply of a range request against the range
/// it asked for (`HTTPConnection::GetRetrRangeReply()`), so the engine has to
/// be able to read back what the server actually served.
pub fn parse_content_range_start(value: &str) -> Option<u64> {
    parse_content_range(value).map(|(start, _, _)| start)
}

/// The file name a `Content-Disposition` header carries, if it carries one.
///
/// `filename*=UTF-8''na%C3%AFve.txt` (RFC 5987) wins over a plain
/// `filename="naive.txt"`, because it is the one a server sends when it has
/// already solved the encoding. Some servers send a whole path in the field,
/// so only the last segment is kept.
pub fn file_name_from_disposition(value: &str) -> Option<String> {
    let mut plain = None;

    for part in value.split(';') {
        let part = part.trim();
        if let Some(raw) = part.strip_prefix("filename*=") {
            let raw = raw.trim().trim_matches('"');
            let encoded = raw.rsplit_once("''").map(|(_, rest)| rest).unwrap_or(raw);
            if let Some(name) = clean_file_name(&percent_decode(encoded)) {
                return Some(name);
            }
        } else if let Some(raw) = part.strip_prefix("filename=") {
            plain = clean_file_name(raw.trim().trim_matches('"'));
        }
    }

    plain
}

/// The last segment of a name a server sent, without quotes or separators.
fn clean_file_name(value: &str) -> Option<String> {
    let last = value.rsplit(['/', '\\']).next()?.trim();
    if last.is_empty() || last == "." || last == ".." {
        return None;
    }
    Some(last.to_string())
}

/// Decode `%XX` escapes; anything else is kept as it came.
fn percent_decode(value: &str) -> String {
    fn digit(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (digit(bytes[index + 1]), digit(bytes[index + 2])) {
                out.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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
/// The reference binary waits a fixed 400 ms between attempts. An exponential
/// wait with a cap costs nothing to implement and does not hammer a struggling
/// server.
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
        assert_eq!(
            parse_content_range_start("bytes 4096-8191/1234"),
            Some(4096)
        );
        assert_eq!(parse_content_range_start("bytes */1234"), None);
    }

    #[test]
    fn a_range_to_the_end_has_no_end() {
        assert_eq!(range_header(10, Some(20)), "bytes=10-20");
        assert_eq!(range_header(10, None), "bytes=10-");
    }

    #[test]
    fn a_content_range_is_read_whole() {
        assert_eq!(
            parse_content_range("bytes 4096-8191/9000"),
            Some((4096, 8191, Some(9000)))
        );
        assert_eq!(parse_content_range("bytes 0-0/*"), Some((0, 0, None)));
        assert_eq!(parse_content_range("bytes 0-0"), None);
        assert_eq!(parse_content_range("items 0-0/10"), None);
    }

    #[test]
    fn a_strong_entity_tag_is_the_validator() {
        assert_eq!(
            validator(Some("\"abc\""), Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            Some("\"abc\"".to_string())
        );
    }

    #[test]
    fn a_weak_entity_tag_is_not_used_for_a_range() {
        assert_eq!(
            validator(Some("W/\"abc\""), Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            Some("Wed, 21 Oct 2015 07:28:00 GMT".to_string())
        );
        assert_eq!(validator(Some("W/\"abc\""), None), None);
    }

    #[test]
    fn a_modification_date_is_the_fallback() {
        assert_eq!(
            validator(None, Some(" Wed, 21 Oct 2015 07:28:00 GMT ")),
            Some("Wed, 21 Oct 2015 07:28:00 GMT".to_string())
        );
        assert_eq!(validator(None, Some("   ")), None);
        assert_eq!(validator(None, None), None);
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
    fn a_plain_name_is_read_out_of_the_disposition() {
        assert_eq!(
            file_name_from_disposition("attachment; filename=\"report.pdf\""),
            Some("report.pdf".to_string())
        );
        assert_eq!(
            file_name_from_disposition("attachment; filename=report.pdf"),
            Some("report.pdf".to_string())
        );
    }

    #[test]
    fn an_encoded_name_wins_over_a_plain_one() {
        assert_eq!(
            file_name_from_disposition(
                "attachment; filename=\"naive.txt\"; filename*=UTF-8''na%C3%AFve.txt"
            ),
            Some("naïve.txt".to_string())
        );
    }

    #[test]
    fn only_the_last_segment_of_a_disposition_name_is_kept() {
        assert_eq!(
            file_name_from_disposition("attachment; filename=\"C:\\\\temp\\\\clip.mp4\""),
            Some("clip.mp4".to_string())
        );
        assert_eq!(file_name_from_disposition("inline"), None);
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
