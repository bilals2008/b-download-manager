//! Reading the links out of a page.
//!
//! A grabber only needs three things from the markup: the addresses a reader
//! could click, the media the page loads, and which of those it should not
//! follow. That is small enough to read with a tolerant scanner rather than a
//! full HTML tree, and the scanner is deliberately forgiving: a page that is
//! cut off, nested oddly, or full of script must never stop a crawl or invent
//! a link out of code.
//!
//! Comments, scripts, styles and declarations are skipped whole. An address is
//! resolved against the page it came from, and only `http` and `https` links
//! without credentials survive: `javascript:`, `data:`, `mailto:` and `file:`
//! are not something a downloader reads.

use std::collections::HashMap;

use reqwest::Url;

use super::policy::MAX_PAGE_LINKS;

/// One link of a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub url: Url,
    /// Whether the page said this link leads to another page that may be
    /// followed. A media source is never followed; an anchor with
    /// `rel="nofollow"` is not either, though it may still name a file.
    pub crawl: bool,
}

/// The links of a page, in the order they appear, each address once.
///
/// The same address can be an anchor and an image; the anchor's permission to
/// be followed wins, because it is the stronger statement.
pub fn links(html: &str, base: &Url) -> Vec<Link> {
    let mut out: Vec<Link> = Vec::new();
    let tags = tags(html);

    // A `<base href>` moves where every relative link points. It is read once,
    // before the links, because the tag may follow the anchors it governs.
    let mut document_base = base.clone();
    if let Some(base_tag) = tags.iter().find(|tag| tag.name == "base") {
        if let Some(href) = base_tag.attribute("href") {
            if let Ok(joined) = base.join(href.trim()) {
                if joined.scheme() == "http" || joined.scheme() == "https" {
                    let mut joined = joined;
                    joined.set_fragment(None);
                    document_base = joined;
                }
            }
        }
    }
    let mut seen: HashMap<String, usize> = HashMap::new();

    for tag in &tags {
        let mut found: Vec<(String, bool)> = Vec::new();
        match tag.name.as_str() {
            "a" | "area" => {
                if let Some(href) = tag.attribute("href") {
                    found.push((href, !tag.is_nofollow()));
                }
            }
            "img" | "source" | "video" | "audio" | "track" | "embed" => {
                if let Some(src) = tag.attribute("src") {
                    found.push((src, false));
                }
                if let Some(srcset) = tag.attribute("srcset") {
                    // A `srcset` may carry an inline image, whose commas split
                    // the base64 into pieces that would otherwise look like
                    // relative links. An inline source is no file to grab.
                    if !srcset
                        .trim_start()
                        .to_ascii_lowercase()
                        .starts_with("data:")
                    {
                        for candidate in srcset.split(',') {
                            if let Some(address) = candidate.split_whitespace().next() {
                                found.push((address.to_string(), false));
                            }
                        }
                    }
                }
            }
            _ => continue,
        }

        for (raw, crawl) in found {
            let Some(url) = resolve(&document_base, &raw) else {
                continue;
            };
            let key = url.as_str().to_string();
            if let Some(index) = seen.get(&key) {
                if let Some(known) = out.get_mut(*index) {
                    known.crawl = known.crawl || crawl;
                }
                continue;
            }
            if out.len() >= MAX_PAGE_LINKS {
                return out;
            }
            seen.insert(key, out.len());
            out.push(Link { url, crawl });
        }
    }

    out
}

/// Resolve one attribute value against the page and keep it when it is a web
/// address. The fragment is dropped: it names a place in a page, not a page.
fn resolve(base: &Url, raw: &str) -> Option<Url> {
    let raw = decode_entities(raw);
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with('#') {
        return None;
    }
    let lowered = raw.to_ascii_lowercase();
    for scheme in [
        "javascript:",
        "mailto:",
        "tel:",
        "data:",
        "blob:",
        "about:",
        "file:",
        "ftp:",
    ] {
        if lowered.starts_with(scheme) {
            return None;
        }
    }
    let mut joined = base.join(raw).ok()?;
    if joined.scheme() != "http" && joined.scheme() != "https" {
        return None;
    }
    if !joined.username().is_empty() || joined.password().is_some() {
        return None;
    }
    joined.set_fragment(None);
    Some(joined)
}

/* --------------------------------------------------------------------- tags */

/// One tag, with its attributes in lower case.
#[derive(Debug, Default)]
struct Tag {
    name: String,
    attributes: Vec<(String, String)>,
}

impl Tag {
    fn attribute(&self, name: &str) -> Option<String> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    fn is_nofollow(&self) -> bool {
        self.attribute("rel").is_some_and(|rel| {
            rel.split_whitespace()
                .any(|word| word.eq_ignore_ascii_case("nofollow"))
        })
    }
}

/// Every start tag of a document, without the contents of comments, scripts
/// and styles. An end tag is not needed and is dropped.
fn tags(html: &str) -> Vec<Tag> {
    let bytes = html.as_bytes();
    let mut out: Vec<Tag> = Vec::new();
    let mut at = 0usize;

    while at < bytes.len() {
        let Some(offset) = bytes[at..].iter().position(|byte| *byte == b'<') else {
            break;
        };
        let start = at + offset;
        at = start + 1;
        let rest = &html[start..];

        if rest.starts_with("<!--") {
            at = find(rest, "-->").map_or(bytes.len(), |end| start + end + 3);
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            at = find(rest, "]]>").map_or(bytes.len(), |end| start + end + 3);
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            at = find(rest, ">").map_or(bytes.len(), |end| start + end + 1);
            continue;
        }
        if rest.starts_with("</") {
            at = find(rest, ">").map_or(bytes.len(), |end| start + end + 1);
            continue;
        }

        let mut cursor = start + 1;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric()
                || matches!(bytes[cursor], b':' | b'-' | b'_'))
        {
            cursor += 1;
        }
        if cursor == start + 1 {
            // A stray `<` in text, not a tag.
            continue;
        }
        let name = html[start + 1..cursor].to_ascii_lowercase();

        let (tag, next) = attributes(html, cursor);
        at = next;
        if matches!(name.as_str(), "script" | "style") {
            let closing = format!("</{name}");
            at = find_ignore_case(&html[at..], &closing)
                .map(|end| at + end + closing.len())
                .unwrap_or(bytes.len());
            // Step past the end tag's `>` so its name is not read as a tag.
            at = html[at..].find('>').map_or(bytes.len(), |end| at + end + 1);
            continue;
        }
        out.push(Tag {
            name,
            attributes: tag,
        });
        if out.len() >= MAX_PAGE_LINKS * 4 {
            // A page with this many tags is a machine's page; reading more
            // only spends memory on links that are already past the limit.
            break;
        }
    }

    out
}

/// Read the attributes of one tag, starting after its name. Returns them and
/// the position just past the closing `>`.
fn attributes(html: &str, from: usize) -> (Vec<(String, String)>, usize) {
    let bytes = html.as_bytes();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut at = from;

    while at < bytes.len() {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at >= bytes.len() {
            break;
        }
        match bytes[at] {
            b'>' => return (out, at + 1),
            b'/' => {
                at += 1;
                continue;
            }
            _ => {}
        }

        let name_start = at;
        while at < bytes.len()
            && !bytes[at].is_ascii_whitespace()
            && !matches!(bytes[at], b'=' | b'>' | b'/')
        {
            at += 1;
        }
        let name = html[name_start..at].to_ascii_lowercase();
        if name.is_empty() {
            at += 1;
            continue;
        }

        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        let mut value = String::new();
        if at < bytes.len() && bytes[at] == b'=' {
            at += 1;
            while at < bytes.len() && bytes[at].is_ascii_whitespace() {
                at += 1;
            }
            if at < bytes.len() && (bytes[at] == b'"' || bytes[at] == b'\'') {
                let quote = bytes[at];
                at += 1;
                let value_start = at;
                while at < bytes.len() && bytes[at] != quote {
                    at += 1;
                }
                value = html[value_start..at].to_string();
                if at < bytes.len() {
                    at += 1;
                }
            } else {
                let value_start = at;
                while at < bytes.len() && !bytes[at].is_ascii_whitespace() && bytes[at] != b'>' {
                    at += 1;
                }
                value = html[value_start..at].to_string();
            }
        }

        out.push((name, value));
    }

    (out, bytes.len())
}

/* ----------------------------------------------------------------- entities */

/// Decode the entities that appear in an address. A full named-entity table
/// would be a dependency for a handful of characters; the ones here are the
/// ones a link can hold, and an unknown entity is left as it was written.
fn decode_entities(value: &str) -> String {
    if !value.contains('&') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;

    while let Some(position) = rest.find('&') {
        out.push_str(&rest[..position]);
        rest = &rest[position..];

        let mut end = None;
        for (index, character) in rest.char_indices().skip(1).take(12) {
            if character == ';' {
                end = Some(index);
                break;
            }
            if !character.is_ascii_alphanumeric() && character != '#' {
                break;
            }
        }
        let Some(end) = end else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };

        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => numeric_entity(entity),
        };
        match decoded {
            Some(character) => {
                out.push(character);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn numeric_entity(entity: &str) -> Option<char> {
    let number = entity.strip_prefix('#')?;
    let code = match number.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => number.parse::<u32>().ok()?,
    };
    char::from_u32(code)
}

fn find(haystack: &str, needle: &str) -> Option<usize> {
    haystack.find(needle)
}

fn find_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.org/docs/page.html").expect("a page")
    }

    fn addresses(html: &str) -> Vec<String> {
        links(html, &base())
            .iter()
            .map(|link| link.url.to_string())
            .collect()
    }

    #[test]
    fn relative_absolute_and_protocol_relative_links_resolve() {
        let found = addresses(
            r#"<a href="film.mp4">a</a>
               <a href="/files/b.zip">b</a>
               <a href="https://cdn.example.net/c.iso">c</a>
               <a href="//cdn.example.net/d.pdf">d</a>
               <a href="../up.mkv">e</a>"#,
        );
        assert!(found.contains(&"https://example.org/docs/film.mp4".to_string()));
        assert!(found.contains(&"https://example.org/files/b.zip".to_string()));
        assert!(found.contains(&"https://cdn.example.net/c.iso".to_string()));
        assert!(found.contains(&"https://cdn.example.net/d.pdf".to_string()));
        assert!(found.contains(&"https://example.org/up.mkv".to_string()));
    }

    #[test]
    fn comments_and_scripts_do_not_invent_links() {
        let found = addresses(
            r#"<!-- <a href="/hidden.mp4">hidden</a> -->
               <script>var x = '<a href="/script.mp4">';</script>
               <style>a::after { content: "<a href='/style.mp4'>"; }</style>
               <a href="/real.mp4">real</a>"#,
        );
        assert_eq!(found, vec!["https://example.org/real.mp4"]);
    }

    #[test]
    fn a_quote_can_hold_a_greater_than_sign() {
        let found = addresses(r#"<a title="a > b" href="/real.zip">x</a>"#);
        assert_eq!(found, vec!["https://example.org/real.zip"]);
    }

    #[test]
    fn an_entity_in_an_address_is_decoded() {
        let found = addresses(r#"<a href="/get?file=a.zip&amp;token=1">x</a>"#);
        assert_eq!(found, vec!["https://example.org/get?file=a.zip&token=1"]);
        assert_eq!(decode_entities("a&#46;zip"), "a.zip");
        assert_eq!(decode_entities("a&#x2e;zip"), "a.zip");
        assert_eq!(decode_entities("a&unknown;b"), "a&unknown;b");
        assert_eq!(decode_entities("a & b"), "a & b");
    }

    #[test]
    fn script_only_schemes_and_credentials_are_dropped() {
        let found = addresses(
            r##"<a href="javascript:alert(1)">x</a>
               <a href="mailto:a@example.org">m</a>
               <a href="data:text/plain,x">d</a>
               <a href="file:///c:/windows">f</a>
               <a href="https://user:secret@example.org/a.zip">c</a>
               <a href="#top">t</a>"##,
        );
        assert!(found.is_empty(), "no link is usable: {found:?}");
    }

    #[test]
    fn media_sources_are_links_but_not_pages_to_follow() {
        let found = links(
            r#"<img src="/a.jpg" srcset="/b.jpg 1x, /c.jpg 2x">
               <video src="/film.mp4"><source src="/film.webm"></video>"#,
            &base(),
        );
        let names: Vec<&str> = found.iter().map(|link| link.url.path()).collect();
        assert_eq!(
            names,
            vec!["/a.jpg", "/b.jpg", "/c.jpg", "/film.mp4", "/film.webm"]
        );
        assert!(found.iter().all(|link| !link.crawl));
    }

    #[test]
    fn an_inline_image_is_not_split_into_fake_links() {
        let found = links(
            r#"<img srcset="data:image/png;base64,AAAA 1x, BBBB 2x">"#,
            &base(),
        );
        assert!(found.is_empty(), "an inline image is no file: {found:?}");
    }

    #[test]
    fn nofollow_is_not_followed_but_is_still_offered() {
        let found = links(
            r#"<a href="/private/a.zip" rel="external nofollow">x</a>"#,
            &base(),
        );
        assert_eq!(found.len(), 1);
        assert!(!found[0].crawl);
        assert_eq!(found[0].url.path(), "/private/a.zip");
    }

    #[test]
    fn an_address_is_listed_once_and_an_anchor_wins_over_a_picture() {
        let found = links(r#"<img src="/a.zip"><a href="/a.zip">a</a>"#, &base());
        assert_eq!(found.len(), 1);
        assert!(found[0].crawl);
        assert_eq!(found[0].url.path(), "/a.zip");
    }

    #[test]
    fn a_base_tag_moves_where_relative_links_point() {
        let found = links(
            r#"<base href="https://cdn.example.org/assets/"><a href="a.zip">a</a>"#,
            &base(),
        );
        assert_eq!(
            found[0].url.to_string(),
            "https://cdn.example.org/assets/a.zip"
        );

        let found = links(r#"<a href="a.zip">a</a><base href="/media/">"#, &base());
        assert_eq!(
            found[0].url.to_string(),
            "https://example.org/media/a.zip",
            "the base applies even when it comes after the anchor"
        );

        let found = links(
            r#"<base href="javascript:alert(1)"><a href="a.zip">a</a>"#,
            &base(),
        );
        assert_eq!(found[0].url.to_string(), "https://example.org/docs/a.zip");
    }

    #[test]
    fn a_fragment_is_not_part_of_the_address() {
        let found = addresses(r#"<a href="/page.html#section">x</a>"#);
        assert_eq!(found, vec!["https://example.org/page.html"]);
    }

    #[test]
    fn a_cut_off_page_is_read_as_far_as_it_goes() {
        // The second tag is cut inside a later attribute; the address before
        // the cut is still a link the reader could click.
        let found = addresses(r#"<a href="/one.zip">one</a><a href="/two.zip" title="unfinished"#);
        assert_eq!(
            found,
            vec!["https://example.org/one.zip", "https://example.org/two.zip"]
        );
        assert!(links("<a href=", &base()).is_empty());
        assert!(links("<", &base()).is_empty());
    }

    #[test]
    fn a_page_with_more_links_than_the_limit_stops_at_it() {
        let mut html = String::new();
        for index in 0..(MAX_PAGE_LINKS + 50) {
            html.push_str(&format!("<a href=\"/f{index}.zip\">x</a>"));
        }
        assert_eq!(links(&html, &base()).len(), MAX_PAGE_LINKS);
    }
}
