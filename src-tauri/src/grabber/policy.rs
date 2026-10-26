//! What one crawl may reach, and what it may offer.
//!
//! A grabber aims a request at whatever address it is given, and then follows
//! what that page points at. Both halves are a hazard: an address may name the
//! machine the application runs on, a name may resolve into the router's
//! network, and a redirect may send the reader somewhere the first check never
//! saw. Every request therefore goes through [`check_public`], and every hop of
//! a redirect is checked again before it is followed.
//!
//! The rules are deliberately plain and in one place, so the window can explain
//! a refusal with the same sentence the crawl used.

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use reqwest::Url;
use serde::Serialize;

use crate::bridge::dest;

/// The most pages one crawl reads.
pub const MAX_PAGES: usize = 500;

/// The most links a page may hold before the rest are left unread.
pub const MAX_PAGE_LINKS: usize = 2_000;

/// The deepest a crawl may follow page links. One level is the start page.
pub const MAX_DEPTH: u8 = 5;

/// The most files one crawl offers. Beyond this the list is frozen and the
/// window says so, instead of growing until the window crawls.
pub const MAX_FILES: usize = 2_000;

/// The pause between two page requests. A grabber that fires as fast as the
/// site answers is a small denial of service, and every crawl is one user.
pub const PAGE_DELAY: Duration = Duration::from_millis(300);

/// The longest page accepted. A page is a directory, not a download.
pub const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;

/// The redirects one page may travel through.
pub const MAX_REDIRECTS: usize = 5;

/// The time one page request may take, body included.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// The longest `robots.txt` read.
pub const MAX_ROBOTS_BYTES: usize = 512 * 1024;

/// The longest `Crawl-delay` honoured. A directive asking for more than this
/// is reported instead of followed; the crawl never waits minutes per page.
pub const MAX_CRAWL_DELAY: Duration = Duration::from_secs(5);

/// The User-Agent every request of the grabber carries.
///
/// It names the grabber honestly and gives robots.txt a token to match. A site
/// that only serves browsers may refuse it; that is the site's decision, and
/// the window reports the refusal.
pub const USER_AGENT: &str = "BDM-SiteGrabber/0.1";

/// The token robots.txt groups match against.
pub const ROBOTS_AGENT: &str = "BDM-SiteGrabber";

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

/// Whether a link stays on the site that was asked for.
///
/// A site is one host plus its `www.` twin, which is one site in the everyday
/// sense and two only in DNS. A subdomain is another host: following it would
/// turn one request into a crawl of whatever shares the domain, which is not
/// what the user pointed at.
pub fn site_hosts(start: &Url) -> Vec<String> {
    let Some(host) = start.host_str() else {
        return Vec::new();
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if let Some(rest) = host.strip_prefix("www.") {
        vec![host.clone(), rest.to_string()]
    } else if host.parse::<IpAddr>().is_ok() {
        vec![host]
    } else {
        vec![host.clone(), format!("www.{host}")]
    }
}

pub fn same_site(url: &Url, hosts: &[String]) -> bool {
    url.host_str()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
        .is_some_and(|host| hosts.contains(&host))
}

/// The names that are never a public site.
const BLOCKED_HOSTS: &[&str] = &[
    "localhost",
    "metadata",
    "metadata.google.internal",
    "metadata.goog",
    "instance-data",
    "instance-data.ec2.internal",
    "metadata.azure.internal",
];

/// The suffixes that only mean something inside one network.
const LOCAL_SUFFIXES: &[&str] = &[
    ".localhost",
    ".local",
    ".internal",
    ".home.arpa",
    ".lan",
    ".test",
    ".invalid",
    ".example",
    ".onion",
];

/// Why an address cannot be crawled, or `None` when it can.
///
/// This checks the spelling of the host only; [`check_public`] also resolves
/// it, because a public name can point at a private address.
pub fn host_problem(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Some("The address has no host.".to_string());
    }
    if BLOCKED_HOSTS.contains(&host.as_str()) {
        return Some("That address is this computer, or a cloud metadata endpoint.".to_string());
    }
    if let Ok(address) = host.parse::<IpAddr>() {
        if !addr_is_public(address) {
            return Some("That address is on this computer or a private network.".to_string());
        }
        return None;
    }
    if LOCAL_SUFFIXES.iter().any(|suffix| host.ends_with(suffix)) {
        return Some("That name only means something inside one network.".to_string());
    }
    // A name without a dot is looked up through the local search suffix, which
    // is how `intranet` becomes a machine next door rather than a site.
    if !host.contains('.') {
        return Some("That name only means something inside one network.".to_string());
    }
    None
}

/// Whether an address is one the public internet can reach.
pub fn addr_is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => ipv4_is_public(address),
        IpAddr::V6(address) => ipv6_is_public(address),
    }
}

/// The IPv4 ranges that are not a public destination.
///
/// This is every special-purpose block a request must not travel to: the
/// machine, the local network, the carrier's shared space, the documentation
/// ranges, and the blocks reserved for the future.
const BLOCKED_V4: &[(Ipv4Addr, u8)] = &[
    (Ipv4Addr::new(0, 0, 0, 0), 8),
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    (Ipv4Addr::new(192, 0, 2, 0), 24),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    (Ipv4Addr::new(198, 18, 0, 0), 15),
    (Ipv4Addr::new(198, 51, 100, 0), 24),
    (Ipv4Addr::new(203, 0, 113, 0), 24),
    (Ipv4Addr::new(224, 0, 0, 0), 4),
    (Ipv4Addr::new(240, 0, 0, 0), 4),
];

fn ipv4_is_public(address: Ipv4Addr) -> bool {
    let value = u32::from(address);
    !BLOCKED_V4.iter().any(|(network, bits)| {
        let mask = u32::MAX << (32 - bits);
        value & mask == u32::from(*network) & mask
    })
}

fn ipv6_is_public(address: Ipv6Addr) -> bool {
    if let Some(embedded) = embedded_ipv4(address) {
        return ipv4_is_public(embedded);
    }
    let segments = address.segments();
    if address.is_loopback() || address.is_unspecified() || address.is_multicast() {
        return false;
    }
    // fc00::/7 is the private space of IPv6, fe80::/10 the link-local one.
    if segments[0] & 0xfe00 == 0xfc00 || segments[0] & 0xffc0 == 0xfe80 {
        return false;
    }
    // 2001:db8::/32 and 100::/64 are documentation and discard space.
    if (segments[0] == 0x2001 && segments[1] == 0x0db8)
        || (segments[0] == 0x0100 && segments[1..4].iter().all(|segment| *segment == 0))
    {
        return false;
    }
    true
}

/// The IPv4 address an IPv6 address carries, when it carries one.
///
/// Both spellings matter: `::ffff:127.0.0.1` is how a resolver often reports an
/// IPv4 address, and `::127.0.0.1` is the older compatible form.
fn embedded_ipv4(address: Ipv6Addr) -> Option<Ipv4Addr> {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return Some(mapped);
    }
    let segments = address.segments();
    // 64:ff9b::/96 is the NAT64 prefix: the last 32 bits are the IPv4
    // address the network would actually connect to.
    if segments[0] == 0x0064
        && segments[1] == 0xff9b
        && segments[2..6].iter().all(|segment| *segment == 0)
    {
        let high = segments[6];
        let low = segments[7];
        return Some(Ipv4Addr::new(
            (high >> 8) as u8,
            high as u8,
            (low >> 8) as u8,
            low as u8,
        ));
    }
    if segments[..6].iter().all(|segment| *segment == 0) {
        let last = segments[7];
        let before = segments[6];
        return Some(Ipv4Addr::new(
            (before >> 8) as u8,
            before as u8,
            (last >> 8) as u8,
            last as u8,
        ));
    }
    None
}

/// Whether an address is a web address at all. The host is neither spelled
/// out nor resolved; that is [`check_literal`]'s and [`check_public`]'s job.
pub fn check_web(url: &Url) -> Result<(), String> {
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("Only http:// and https:// addresses can be read.".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("An address with a user name or password is not followed.".to_string());
    }
    url.host_str()
        .ok_or_else(|| "The address has no host.".to_string())?;
    Ok(())
}

/// Whether the address itself is allowed. The host is not resolved.
pub fn check_literal(url: &Url) -> Result<(), String> {
    check_web(url)?;
    let host = url
        .host_str()
        .ok_or_else(|| "The address has no host.".to_string())?;
    if let Some(problem) = host_problem(host) {
        return Err(problem);
    }
    Ok(())
}

/// Whether a request may be sent to this address, after resolving its host.
///
/// Every page and every redirect goes through this. A name that resolves to
/// the machine, the local network or a metadata endpoint is refused before the
/// connection is made. A name that resolves to several addresses is refused if
/// any of them is private: a request must not be able to pick the private one.
pub async fn check_public(url: &Url) -> Result<(), String> {
    check_literal(url)?;
    let host = url
        .host_str()
        .ok_or_else(|| "The address has no host.".to_string())?;
    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| "The address of the site could not be looked up.".to_string())?;
    let mut seen = false;
    for address in addresses {
        seen = true;
        if !addr_is_public(address.ip()) {
            return Err("That name leads to a private network.".to_string());
        }
    }
    if !seen {
        return Err("The address of the site could not be looked up.".to_string());
    }
    Ok(())
}

/// The paths and patterns the user asked to leave out.
#[derive(Debug, Clone, Default)]
pub struct Exclusions {
    patterns: Vec<Pattern>,
}

#[derive(Debug, Clone)]
struct Pattern {
    text: String,
    /// A pattern beginning with `/` matches the path only; anything else may
    /// also match the host and scheme.
    path_only: bool,
}

impl Exclusions {
    /// Read the typed list. Junk, duplicates and anything past the limit are
    /// dropped rather than refused: one bad line should not stop a crawl.
    pub fn parse(values: &[String]) -> Self {
        const LIMIT: usize = 32;
        let mut patterns: Vec<Pattern> = Vec::new();
        for value in values {
            let text = value.trim();
            if text.is_empty() || text.chars().count() > 200 {
                continue;
            }
            if patterns
                .iter()
                .any(|known| known.text.eq_ignore_ascii_case(text))
            {
                continue;
            }
            patterns.push(Pattern {
                text: text.to_string(),
                path_only: text.starts_with('/') && !text.contains("://"),
            });
            if patterns.len() >= LIMIT {
                break;
            }
        }
        Self { patterns }
    }

    /// Whether one link matches any pattern. The path and query are checked,
    /// and a pattern without a leading `/` is also checked against the host.
    pub fn matches(&self, url: &Url) -> bool {
        if self.patterns.is_empty() {
            return false;
        }
        let mut path = url.path().to_string();
        if let Some(query) = url.query() {
            path.push('?');
            path.push_str(query);
        }
        let whole = url.as_str();
        let lowered = whole.to_ascii_lowercase();
        self.patterns.iter().any(|pattern| {
            if pattern.path_only {
                // A path rule is a prefix, like a rule for a folder: `/private`
                // leaves out `/private/a` as well as `/private` itself.
                wildcard_prefix(&pattern.text, &path)
            } else {
                // Without a leading `/` a rule may name a host, a fragment of
                // an address, or a pattern. All three readings are tried.
                lowered.contains(&pattern.text.to_ascii_lowercase())
                    || wildcard(&pattern.text, &path)
                    || wildcard(&pattern.text, whole)
            }
        })
    }
}

/// A small glob: `*` matches any run of characters, everything else is taken
/// literally. The comparison ignores ASCII case, because a path on a server
/// may be either and a rule typed by a person should not depend on it.
fn wildcard(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_ascii_lowercase().chars().collect();
    let text: Vec<char> = text.to_ascii_lowercase().chars().collect();
    let (mut pattern_at, mut text_at) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut resume = 0usize;

    while text_at < text.len() {
        if pattern_at < pattern.len() && pattern[pattern_at] == text[text_at] {
            pattern_at += 1;
            text_at += 1;
        } else if pattern_at < pattern.len() && pattern[pattern_at] == '*' {
            star = Some(pattern_at);
            pattern_at += 1;
            resume = text_at;
        } else if let Some(position) = star {
            pattern_at = position + 1;
            resume += 1;
            text_at = resume;
        } else {
            return false;
        }
    }
    while pattern_at < pattern.len() && pattern[pattern_at] == '*' {
        pattern_at += 1;
    }
    pattern_at == pattern.len()
}

/// A glob that only has to match the beginning of the text.
fn wildcard_prefix(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_ascii_lowercase().chars().collect();
    let text: Vec<char> = text.to_ascii_lowercase().chars().collect();
    let (mut pattern_at, mut text_at) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut resume = 0usize;

    loop {
        if pattern_at == pattern.len() {
            return true;
        }
        if text_at < text.len() && pattern[pattern_at] == text[text_at] {
            pattern_at += 1;
            text_at += 1;
        } else if pattern[pattern_at] == '*' {
            star = Some(pattern_at);
            pattern_at += 1;
            resume = text_at;
        } else if let Some(position) = star {
            if resume >= text.len() {
                return false;
            }
            resume += 1;
            text_at = resume;
            pattern_at = position + 1;
        } else {
            return false;
        }
    }
}

/// The file types a crawl should offer.
#[derive(Debug, Clone)]
pub struct Accepted {
    kinds: BTreeSet<FileKind>,
    extensions: BTreeSet<String>,
}

impl Accepted {
    pub fn new(kinds: impl IntoIterator<Item = FileKind>, extensions: &[String]) -> Self {
        let extensions = extensions
            .iter()
            .map(|extension| {
                extension
                    .trim()
                    .trim_start_matches('.')
                    .to_ascii_lowercase()
            })
            .filter(|extension| {
                !extension.is_empty()
                    && extension.len() <= 16
                    && extension.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '_')
                    })
            })
            .collect();
        Self {
            kinds: kinds.into_iter().collect(),
            extensions,
        }
    }

    /// Whether a candidate is one the user asked the grabber to collect.
    pub fn takes(&self, candidate: &Candidate) -> bool {
        self.kinds.contains(&candidate.kind) || self.extensions.contains(&candidate.extension)
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty() && self.extensions.is_empty()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).expect("a URL")
    }

    #[test]
    fn every_file_kind_has_a_token_and_a_category() {
        for kind in KINDS {
            assert_eq!(FileKind::from_token(kind.token()), Some(kind));
            assert!(!kind.category().is_empty());
        }
        assert_eq!(FileKind::from_token("VIDEO"), Some(FileKind::Video));
        assert_eq!(FileKind::from_token("pictures"), None);
    }

    #[test]
    fn a_type_belongs_to_the_kind_the_window_names() {
        assert_eq!(kind_of("MP4"), Some(FileKind::Video));
        assert_eq!(kind_of(".mp3"), Some(FileKind::Music));
        assert_eq!(kind_of("pdf"), Some(FileKind::Documents));
        assert_eq!(kind_of("webp"), Some(FileKind::Images));
        assert_eq!(kind_of("7z"), Some(FileKind::Archives));
        assert_eq!(kind_of("msi"), Some(FileKind::Programs));
        assert_eq!(kind_of("bin"), Some(FileKind::Other));
        assert_eq!(kind_of("html"), None, "a page is not a file to save");
        assert_eq!(kind_of("m3u8"), None, "a stream manifest is not a file");
        assert_eq!(kind_of("ts"), None);
        assert_eq!(kind_of(""), None);
    }

    #[test]
    fn the_extension_comes_from_the_path_not_the_query() {
        assert_eq!(
            extension_of(&url("https://example.org/a/film.MP4?x=1")).as_deref(),
            Some("mp4")
        );
        assert_eq!(
            extension_of(&url("https://example.org/files/report%2Epdf")).as_deref(),
            Some("pdf")
        );
        assert_eq!(
            extension_of(&url("https://example.org/get?file=a.zip")),
            None
        );
        assert_eq!(extension_of(&url("https://example.org/files/")), None);
        assert_eq!(extension_of(&url("https://example.org/a.b%20c")), None);
        assert_eq!(
            extension_of(&url("https://example.org/a.verylongextension")),
            None
        );
    }

    #[test]
    fn a_candidate_carries_the_name_and_category_the_list_needs() {
        let found = Candidate::from_url(
            &url("https://example.org/files/My%20Film.mp4?sig=1"),
            "https://example.org/",
        )
        .expect("a video");
        assert_eq!(found.name, "My Film.mp4");
        assert_eq!(found.extension, "mp4");
        assert_eq!(found.kind, FileKind::Video);
        assert_eq!(found.category, "Video");
        assert!(found.url.contains("sig=1"), "the address is kept whole");

        let archive =
            Candidate::from_url(&url("https://example.org/a.rar"), "https://example.org/")
                .expect("an archive");
        assert_eq!(archive.category, "Compressed");
    }

    #[test]
    fn an_asset_is_not_a_page_to_read() {
        assert!(!is_page_document(&url("https://example.org/style.css")));
        assert!(!is_page_document(&url("https://example.org/app.js")));
        assert!(!is_page_document(&url("https://example.org/live.m3u8")));
        assert!(!is_page_document(&url("https://example.org/data.json")));
        assert!(is_page_document(&url("https://example.org/index.html")));
        assert!(is_page_document(&url("https://example.org/about")));
        assert!(is_page_document(&url("https://example.org/")));
    }

    #[test]
    fn a_page_is_not_a_candidate() {
        assert!(Candidate::from_url(&url("https://example.org/about"), "x").is_none());
        assert!(Candidate::from_url(&url("https://example.org/index.html"), "x").is_none());
        assert!(Candidate::from_url(&url("https://example.org/a.m3u8"), "x").is_none());
    }

    #[test]
    fn a_site_is_one_host_and_its_www_twin() {
        assert_eq!(
            site_hosts(&url("https://www.Example.org/a")),
            vec!["www.example.org", "example.org"]
        );
        assert_eq!(
            site_hosts(&url("https://example.org/a")),
            vec!["example.org", "www.example.org"]
        );
        assert_eq!(
            site_hosts(&url("http://93.184.216.34/")),
            vec!["93.184.216.34"]
        );

        let hosts = site_hosts(&url("https://example.org/"));
        assert!(same_site(&url("https://www.example.org/x"), &hosts));
        assert!(same_site(&url("http://example.org/x"), &hosts));
        assert!(
            !same_site(&url("https://files.example.org/x"), &hosts),
            "a subdomain is another host"
        );
        assert!(!same_site(&url("https://example.net/x"), &hosts));
    }

    #[test]
    fn the_private_and_reserved_addresses_are_known() {
        for text in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "192.0.2.1",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "64:ff9b::7f00:1",
        ] {
            assert!(
                !addr_is_public(text.parse().expect("an address")),
                "{text} must not be reachable"
            );
        }
        for text in ["93.184.216.34", "8.8.8.8", "2606:4700::1111"] {
            assert!(
                addr_is_public(text.parse().expect("an address")),
                "{text} is public"
            );
        }
    }

    #[test]
    fn local_names_and_metadata_endpoints_are_refused() {
        for host in [
            "localhost",
            "LOCALHOST.",
            "metadata.google.internal",
            "instance-data",
            "printer.local",
            "intranet",
            "something.internal",
            "127.0.0.1",
            "[::1]",
        ] {
            let host = host.trim_matches(['[', ']']);
            assert!(host_problem(host).is_some(), "{host} must be refused");
        }
        assert_eq!(host_problem("example.org"), None);
        assert_eq!(host_problem("93.184.216.34"), None);
    }

    #[test]
    fn only_web_addresses_without_credentials_are_read() {
        assert!(check_literal(&url("https://example.org/a")).is_ok());
        assert!(check_literal(&url("ftp://example.org/a")).is_err());
        assert!(check_literal(&url("file:///etc/passwd")).is_err());
        assert!(check_literal(&url("javascript:alert(1)")).is_err());
        assert!(
            check_literal(&url("https://user:secret@example.org/a")).is_err(),
            "credentials in an address are a way to hide the host"
        );
        assert!(check_literal(&url("http://192.168.0.1/")).is_err());
    }

    #[test]
    fn exclusion_patterns_match_paths_and_hosts() {
        let rules = Exclusions::parse(&[
            "/private/*".to_string(),
            "*.zip".to_string(),
            "cdn.example.org".to_string(),
            "/private/*".to_string(),
            "  ".to_string(),
            "/".to_string() + &"a".repeat(300),
        ]);
        assert!(rules.matches(&url("https://example.org/private/a/b.mp4")));
        assert!(!rules.matches(&url("https://example.org/public/a.mp4")));
        assert!(rules.matches(&url("https://example.org/files/a.ZIP")));
        assert!(rules.matches(&url("https://cdn.example.org/a.mp4")));
        assert!(!rules.matches(&url("https://example.org/a.mp4")));
        assert!(
            Exclusions::parse(&["/private".to_string()])
                .matches(&url("https://example.org/private/a.mp4")),
            "a path rule leaves out the pages under it"
        );
    }

    #[test]
    fn an_empty_exclusion_list_matches_nothing() {
        let rules = Exclusions::parse(&[]);
        assert!(!rules.matches(&url("https://example.org/anything")));
    }

    #[test]
    fn a_glob_anchors_neither_end() {
        assert!(wildcard("/private/*", "/private/a/b"));
        assert!(!wildcard("/private/*", "/public/private/a"));
        assert!(wildcard("*.mp4", "https://example.org/a.mp4"));
        assert!(wildcard("*", "anything"));
        assert!(wildcard("a*b*c", "axxbyyc"));
        assert!(!wildcard("a*b*c", "axxbyy"));
        assert!(!wildcard("", "x"));
    }

    #[test]
    fn accepted_types_take_the_ticked_kinds_and_the_typed_extensions() {
        let accepted = Accepted::new([FileKind::Video], &["iso".to_string(), ".ZIP".to_string()]);
        let video = Candidate::from_url(&url("https://example.org/a.mp4"), "x").unwrap();
        let archive = Candidate::from_url(&url("https://example.org/a.iso"), "x").unwrap();
        let zip = Candidate::from_url(&url("https://example.org/a.zip"), "x").unwrap();
        let doc = Candidate::from_url(&url("https://example.org/a.pdf"), "x").unwrap();
        assert!(accepted.takes(&video));
        assert!(accepted.takes(&archive));
        assert!(accepted.takes(&zip));
        assert!(!accepted.takes(&doc));

        let none = Accepted::new([], &[]);
        assert!(none.is_empty());
        assert!(!none.takes(&video));
    }
}
