//! Reading a link as a file: its type, its name, and the category the list
//! will show.

use reqwest::Url;
use serde::Serialize;

use crate::bridge::dest;

/// The kinds of file the window offers. They are the grabber's own grouping;
/// `category` maps them onto the download list's built-in categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Video,
    Music,
    Documents,
    Images,
    Archives,
    Programs,
    Other,
}

/// Every kind, in the order the window shows them.
pub const KINDS: [FileKind; 7] = [
    FileKind::Video,
    FileKind::Music,
    FileKind::Documents,
    FileKind::Images,
    FileKind::Archives,
    FileKind::Programs,
    FileKind::Other,
];

impl FileKind {
    /// The word the window and the bridge use for this kind.
    pub fn token(self) -> &'static str {
        match self {
            FileKind::Video => "video",
            FileKind::Music => "music",
            FileKind::Documents => "documents",
            FileKind::Images => "images",
            FileKind::Archives => "archives",
            FileKind::Programs => "programs",
            FileKind::Other => "other",
        }
    }

    /// The built-in category a download of this kind belongs to.
    pub fn category(self) -> &'static str {
        match self {
            FileKind::Video => "Video",
            FileKind::Music => "Music",
            FileKind::Documents => "Documents",
            FileKind::Programs => "Programs",
            FileKind::Archives => "Compressed",
            // The list has no picture category, and a picture is not a
            // document; Other is the built-in that claims neither.
            FileKind::Images | FileKind::Other => "Other",
        }
    }

    /// Read a token back. An unknown token is not a kind.
    pub fn from_token(value: &str) -> Option<Self> {
        KINDS
            .iter()
            .copied()
            .find(|kind| kind.token().eq_ignore_ascii_case(value.trim()))
    }
}

const VIDEO: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "m4v", "mpg", "mpeg", "wmv", "flv", "3gp", "ogv",
];
const MUSIC: &[&str] = &[
    "mp3", "flac", "wav", "m4a", "ogg", "aac", "opus", "wma", "aiff", "mid",
];
const DOCUMENTS: &[&str] = &[
    "pdf", "doc", "docx", "txt", "rtf", "xls", "xlsx", "ppt", "pptx", "csv", "epub", "odt", "ods",
    "odp", "mobi", "djvu",
];
const IMAGES: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "tif", "tiff", "avif", "heic", "jxl",
];
const ARCHIVES: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "cab", "lz", "zst",
];
const PROGRAMS: &[&str] = &[
    "exe", "msi", "msix", "dmg", "pkg", "apk", "deb", "rpm", "appimage", "jar",
];

/// The extensions that belong to a page rather than to a file, plus the
/// manifests of a stream, which are a player's business and not a file to save.
/// The extensions that are a page a crawl may follow, as opposed to an asset
/// or a file. A link without an extension is a page until the server says
/// otherwise.
const PAGE_DOCUMENTS: &[&str] = &["html", "htm", "xhtml", "php", "asp", "aspx", "jsp", "cgi"];

const PAGE_PARTS: &[&str] = &[
    "html",
    "htm",
    "xhtml",
    "php",
    "asp",
    "aspx",
    "jsp",
    "cgi",
    "css",
    "js",
    "mjs",
    "cjs",
    "json",
    "xml",
    "rss",
    "atom",
    "map",
    "woff",
    "woff2",
    "ttf",
    "otf",
    "eot",
    "webmanifest",
    "ico",
    "m3u8",
    "mpd",
    "m4s",
    "f4m",
    "ism",
    "ts",
];

/// The kind of a file type, or `None` when the type is a page part.
///
/// An extension nobody claimed is [`FileKind::Other`]: it may be a file worth
/// keeping, and the window leaves that kind unticked until the user asks.
pub fn kind_of(extension: &str) -> Option<FileKind> {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    if extension.is_empty() {
        return None;
    }
    let has = |list: &[&str]| list.iter().any(|known| *known == extension);
    if PAGE_PARTS.contains(&extension.as_str()) {
        return None;
    }
    if has(VIDEO) {
        Some(FileKind::Video)
    } else if has(MUSIC) {
        Some(FileKind::Music)
    } else if has(DOCUMENTS) {
        Some(FileKind::Documents)
    } else if has(IMAGES) {
        Some(FileKind::Images)
    } else if has(ARCHIVES) {
        Some(FileKind::Archives)
    } else if has(PROGRAMS) {
        Some(FileKind::Programs)
    } else {
        Some(FileKind::Other)
    }
}

/// The extension of the last segment of a link's path, without the dot.
///
/// A query string can name a file too (`?file=film.mp4`), but guessing there
/// would offer links that are not files. The path is the only honest source.
pub fn extension_of(url: &Url) -> Option<String> {
    let segment = url.path_segments()?.rfind(|part| !part.is_empty())?;
    let decoded = percent_decode(segment);
    let dot = decoded.rfind('.')?;
    let extension = decoded[dot + 1..].to_ascii_lowercase();
    if extension.is_empty() || extension.len() > 16 {
        return None;
    }
    if !extension
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '_'))
    {
        return None;
    }
    Some(extension)
}

/// Whether a link is worth requesting as a page.
///
/// An address without an extension is a page until the server proves
/// otherwise, and a page extension is always a page. A stylesheet, a script
/// or a stream manifest is an asset: following it would only fetch a file the
/// parser cannot read.
pub fn is_page_document(url: &Url) -> bool {
    match extension_of(url) {
        Some(extension) => PAGE_DOCUMENTS.contains(&extension.as_str()),
        None => true,
    }
}

/// A file the crawl found, with everything the window and the download need.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub url: String,
    pub name: String,
    pub extension: String,
    pub kind: FileKind,
    /// The built-in category the download list will show.
    pub category: String,
    /// The page the link was found on.
    pub page: String,
}

impl Candidate {
    /// Read one link as a candidate, when its type is one the grabber offers.
    pub fn from_url(url: &Url, page: &str) -> Option<Self> {
        let extension = extension_of(url)?;
        let kind = kind_of(&extension)?;
        let name = dest::name_from_url(url.as_str()).unwrap_or_else(|| "download".to_string());
        Some(Self {
            url: url.as_str().to_string(),
            name,
            extension,
            kind,
            category: kind.category().to_string(),
            page: page.to_string(),
        })
    }
}

/// An address without its query, for a line in the window's log.
///
/// The query may carry a token that signs one file for one reader; a log that
/// repeats it is a log that leaks it, and the log is read over shoulders.
pub fn short(url: &Url) -> String {
    let mut out = String::new();
    if let Some(host) = url.host_str() {
        out.push_str(host);
    }
    out.push_str(url.path());
    out
}

/// Decode the percent escapes of one path segment.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
