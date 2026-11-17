//! Reading one page: the tags and the text between them, the picture an
//! address points at, and the addresses resolved against the page's own.
//!
//! This is the bottom of the gallery reader: one bounded pass over the HTML,
//! no fetches, and nothing that knows how a gallery is named.

use std::collections::HashSet;

use reqwest::Url;

use super::{has_media_extension, MAX_TAGS};

/// What one page holds.
pub(super) struct Page {
    pub(super) title: String,
    pub(super) heading: String,
    pub(super) candidates: Vec<String>,
}

/// One event of a page.
enum Event {
    Start {
        name: String,
        attrs: Vec<(String, String)>,
    },
    End {
        name: String,
    },
    Text(String),
}

/// Read one page and collect the addresses that point at a picture.
pub(super) fn read_page(html: &str, base: Option<&Url>) -> Page {
    let mut page = Page {
        title: String::new(),
        heading: String::new(),
        candidates: Vec::new(),
    };
    let mut seen: HashSet<String> = HashSet::new();
    // Whether the link that is open right now points at a picture itself.
    let mut links: Vec<bool> = Vec::new();
    let mut capture: Option<&'static str> = None;

    for event in scan(html) {
        match event {
            Event::Start { name, attrs } => match name.as_str() {
                "title" => capture = Some("title"),
                "h1" => capture = Some("heading"),
                "a" => {
                    let address = attr(&attrs, "href");
                    let resolved = address.as_deref().and_then(|href| resolve(base, href));
                    let is_picture = resolved
                        .as_deref()
                        .map(has_media_extension)
                        .unwrap_or(false);
                    if is_picture {
                        if let Some(url) = resolved {
                            if seen.insert(url.clone()) {
                                page.candidates.push(url);
                            }
                        }
                    }
                    links.push(is_picture);
                }
                "img" | "source" => {
                    // A link that points at the full-size picture holds a
                    // thumbnail of it; the link is the one worth keeping.
                    if links.last().copied().unwrap_or(false) {
                        continue;
                    }
                    if let Some(value) = image_source(&attrs) {
                        if let Some(url) = resolve(base, &value) {
                            if seen.insert(url.clone()) {
                                page.candidates.push(url);
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::End { name } => match name.as_str() {
                "title" | "h1" => capture = None,
                "a" => {
                    links.pop();
                }
                _ => {}
            },
            Event::Text(text) => {
                if let Some(kind) = capture {
                    let text = decode_entities(text.trim());
                    if text.is_empty() {
                        continue;
                    }
                    match kind {
                        "title" => {
                            if page.title.is_empty() {
                                page.title = text;
                            }
                        }
                        _ => {
                            if page.heading.is_empty() {
                                page.heading = text;
                            }
                        }
                    }
                }
            }
        }
    }

    page
}

/// The address of the picture one tag carries.
///
/// The lazy attributes come first: a page that keeps the real picture in
/// `data-src` puts a placeholder in `src`. A `srcset` holds several sizes and
/// the last one is the largest.
fn image_source(attrs: &[(String, String)]) -> Option<String> {
    for key in [
        "data-original",
        "data-src",
        "data-lazy-src",
        "data-srcset",
        "srcset",
        "src",
    ] {
        let Some(value) = attr(attrs, key) else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.to_ascii_lowercase().starts_with("data:") {
            continue;
        }
        if key.ends_with("srcset") {
            if let Some(url) = largest_candidate(value) {
                return Some(url);
            }
            continue;
        }
        return Some(value.to_string());
    }
    None
}

/// The largest candidate of a `srcset`.
fn largest_candidate(value: &str) -> Option<String> {
    let last = value.split(',').rfind(|part| !part.trim().is_empty())?;
    let url = last.split_whitespace().next()?;
    if url.is_empty() {
        return None;
    }
    Some(url.to_string())
}

/// Resolve an address found in a page against the page's own address.
fn resolve(base: Option<&Url>, value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('#') {
        return None;
    }
    let lowered = value.to_ascii_lowercase();
    if lowered.starts_with("data:")
        || lowered.starts_with("javascript:")
        || lowered.starts_with("mailto:")
        || lowered.starts_with("blob:")
    {
        return None;
    }
    let joined = match base {
        Some(base) => base.join(value).ok()?,
        None => Url::parse(value).ok()?,
    };
    if joined.scheme() != "http" && joined.scheme() != "https" {
        return None;
    }
    Some(joined.to_string())
}

/// One attribute of a tag.
fn attr(attrs: &[(String, String)], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

/// Split a page into tags and the text between them.
fn scan(html: &str) -> Vec<Event> {
    let bytes = html.as_bytes();
    let mut events: Vec<Event> = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() && events.len() < MAX_TAGS {
        if bytes[index] != b'<' {
            let start = index;
            while index < bytes.len() && bytes[index] != b'<' {
                index += 1;
            }
            events.push(Event::Text(html[start..index].to_string()));
            continue;
        }
        if html[index..].starts_with("<!--") {
            index = match html[index + 4..].find("-->") {
                Some(offset) => index + 4 + offset + 3,
                None => bytes.len(),
            };
            continue;
        }
        if html[index..].starts_with("<!") || html[index..].starts_with("<?") {
            index = match html[index..].find('>') {
                Some(offset) => index + offset + 1,
                None => bytes.len(),
            };
            continue;
        }

        let closing = html[index..].starts_with("</");
        let end = match find_tag_end(html, index + 1) {
            Some(end) => end,
            None => {
                events.push(Event::Text(html[index..].to_string()));
                break;
            }
        };
        let inner = &html[index + usize::from(closing) + 1..end];
        let (name, attrs) = parse_tag(inner);
        events.push(if closing {
            Event::End { name }
        } else {
            Event::Start { name, attrs }
        });
        index = end + 1;
    }

    events
}

/// The end of a tag, ignoring the `>` inside a quoted value.
fn find_tag_end(html: &str, from: usize) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut quote = 0u8;
    let mut index = from;
    while index < bytes.len() {
        let byte = bytes[index];
        if quote != 0 {
            if byte == quote {
                quote = 0;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = byte;
        } else if byte == b'>' {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// The name and the attributes of a tag.
fn parse_tag(inner: &str) -> (String, Vec<(String, String)>) {
    let bytes = inner.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'/' {
        index += 1;
    }
    let name = inner[..index].trim().to_ascii_lowercase();

    let mut attrs: Vec<(String, String)> = Vec::new();
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let name_start = index;
        while index < bytes.len()
            && bytes[index] != b'='
            && bytes[index] != b'/'
            && !bytes[index].is_ascii_whitespace()
        {
            index += 1;
        }
        let key = inner[name_start..index].trim().to_ascii_lowercase();
        if key.is_empty() {
            index += 1;
            continue;
        }
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let value = if bytes[index] == b'"' || bytes[index] == b'\'' {
            let quote = bytes[index];
            index += 1;
            let start = index;
            while index < bytes.len() && bytes[index] != quote {
                index += 1;
            }
            let value = &inner[start..index.min(inner.len())];
            index += 1;
            value.to_string()
        } else {
            let start = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'>'
            {
                index += 1;
            }
            inner[start..index].to_string()
        };
        attrs.push((key, decode_entities(&value)));
    }

    (name, attrs)
}

/// The five named entities and the numeric ones.
fn decode_entities(value: &str) -> String {
    if !value.contains('&') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        rest = &rest[index..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(character) => out.push(character),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}
