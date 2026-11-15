//! Image galleries: the pictures of one page, as a list the engine can take.
//!
//! The reference product shells out to gallery-dl for this. Here the page is
//! read directly: the tags that carry a picture address are looked for, the
//! address is resolved against the page's own address, and what comes out is a
//! list of files. Nothing is fetched — the caller decides what to download and
//! when.

use std::collections::HashSet;

use reqwest::Url;
use serde::{Deserialize, Serialize};

/// The most images one gallery may hold.
///
/// A page can carry far more pictures than a person wants; the rest are left
/// alone and the gallery says so.
pub const MAX_IMAGES: usize = 500;

/// The most bytes a whole gallery may take (8 GiB).
///
/// The engine cannot know the size of a picture before it asks for it, so this
/// cap belongs to the caller, which sums what the server reports.
pub const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// The most tags one page may hold.
const MAX_TAGS: usize = 20_000;

/// The longest folder name one gallery may have.
const MAX_FOLDER_LENGTH: usize = 120;

/// The extension a picture is given when its address names none.
const DEFAULT_EXTENSION: &str = "jpg";

/// The extensions that mark an address as a picture or a clip.
const MEDIA_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "avif", "bmp", "svg", "tif", "tiff", "heic", "heif", "mp4",
    "m4v", "webm", "mov",
];

/// The characters a file name may not carry on any of the systems a download
/// can land on.
const FORBIDDEN: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// One picture of a gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryImage {
    /// The absolute address of the picture.
    pub url: String,
    /// The name the picture is written under.
    pub name: String,
    /// Where the picture sits in the gallery, counting from one.
    pub index: usize,
}

/// The pictures of one gallery page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gallery {
    /// The page's own title, cleaned into a folder name by [`folder_name`].
    pub title: String,
    /// The pictures, in the order the page shows them.
    pub images: Vec<GalleryImage>,
    /// The address of the page the pictures came from.
    #[serde(default)]
    pub page: Option<String>,
    /// Whether the page held more pictures than [`MAX_IMAGES`].
    #[serde(default)]
    pub truncated: bool,
}

/// One picture of a gallery.
pub fn extract(html: &str, base_url: &str) -> Gallery {
    let base = Url::parse(base_url.trim()).ok();
    let page = read_page(html, base.as_ref());

    let mut images: Vec<GalleryImage> = Vec::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut truncated = false;
    for url in &page.candidates {
        let Some(parsed) = Url::parse(url).ok() else {
            continue;
        };
        let Some(segment) = last_segment(&parsed) else {
            continue;
        };
        let name = picture_name(&segment);
        if name.is_empty() {
            continue;
        }
        // Two addresses can carry the same file name; the second one would
        // overwrite the first.
        if !names.insert(name.to_lowercase()) {
            continue;
        }
        if images.len() == MAX_IMAGES {
            truncated = true;
            break;
        }
        images.push(GalleryImage {
            url: url.clone(),
            name,
            index: images.len() + 1,
        });
    }

    let title = if page.title.is_empty() {
        page.heading
    } else {
        page.title
    };
    Gallery {
        title,
        images,
        page: match base_url.trim() {
            "" => None,
            page => Some(page.to_string()),
        },
        truncated,
    }
}

/// Whether a page looks like a gallery rather than one ordinary download.
///
/// Two rules, and neither is a guess from a single word: the page holds
/// several addresses that look like full-size pictures, or its own address
/// carries the shape of a gallery (`/gallery/`, `/galleries/`, `/post/`,
/// `/album/`). A page with one picture is an ordinary download whatever its
/// address says, so the count has to reach two before the shape counts.
pub fn looks_like_gallery(url: &str, html: &str) -> bool {
    let base = Url::parse(url.trim()).ok();
    let page = read_page(html, base.as_ref());

    let mut seen: HashSet<&str> = HashSet::new();
    let mut count = 0usize;
    for candidate in &page.candidates {
        if !has_media_extension(candidate) {
            continue;
        }
        if seen.insert(candidate.as_str()) {
            count += 1;
        }
    }

    if count >= 5 {
        return true;
    }
    if count < 2 {
        return false;
    }
    gallery_shaped(url)
}

/// The folder name one gallery is written into.
///
/// The page's title, without the characters a file name may not carry on any
/// system, and short enough for every system to accept.
pub fn folder_name(gallery: &Gallery) -> String {
    let cleaned = clean(&gallery.title);
    let capped: String = cleaned.chars().take(MAX_FOLDER_LENGTH).collect();
    let capped = capped.trim().to_string();
    if capped.is_empty() {
        return "gallery".to_string();
    }
    capped
}

/// The file name one picture is written under.
///
/// The picture's place in the gallery, then its own name. `title` is only
/// used when the picture has no name of its own.
pub fn file_name(image: &GalleryImage, title: &str) -> String {
    let name = if image.name.trim().is_empty() {
        let fallback = clean(title);
        if fallback.is_empty() {
            "image".to_string()
        } else {
            fallback
        }
    } else {
        image.name.clone()
    };
    format!("{:02}_{}", image.index.max(1), name)
}

/// What one page holds.
struct Page {
    title: String,
    heading: String,
    candidates: Vec<String>,
}

/// One event of a page.
enum Event {
    Start { name: String, attrs: Vec<(String, String)> },
    End { name: String },
    Text(String),
}

/// Read one page and collect the addresses that point at a picture.
fn read_page(html: &str, base: Option<&Url>) -> Page {
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

/// The last segment of an address's path, without the query.
fn last_segment(url: &Url) -> Option<String> {
    url.path_segments()?.next_back().map(|segment| segment.to_string())
}

/// Whether an address names a picture or a clip.
fn has_media_extension(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let Some(segment) = last_segment(&parsed) else {
        return false;
    };
    let decoded = percent_decode(&segment);
    let Some((_, extension)) = decoded.rsplit_once('.') else {
        return false;
    };
    MEDIA_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
}

/// The name a picture is written under.
fn picture_name(segment: &str) -> String {
    let decoded = percent_decode(segment);
    let (stem, extension) = match decoded.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => {
            (stem.to_string(), Some(extension.to_string()))
        }
        _ => (decoded.clone(), None),
    };
    let stem = clean(&stem);
    if stem.is_empty() {
        return String::new();
    }
    let extension = extension
        .map(|extension| clean(&extension))
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| DEFAULT_EXTENSION.to_string());
    format!("{stem}.{extension}")
}

/// Whether an address carries the shape of a gallery.
fn gallery_shaped(url: &str) -> bool {
    let path = match Url::parse(url.trim()) {
        Ok(parsed) => parsed.path().to_ascii_lowercase(),
        Err(_) => url.to_ascii_lowercase(),
    };
    ["/gallery/", "/galleries/", "/post/", "/album/"]
        .iter()
        .any(|shape| path.contains(shape))
}

/// One attribute of a tag.
fn attr(attrs: &[(String, String)], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

/// Drop what a file name may not carry and tidy the spaces that leaves.
fn clean(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_control() || FORBIDDEN.contains(&character) {
            out.push(' ');
        } else {
            out.push(character);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
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
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'>' {
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

/// Read `%xx` as the byte it names.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 3 <= bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIX: &str = r#"
        <html><head><title>Winter Set</title></head><body>
          <div class="gallery">
            <img src="/img/one.jpg" alt="one">
            <img data-src="two.jpg">
            <img srcset="three-640.jpg 640w, three-1280.jpg 1280w">
            <img src="/img/four-small.png" data-src="/img/four.png">
            <img src="https://cdn.example.com/five.webp">
            <a href="six.jpg"><img src="thumb-six.jpg"></a>
          </div>
        </body></html>
    "#;

    #[test]
    fn every_image_of_a_page_is_found() {
        let gallery = extract(SIX, "https://example.com/post/1");
        assert_eq!(gallery.title, "Winter Set");
        assert_eq!(gallery.page.as_deref(), Some("https://example.com/post/1"));
        assert!(!gallery.truncated);

        let names: Vec<&str> = gallery.images.iter().map(|image| image.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "one.jpg",
                "two.jpg",
                "three-1280.jpg",
                "four.png",
                "five.webp",
                "six.jpg"
            ]
        );
        assert_eq!(
            gallery.images[0].url,
            "https://example.com/img/one.jpg"
        );
        assert_eq!(
            gallery.images[1].url,
            "https://example.com/post/two.jpg"
        );
        assert_eq!(
            gallery.images[2].url,
            "https://example.com/post/three-1280.jpg"
        );
        assert_eq!(gallery.images[4].url, "https://cdn.example.com/five.webp");
        assert_eq!(gallery.images[5].url, "https://example.com/post/six.jpg");
        assert_eq!(
            gallery.images.iter().map(|image| image.index).collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn the_same_picture_is_taken_once() {
        let gallery = extract(
            r#"<img src="/a/photo.jpg"><img src="/a/photo.jpg"><img src="/b/photo.jpg"><img src="/b/other.jpg">"#,
            "https://example.com/g/",
        );
        let names: Vec<&str> = gallery.images.iter().map(|image| image.name.as_str()).collect();
        assert_eq!(names, vec!["photo.jpg", "other.jpg"]);
    }

    #[test]
    fn one_picture_is_not_a_gallery() {
        let html = r#"<img src="/img/only.jpg">"#;
        assert!(!looks_like_gallery("https://example.com/photo.html", html));
        assert!(!looks_like_gallery("https://example.com/post/1", html));
    }

    #[test]
    fn many_pictures_are_a_gallery() {
        assert!(looks_like_gallery("https://example.com/photo.html", SIX));
        let html = r#"<img src="/a.jpg"><img src="/b.jpg">"#;
        assert!(looks_like_gallery("https://example.com/gallery/1", html));
        assert!(!looks_like_gallery("https://example.com/photo.html", html));
    }

    #[test]
    fn a_folder_name_carries_no_forbidden_character() {
        let gallery = Gallery {
            title: "A: \"B\" <C> / D \\ E? F*G | H\u{1}I".to_string(),
            images: Vec::new(),
            page: None,
            truncated: false,
        };
        assert_eq!(folder_name(&gallery), "A B C D E F G H I");

        let empty = Gallery {
            title: String::new(),
            images: Vec::new(),
            page: None,
            truncated: false,
        };
        assert_eq!(folder_name(&empty), "gallery");
    }

    #[test]
    fn a_file_name_carries_its_place_in_the_gallery() {
        let image = GalleryImage {
            url: "https://example.com/img/photo.jpg".to_string(),
            name: "photo.jpg".to_string(),
            index: 1,
        };
        assert_eq!(file_name(&image, "Winter Set"), "01_photo.jpg");

        let later = GalleryImage {
            index: 12,
            ..image
        };
        assert_eq!(file_name(&later, "Winter Set"), "12_photo.jpg");
    }

    #[test]
    fn the_cap_stops_the_list_and_says_so() {
        let mut html = String::new();
        for index in 0..=MAX_IMAGES {
            html.push_str(&format!("<img src=\"/i/{index}.jpg\">"));
        }
        let gallery = extract(&html, "https://example.com/g/");
        assert_eq!(gallery.images.len(), MAX_IMAGES);
        assert!(gallery.truncated);

        let mut within = String::new();
        for index in 0..MAX_IMAGES {
            within.push_str(&format!("<img src=\"/i/{index}.jpg\">"));
        }
        let gallery = extract(&within, "https://example.com/g/");
        assert_eq!(gallery.images.len(), MAX_IMAGES);
        assert!(!gallery.truncated);
    }

    #[test]
    fn a_data_address_and_a_link_to_another_page_are_ignored() {
        let gallery = extract(
            r#"<img src="data:image/png;base64,AAA"><a href="/other/page/">next page</a>"#,
            "https://example.com/g/",
        );
        assert!(gallery.images.is_empty());
    }

    #[test]
    fn a_link_to_another_page_keeps_its_own_pictures() {
        let gallery = extract(
            r#"<a href="/other/page/"><img src="/pic/inside.jpg"></a>"#,
            "https://example.com/g/",
        );
        assert_eq!(gallery.images.len(), 1);
        assert_eq!(gallery.images[0].url, "https://example.com/pic/inside.jpg");
    }

    #[test]
    fn a_name_is_read_from_the_address() {
        let gallery = extract(
            r#"<img src="/img/my%20photo.jpg"><img src="/img/no-extension">"#,
            "https://example.com/g/",
        );
        let names: Vec<&str> = gallery.images.iter().map(|image| image.name.as_str()).collect();
        assert_eq!(names, vec!["my photo.jpg", "no-extension.jpg"]);
    }

    #[test]
    fn a_heading_is_used_when_the_page_has_no_title() {
        let gallery = extract(r#"<h1>Autumn Set</h1><img src="/a.jpg">"#, "https://example.com/g/");
        assert_eq!(gallery.title, "Autumn Set");
    }
}
