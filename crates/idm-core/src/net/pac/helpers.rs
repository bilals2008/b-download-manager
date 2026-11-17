//! The helper functions a PAC script may call, as browsers provide them.
//!
//! Two of them cannot be answered here: `myIpAddress` and `dnsResolve` need
//! the machine's own address and a resolver, and this module has neither.
//! They answer "unknown" rather than a guess — `dnsResolve` answers `null`,
//! which is what a browser answers for a name it cannot resolve, and
//! `myIpAddress` answers `null` too, so a script comparing it with an
//! address is simply false rather than accidentally true.

use std::net::Ipv4Addr;
use std::time::{SystemTime, UNIX_EPOCH};

use super::js::{as_text, to_int, Value};
use super::PacError;

/// The clock the date helpers read.
///
/// The engine has no time-zone database, so the local time is read as UTC:
/// the `GMT` forms of `weekdayRange` and `timeRange` are the ones with a
/// defined answer. A caller that knows the offset passes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    /// The seconds since the Unix epoch.
    pub unix_seconds: i64,
    /// The offset of the local time from UTC, in seconds.
    pub local_offset_seconds: i32,
}

impl Clock {
    /// The clock now, read as UTC.
    pub fn now() -> Self {
        let unix_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or(0);
        Clock {
            unix_seconds,
            local_offset_seconds: 0,
        }
    }

    /// The clock at that moment, with that offset from UTC.
    pub fn at(unix_seconds: i64, local_offset_seconds: i32) -> Self {
        Clock {
            unix_seconds,
            local_offset_seconds,
        }
    }

    /// The parts of the day the date helpers read.
    fn parts(&self, gmt: bool) -> Parts {
        let seconds = self.unix_seconds
            + if gmt {
                0
            } else {
                i64::from(self.local_offset_seconds)
            };
        let days = seconds.div_euclid(86_400);
        let rest = seconds.rem_euclid(86_400);
        Parts {
            hour: (rest / 3_600) as u32,
            minute: (rest % 3_600 / 60) as u32,
            second: (rest % 60) as u32,
            // The first of January 1970 was a Thursday, the fourth day of
            // the week when Sunday is the first.
            weekday: (days + 4).rem_euclid(7) as u32,
        }
    }
}

/// The parts of the day the date helpers read.
struct Parts {
    hour: u32,
    minute: u32,
    second: u32,
    /// Sunday is 0, as in JavaScript.
    weekday: u32,
}

/// The days of the week, as the scripts name them.
const DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

/// The call of a helper function, or `None` when the name is not one.
pub(super) fn call(name: &str, args: &[Value], clock: &Clock) -> Option<Result<Value, PacError>> {
    let answered = match name {
        "isPlainHostName" => plain_host_name(args),
        "dnsDomainIs" => domain_is(args),
        "localHostOrDomainIs" => local_host_or_domain_is(args),
        "dnsResolve" => dns_resolve(args),
        "isResolvable" => is_resolvable(args),
        "myIpAddress" => Ok(Value::Null),
        "dnsDomainLevels" => domain_levels(args),
        "convert_addr" => convert_addr(args),
        "isInNet" => is_in_net(args),
        "shExpMatch" => sh_exp_match(args),
        "weekdayRange" => weekday_range(args, clock),
        "timeRange" => time_range(args, clock),
        "alert" => Ok(Value::Undefined),
        _ => return None,
    };
    Some(answered)
}

/// The first argument as text.
fn text(args: &[Value], index: usize) -> Result<String, PacError> {
    args.get(index)
        .map(as_text)
        .ok_or_else(|| PacError::runtime("the script called a helper with too few arguments"))
}

/// `isPlainHostName(host)`: the name has no dot in it.
fn plain_host_name(args: &[Value]) -> Result<Value, PacError> {
    let host = text(args, 0)?;
    Ok(Value::Bool(!host.contains('.')))
}

/// `dnsDomainIs(host, domain)`: the host ends with the domain.
fn domain_is(args: &[Value]) -> Result<Value, PacError> {
    let host = text(args, 0)?.to_ascii_lowercase();
    let domain = text(args, 1)?.to_ascii_lowercase();
    Ok(Value::Bool(
        host.len() >= domain.len() && host.ends_with(&domain),
    ))
}

/// `localHostOrDomainIs(host, hostdom)`: the host is the domain, or it is a
/// name without a dot and the domain begins with it.
fn local_host_or_domain_is(args: &[Value]) -> Result<Value, PacError> {
    let host = text(args, 0)?.to_ascii_lowercase();
    let domain = text(args, 1)?.to_ascii_lowercase();
    let same = host == domain;
    let shorter = !host.contains('.') && domain.starts_with(&format!("{host}."));
    Ok(Value::Bool(same || shorter))
}

/// `dnsResolve(host)`: the address of a host.
///
/// A host that is already an address is itself. A name cannot be resolved
/// here, so the answer is `null` — what a browser answers for a name it
/// cannot resolve either, and never a guess.
fn dns_resolve(args: &[Value]) -> Result<Value, PacError> {
    let host = text(args, 0)?;
    if is_address(&host) {
        Ok(Value::Str(host))
    } else {
        Ok(Value::Null)
    }
}

/// `isResolvable(host)`: whether the host has an address.
fn is_resolvable(args: &[Value]) -> Result<Value, PacError> {
    Ok(match dns_resolve(args)? {
        Value::Null => Value::Bool(false),
        _ => Value::Bool(true),
    })
}

/// `dnsDomainLevels(host)`: how many dots the host has.
fn domain_levels(args: &[Value]) -> Result<Value, PacError> {
    let host = text(args, 0)?;
    Ok(Value::Num(host.matches('.').count() as f64))
}

/// `convert_addr(ip)`: the address as a number, or `null` when it is not one.
fn convert_addr(args: &[Value]) -> Result<Value, PacError> {
    Ok(match address_of(&text(args, 0)?) {
        Some(number) => Value::Num(f64::from(number)),
        None => Value::Null,
    })
}

/// `isInNet(host, pattern, mask)`: the host's address is inside the network
/// the pattern and the mask describe.
fn is_in_net(args: &[Value]) -> Result<Value, PacError> {
    let host = text(args, 0)?;
    let pattern = address_of(&text(args, 1)?);
    let mask = address_of(&text(args, 2)?);
    let (Some(pattern), Some(mask)) = (pattern, mask) else {
        return Ok(Value::Bool(false));
    };
    // A host that is already an address is used as it is; a name would have
    // to be resolved, and there is no resolver here.
    let Some(host) = address_of(&host) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool((host & mask) == (pattern & mask)))
}

/// The address as a number, when it is a dotted quad.
fn address_of(text: &str) -> Option<u32> {
    let mut number: u32 = 0;
    let mut parts = 0;
    for part in text.split('.') {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let octet: u32 = part.parse().ok()?;
        if octet > 255 {
            return None;
        }
        number = number * 256 + octet;
        parts += 1;
    }
    if parts == 4 {
        Some(number)
    } else {
        None
    }
}

/// `shExpMatch(str, exp)`: the string matches the pattern, where `*` stands
/// for any run of characters and `?` for one.
fn sh_exp_match(args: &[Value]) -> Result<Value, PacError> {
    let subject = text(args, 0)?;
    let pattern = text(args, 1)?;
    Ok(Value::Bool(glob_matches(&subject, &pattern)))
}

/// Whether `subject` matches the `*` and `?` pattern.
fn glob_matches(subject: &str, pattern: &str) -> bool {
    let subject: Vec<char> = subject.chars().collect();
    let pattern: Vec<char> = pattern.chars().collect();
    let (mut s, mut p) = (0usize, 0usize);
    // Where the last `*` was, and how far it has reached into the subject:
    // the place to go back to when the rest does not fit.
    let mut star: Option<(usize, usize)> = None;
    while s < subject.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == subject[s]) {
            s += 1;
            p += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, s));
            p += 1;
        } else if let Some((at, reached)) = star {
            // The `*` takes one more character and the rest is tried again.
            let reached = reached + 1;
            star = Some((at, reached));
            p = at + 1;
            s = reached;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

/// Whether the last argument says "GMT".
fn gmt(args: &[Value]) -> bool {
    args.last()
        .map(|last| as_text(last).eq_ignore_ascii_case("GMT"))
        .unwrap_or(false)
}

/// The arguments without the "GMT" at the end.
fn without_gmt(args: &[Value]) -> &[Value] {
    if gmt(args) {
        &args[..args.len() - 1]
    } else {
        args
    }
}

/// Whether `value` is inside the range, which may run over midnight.
fn within(value: i64, low: i64, high: i64) -> bool {
    if low <= high {
        value >= low && value <= high
    } else {
        value >= low || value <= high
    }
}

/// `weekdayRange(...)`: today is one of the days named.
fn weekday_range(args: &[Value], clock: &Clock) -> Result<Value, PacError> {
    let is_gmt = gmt(args);
    let args = without_gmt(args);
    if args.is_empty() {
        return Err(PacError::runtime("weekdayRange needs a day"));
    }
    let first = day_number(&as_text(&args[0]))?;
    let second = if args.len() > 1 {
        day_number(&as_text(&args[1]))?
    } else {
        first
    };
    let today = clock.parts(is_gmt).weekday as i64;
    Ok(Value::Bool(within(today, first, second)))
}

/// The number of a day of the week, as the scripts name it.
fn day_number(name: &str) -> Result<i64, PacError> {
    let name = name.trim().to_ascii_uppercase();
    DAYS.iter()
        .position(|day| *day == name)
        .map(|at| at as i64)
        .ok_or_else(|| PacError::runtime(format!("weekdayRange does not know the day '{name}'")))
}

/// `timeRange(...)`: now is inside the hours named.
fn time_range(args: &[Value], clock: &Clock) -> Result<Value, PacError> {
    let is_gmt = gmt(args);
    let args = without_gmt(args);
    let numbers: Vec<i64> = args
        .iter()
        .map(|arg| to_int(arg).ok_or_else(|| PacError::runtime("timeRange needs numbers")))
        .collect::<Result<Vec<i64>, PacError>>()?;
    let parts = clock.parts(is_gmt);
    let hour = i64::from(parts.hour);
    let minute = i64::from(parts.minute);
    let second = i64::from(parts.second);
    let now = hour * 3_600 + minute * 60 + second;
    Ok(Value::Bool(match numbers.len() {
        1 => hour == numbers[0],
        2 => within(hour, numbers[0], numbers[1]),
        4 => within(
            now,
            numbers[0] * 3_600 + numbers[1] * 60,
            numbers[2] * 3_600 + numbers[3] * 60 + 59,
        ),
        6 => within(
            now,
            numbers[0] * 3_600 + numbers[1] * 60 + numbers[2],
            numbers[3] * 3_600 + numbers[4] * 60 + numbers[5],
        ),
        _ => {
            return Err(PacError::runtime(
                "timeRange takes one, two, four or six arguments",
            ))
        }
    }))
}

/// The address of a host as the browser's `dnsResolve` gives it: an address
/// is itself, a name is unknown here.
pub(super) fn is_address(host: &str) -> bool {
    host.parse::<Ipv4Addr>().is_ok() || host.parse::<std::net::Ipv6Addr>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(name: &str, args: &[&str]) -> Value {
        let values: Vec<Value> = args.iter().map(|a| Value::Str((*a).to_owned())).collect();
        match call(name, &values, &Clock::at(0, 0)) {
            Some(Ok(value)) => value,
            other => panic!("{name}: {other:?}"),
        }
    }

    #[test]
    fn the_domain_helpers_answer() {
        assert_eq!(
            ask("dnsDomainIs", &["wiki.Example.org", ".example.org"]),
            Value::Bool(true)
        );
        assert_eq!(
            ask("dnsDomainIs", &["notexample.org", ".example.org"]),
            Value::Bool(false)
        );
        assert_eq!(
            ask("dnsDomainIs", &["example.org", "example.org"]),
            Value::Bool(true)
        );
        assert_eq!(
            ask("localHostOrDomainIs", &["www", "www.example.org"]),
            Value::Bool(true)
        );
        assert_eq!(
            ask("localHostOrDomainIs", &["www.other.org", "www.example.org"]),
            Value::Bool(false)
        );
        assert_eq!(ask("dnsDomainLevels", &["a.b.c"]), Value::Num(2.0));
        assert_eq!(ask("isPlainHostName", &["nas"]), Value::Bool(true));
        assert_eq!(ask("isPlainHostName", &["nas.local"]), Value::Bool(false));
    }

    #[test]
    fn the_net_helper_compares_addresses() {
        assert_eq!(
            ask("isInNet", &["192.168.1.20", "192.168.0.0", "255.255.0.0"]),
            Value::Bool(true)
        );
        assert_eq!(
            ask("isInNet", &["10.0.0.1", "192.168.0.0", "255.255.0.0"]),
            Value::Bool(false)
        );
        // A name cannot be resolved here, so the answer is "not inside".
        assert_eq!(
            ask(
                "isInNet",
                &["host.example.org", "192.168.0.0", "255.255.0.0"]
            ),
            Value::Bool(false)
        );
        assert_eq!(
            ask("convert_addr", &["104.16.41.2"]),
            Value::Num(1745889538.0)
        );
        assert_eq!(ask("convert_addr", &["nonsense"]), Value::Null);
    }

    #[test]
    fn the_pattern_helper_matches_wildcards() {
        assert_eq!(ask("shExpMatch", &["a.b.c", "a.?.c"]), Value::Bool(true));
        assert_eq!(ask("shExpMatch", &["abc", "a?"]), Value::Bool(false));
        assert_eq!(ask("shExpMatch", &["a.b.c", "a.*.c"]), Value::Bool(true));
        assert_eq!(ask("shExpMatch", &["a.b.c.d", "a.*.c"]), Value::Bool(false));
        assert_eq!(ask("shExpMatch", &["abc", "a*c"]), Value::Bool(true));
    }

    #[test]
    fn the_unknown_answers_are_unknown() {
        assert_eq!(ask("dnsResolve", &["example.org"]), Value::Null);
        assert_eq!(ask("myIpAddress", &[]), Value::Null);
        assert_eq!(ask("isResolvable", &["example.org"]), Value::Bool(false));
        assert!(is_address("192.168.0.1") && is_address("2620:fe::fe"));
        assert!(!is_address("example.org"));
    }

    #[test]
    fn the_date_helpers_read_the_clock() {
        // The third of January 2024, a Wednesday, at noon.
        let clock = Clock::at(1_704_283_200, 0);
        let values = vec![Value::Str("MON".into()), Value::Str("FRI".into())];
        assert_eq!(
            call("weekdayRange", &values, &clock),
            Some(Ok(Value::Bool(true)))
        );
        let values = vec![Value::Str("SUN".into()), Value::Str("SAT".into())];
        assert_eq!(
            call("weekdayRange", &values, &clock),
            Some(Ok(Value::Bool(true)))
        );
        let values = vec![Value::Str("SAT".into())];
        assert_eq!(
            call("weekdayRange", &values, &clock),
            Some(Ok(Value::Bool(false)))
        );
        let values = vec![Value::Num(0.0), Value::Num(23.0)];
        assert_eq!(
            call("timeRange", &values, &clock),
            Some(Ok(Value::Bool(true)))
        );
        let values = vec![Value::Num(13.0), Value::Num(23.0)];
        assert_eq!(
            call("timeRange", &values, &clock),
            Some(Ok(Value::Bool(false)))
        );
        // A range over midnight.
        let values = vec![Value::Num(22.0), Value::Num(2.0)];
        assert_eq!(
            call("timeRange", &values, &clock),
            Some(Ok(Value::Bool(false)))
        );
        // "GMT" says the clock is read as it is.
        let values = vec![Value::Num(0.0), Value::Num(23.0), Value::Str("GMT".into())];
        assert_eq!(
            call("timeRange", &values, &clock),
            Some(Ok(Value::Bool(true)))
        );
    }
}
