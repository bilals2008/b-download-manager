//! Reading the headers a server answers with: content ranges and file names.

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
