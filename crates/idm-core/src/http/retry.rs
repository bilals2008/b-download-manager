//! Retry timing, HTTP dates and the meaning of a status.

use super::*;

/// How long the server asked us to wait before trying again.
///
/// Both forms of the header are honoured: the delta-seconds form
/// (`120`), and the HTTP-date form (`Sun, 06 Nov 1994 08:49:37 GMT`). A date
/// in the past — or one this build cannot read — is not a wait, so the caller
/// falls back to its own backoff instead of sleeping for a negative time.
/// The result is capped at [`MAX_RETRY_AFTER`] so a wrong clock on either side
/// cannot park a download for days.
pub fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds.min(MAX_RETRY_AFTER.as_secs())));
    }
    let at = parse_http_date(value)?;
    let wait = at.duration_since(SystemTime::now()).unwrap_or_default();
    Some(wait.min(MAX_RETRY_AFTER))
}

/// The longest wait a `Retry-After` may impose. A day is already far more
/// than any server means by it in the middle of a download.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// Read one of the three date forms RFC 9110 allows in `Retry-After`.
///
/// The preferred form is IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`); the
/// obsolete RFC 850 (`Sunday, 06-Nov-94 08:49:37 GMT`) and asctime
/// (`Sun Nov  6 08:49:37 1994`) forms are still sent by old servers, so they
/// are read too. A value is refused rather than guessed at, because a wrong
/// wait is worse than the caller's own backoff.
pub fn parse_http_date(value: &str) -> Option<SystemTime> {
    let value = value.trim();
    let rest = value
        .split_once(", ")
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| {
            // asctime has no comma after the weekday; drop the two names.
            value.split_once(' ').map(|(_, rest)| rest).unwrap_or(value)
        });
    let parts: Vec<&str> = rest.split_whitespace().collect();
    // `06 Nov 1994 08:49:37 GMT` splits into six pieces, `06-Nov-94` stays
    // whole, `Nov 6 08:49:37 1994` splits into five.
    let (day, month, year, clock) = match parts.as_slice() {
        [day, month, year, clock, zone] if zone.eq_ignore_ascii_case("GMT") => {
            (day.to_string(), month.to_string(), *year, *clock)
        }
        [day_month_year, clock, zone] if zone.eq_ignore_ascii_case("GMT") => {
            return parse_rfc850(day_month_year, clock);
        }
        // `Nov 6 08:49:37 1994` — the asctime form, which names no zone and
        // is therefore always GMT.
        [month, day, clock, year] => ((*day).to_string(), (*month).to_string(), *year, *clock),
        _ => return None,
    };
    let day: u32 = day.parse().ok()?;
    let month = month_number(&month)?;
    let year: i64 = year.parse().ok()?;
    let (hour, minute, second) = parse_clock(clock)?;
    to_system_time(day, month, year, hour, minute, second)
}

/// `06-Nov-94 08:49:37` — the RFC 850 form.
fn parse_rfc850(value: &str, clock: &str) -> Option<SystemTime> {
    let (day, rest) = value.split_once('-')?;
    let (month, year) = rest.split_once('-')?;
    let day: u32 = day.parse().ok()?;
    let month = month_number(month)?;
    let year: i64 = year.parse().ok()?;
    // A two-digit year is this century when it is small; the form predates
    // 2000 by design, so 94 means 1994 and 24 means 2024.
    let year = if year < 70 {
        2000 + year
    } else if year < 100 {
        1900 + year
    } else {
        year
    };
    let (hour, minute, second) = parse_clock(clock)?;
    to_system_time(day, month, year, hour, minute, second)
}

fn parse_clock(value: &str) -> Option<(i64, i64, i64)> {
    let mut pieces = value.split(':');
    let hour = pieces.next()?.parse().ok()?;
    let minute = pieces.next()?.parse().ok()?;
    let second = pieces.next()?.parse().ok()?;
    if pieces.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some((hour, minute, second))
}

fn month_number(name: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let lower = name.to_ascii_lowercase();
    let lower = lower.trim_end_matches('.');
    // The asctime form abbreviates to three letters; a full name is accepted
    // too, because some proxies send one.
    let three = if lower.len() >= 3 { &lower[..3] } else { lower };
    MONTHS
        .iter()
        .position(|month| *month == three)
        .map(|index| index as u32 + 1)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`), so no calendar crate is needed.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i64;
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn to_system_time(
    day: u32,
    month: u32,
    year: i64,
    hour: i64,
    minute: i64,
    second: i64,
) -> Option<SystemTime> {
    if !(1..=31).contains(&day) || !(1970..=9999).contains(&year) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if days < 0 {
        return None;
    }
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    Some(UNIX_EPOCH + Duration::from_secs(seconds as u64))
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
