//! The crawl: from one page to the files it leads to.
//!
//! The walk is breadth-first and patient. It reads the pages of one site, a
//! page at a time, with a pause between requests, and it stops at the depth,
//! page and file limits the user set. Every page and every redirect goes
//! through the address policy, every page is checked against the site's
//! `robots.txt`, and a cancellation is noticed between chunks, not after the
//! whole request.
//!
//! This module never creates a download. It only offers candidates; the window
//! hands the selected ones to the engine.

use std::collections::{HashMap, HashSet, VecDeque};

use futures_util::StreamExt;
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_TYPE, LOCATION};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use super::extract;
use super::policy::{
    self, Accepted, Candidate, Exclusions, FileKind, MAX_DEPTH, MAX_FILES, MAX_PAGES,
    MAX_PAGE_BYTES, MAX_REDIRECTS, MAX_ROBOTS_BYTES, PAGE_DELAY,
};
use super::robots::{self, Robots};

const ACCEPT_PAGE: &str = "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1";
const ACCEPT_TEXT: &str = "text/plain,*/*;q=0.1";
const IDENTITY: &str = "identity";

/* ---------------------------------------------------------------- request */

/// What the window asks for.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Request {
    pub url: String,
    pub depth: u8,
    pub pages: usize,
    /// The kind tokens the user ticked.
    pub types: Vec<String>,
    /// Extra file extensions typed beside the kinds.
    pub extensions: Vec<String>,
    /// Paths and patterns to leave out.
    pub exclude: Vec<String>,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            url: String::new(),
            depth: 1,
            pages: 50,
            types: default_types(),
            extensions: Vec::new(),
            exclude: Vec::new(),
        }
    }
}

/// The kinds ticked when the window opens: the ones that are usually a file
/// worth saving. Images and unknown types would turn a gallery or a site's
/// assets into a file list, so they are a choice, not a default.
fn default_types() -> Vec<String> {
    [
        FileKind::Video,
        FileKind::Music,
        FileKind::Documents,
        FileKind::Archives,
        FileKind::Programs,
    ]
    .iter()
    .map(|kind| kind.token().to_string())
    .collect()
}

/// The request, read into the addresses and limits the crawl uses.
#[derive(Debug, Clone)]
pub struct Settings {
    pub start: Url,
    pub hosts: Vec<String>,
    pub depth: u8,
    pub pages: usize,
    pub accepted: Accepted,
    pub exclusions: Exclusions,
    /// The integration tests crawl a server on the loopback interface, which
    /// the production policy refuses like any other private address. Only
    /// loopback is let through; every other private destination stays refused.
    #[cfg(test)]
    pub test_allow_local: bool,
}

impl Settings {
    /// Read the window's request, refusing what the crawler cannot do.
    pub fn read(request: &Request) -> Result<Self, String> {
        let text = request.url.trim();
        if text.is_empty() {
            return Err("Enter the address of the page to read.".to_string());
        }
        let mut start = Url::parse(text)
            .map_err(|_| "That is not an address a browser could open.".to_string())?;
        policy::check_literal(&start)?;

        // Starting on a file answers nothing: there is no page to read. The
        // window is for finding files, so it asks for a page.
        if let Some(extension) = policy::extension_of(&start) {
            if policy::kind_of(&extension).is_some() {
                return Err(
                    "That address is a file. Start with the page that links to it.".to_string(),
                );
            }
        }
        start.set_fragment(None);

        let kinds: Vec<FileKind> = request
            .types
            .iter()
            .filter_map(|token| FileKind::from_token(token))
            .collect();
        let accepted = Accepted::new(kinds, &request.extensions);
        if accepted.is_empty() {
            return Err("Tick at least one file type, or name an extension.".to_string());
        }

        let hosts = policy::site_hosts(&start);
        Ok(Self {
            start,
            hosts,
            depth: request.depth.clamp(1, MAX_DEPTH),
            pages: request.pages.clamp(1, MAX_PAGES),
            accepted,
            exclusions: Exclusions::parse(&request.exclude),
            #[cfg(test)]
            test_allow_local: false,
        })
    }
}

/// The address check every page and redirect goes through.
async fn check_target(url: &Url, settings: &Settings) -> Result<(), String> {
    if let Some(answer) = loopback_allowed(url, settings) {
        return answer;
    }
    policy::check_public(url).await
}

/// The spelling check every offered file goes through. A file is not fetched
/// here, so its name is not resolved: the hand-over resolves it again.
fn check_candidate(url: &Url, settings: &Settings) -> Result<(), String> {
    if let Some(answer) = loopback_allowed(url, settings) {
        return answer;
    }
    policy::check_literal(url)
}

/// The test policy's one exception: a server the test itself runs on loopback.
///
/// Every other address still takes the production path, so a test can assert
/// that a metadata endpoint or a private network is refused.
fn loopback_allowed(url: &Url, settings: &Settings) -> Option<Result<(), String>> {
    #[cfg(test)]
    if settings.test_allow_local {
        if let Err(problem) = policy::check_web(url) {
            return Some(Err(problem));
        }
        let loopback = url
            .host_str()
            .and_then(|host| host.parse::<std::net::IpAddr>().ok())
            .is_some_and(|address| address.is_loopback());
        if loopback {
            return Some(Ok(()));
        }
    }
    let _ = (url, settings);
    None
}

/* ------------------------------------------------------------------ events */

/// One step of the crawl, for the window.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    Started {
        url: String,
        site: String,
        pages: usize,
        depth: u8,
    },
    /// One page was read.
    Page {
        url: String,
        depth: u8,
        links: usize,
        files: usize,
    },
    /// One file was found.
    File { file: Candidate },
    /// Something was left out, with the sentence the window shows.
    Note {
        level: &'static str,
        url: String,
        message: String,
    },
    Progress {
        pages_read: usize,
        pages_queued: usize,
        files: usize,
        page_limit: usize,
    },
    Done {
        status: &'static str,
        message: String,
        pages_read: usize,
        pages_queued: usize,
        files: usize,
        duplicates: usize,
        restricted: usize,
        excluded: usize,
        unsupported: usize,
        truncated: bool,
    },
}

/// What the crawl counted, for the final line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub pages_read: usize,
    pub pages_queued: usize,
    pub files: usize,
    pub duplicates: usize,
    pub restricted: usize,
    pub excluded: usize,
    pub unsupported: usize,
    pub truncated: bool,
}

/// How the run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// `done`, `cancelled` or `failed`.
    pub status: &'static str,
    pub message: String,
    pub stats: Stats,
}

impl Outcome {
    fn cancelled(stats: Stats) -> Self {
        Self {
            status: "cancelled",
            message: "The crawl was stopped.".to_string(),
            stats,
        }
    }
}

/* -------------------------------------------------------------------- crawl */

/// Walk the site and report what is found.
///
/// `emit` is the one place a result leaves the crawl: the application turns it
/// into an event for the window and a snapshot for the next open, and the
/// tests read the same stream. The stream always ends with [`Event::Done`],
/// however the walk ended.
pub async fn run<F>(
    settings: Settings,
    client: Client,
    cancel: watch::Receiver<bool>,
    emit: &mut F,
) -> Outcome
where
    F: FnMut(Event) + ?Sized,
{
    let outcome = walk(settings, client, cancel, emit).await;
    emit(Event::Done {
        status: outcome.status,
        message: outcome.message.clone(),
        pages_read: outcome.stats.pages_read,
        pages_queued: outcome.stats.pages_queued,
        files: outcome.stats.files,
        duplicates: outcome.stats.duplicates,
        restricted: outcome.stats.restricted,
        excluded: outcome.stats.excluded,
        unsupported: outcome.stats.unsupported,
        truncated: outcome.stats.truncated,
    });
    outcome
}

async fn walk<F>(
    settings: Settings,
    client: Client,
    mut cancel: watch::Receiver<bool>,
    emit: &mut F,
) -> Outcome
where
    F: FnMut(Event) + ?Sized,
{
    let mut stats = Stats::default();
    emit(Event::Started {
        url: settings.start.as_str().to_string(),
        site: settings.hosts.first().cloned().unwrap_or_default(),
        pages: settings.pages,
        depth: settings.depth,
    });

    let mut queue: VecDeque<(Url, u8)> = VecDeque::new();
    let mut seen_pages: HashSet<String> = HashSet::new();
    let mut read_pages: HashSet<String> = HashSet::new();
    let mut seen_files: HashSet<String> = HashSet::new();
    let mut known_robots: HashMap<String, Result<Robots, String>> = HashMap::new();
    let mut delay = PAGE_DELAY;
    let mut first = true;

    seen_pages.insert(settings.start.as_str().to_string());
    queue.push_back((settings.start.clone(), 1));

    while let Some((page_url, depth)) = queue.pop_front() {
        if *cancel.borrow() {
            return Outcome::cancelled(stats);
        }
        if stats.pages_read >= settings.pages {
            stats.truncated = true;
            break;
        }
        // A redirect may have already read this address under another name;
        // reading it again would spend a request and a page of the budget.
        if !read_pages.insert(page_url.as_str().to_string()) {
            continue;
        }

        if !first {
            tokio::select! {
                biased;
                _ = cancelled(&mut cancel) => return Outcome::cancelled(stats),
                _ = tokio::time::sleep(delay) => {}
            }
        }
        first = false;

        let origin = page_url.origin().ascii_serialization();
        if !known_robots.contains_key(&origin) {
            let rules = read_robots(&client, &origin, &settings, &mut cancel).await;
            if let Err(reason) = &rules {
                emit(Event::Note {
                    level: "blocked",
                    url: origin.clone(),
                    message: reason.clone(),
                });
            }
            if let Ok(rules) = &rules {
                if let Some(asked) = rules.delay() {
                    delay = asked.max(PAGE_DELAY);
                }
            }
            known_robots.insert(origin.clone(), rules);
        }
        match known_robots.get(&origin) {
            Some(Ok(rules)) => {
                if !rules.allows(&path_of(&page_url)) {
                    stats.restricted += 1;
                    emit(Event::Note {
                        level: "blocked",
                        url: policy::short(&page_url),
                        message: "robots.txt of this site asks crawlers not to read this page."
                            .to_string(),
                    });
                    continue;
                }
            }
            Some(Err(_)) => {
                stats.restricted += 1;
                continue;
            }
            None => continue,
        }

        let fetched = match fetch_page(&client, &page_url, &settings, &mut cancel).await {
            Ok(page) => page,
            Err(FetchError::Cancelled) => return Outcome::cancelled(stats),
            Err(FetchError::RateLimited(message)) => {
                stats.restricted += 1;
                emit(Event::Note {
                    level: "blocked",
                    url: policy::short(&page_url),
                    message: message.clone(),
                });
                let mut done = stats;
                done.pages_queued = queue.len();
                return Outcome {
                    status: "done",
                    message: format!("The site asked the crawler to wait: {message}"),
                    stats: done,
                };
            }
            Err(error) => {
                stats.restricted += 1;
                emit(Event::Note {
                    level: "blocked",
                    url: policy::short(&page_url),
                    message: error.message(),
                });
                continue;
            }
        };

        // A redirect may land on the `www.` twin, whose robots file is a
        // different file. The page's own rules were checked above; the rules
        // of the address that finally answered are checked here.
        let final_origin = fetched.url.origin().ascii_serialization();
        if final_origin != origin {
            if !known_robots.contains_key(&final_origin) {
                let rules = read_robots(&client, &final_origin, &settings, &mut cancel).await;
                if let Err(reason) = &rules {
                    emit(Event::Note {
                        level: "blocked",
                        url: final_origin.clone(),
                        message: reason.clone(),
                    });
                }
                known_robots.insert(final_origin.clone(), rules);
            }
            let allowed = known_robots.get(&final_origin).is_some_and(|rules| {
                rules
                    .as_ref()
                    .is_ok_and(|rules| rules.allows(&path_of(&fetched.url)))
            });
            if !allowed {
                stats.restricted += 1;
                emit(Event::Note {
                    level: "blocked",
                    url: policy::short(&fetched.url),
                    message: "robots.txt of this site asks crawlers not to read this page."
                        .to_string(),
                });
                continue;
            }
        }

        read_pages.insert(fetched.url.as_str().to_string());
        stats.pages_read += 1;
        let found = extract::links(&fetched.body, &fetched.url);
        let links = found.len();
        let mut new_files = 0usize;

        for link in found {
            if settings.exclusions.matches(&link.url) {
                stats.excluded += 1;
                continue;
            }

            if let Some(candidate) = Candidate::from_url(&link.url, fetched.url.as_str()) {
                if !settings.accepted.takes(&candidate) {
                    stats.unsupported += 1;
                    continue;
                }
                // A file on an address the crawler may not request is not
                // offered: the list would only be a way around the policy.
                if check_candidate(&link.url, &settings).is_err() {
                    stats.restricted += 1;
                    emit(Event::Note {
                        level: "blocked",
                        url: policy::short(&link.url),
                        message: "That address is on this computer or a private network."
                            .to_string(),
                    });
                    continue;
                }
                if seen_files.len() >= MAX_FILES {
                    stats.truncated = true;
                    continue;
                }
                if seen_files.insert(candidate.url.clone()) {
                    stats.files = seen_files.len();
                    new_files += 1;
                    emit(Event::File { file: candidate });
                } else {
                    stats.duplicates += 1;
                }
                continue;
            }

            // A stylesheet, a script or a manifest is not a page to read.
            if !policy::is_page_document(&link.url) {
                stats.unsupported += 1;
                continue;
            }

            // A page link: followed only when it is an anchor, on the same
            // site, and the depth limit still leaves room.
            if !link.crawl || depth >= settings.depth {
                continue;
            }
            if !policy::same_site(&link.url, &settings.hosts) {
                continue;
            }
            let key = link.url.as_str().to_string();
            if !seen_pages.insert(key) {
                stats.duplicates += 1;
                continue;
            }
            if queue.len() + stats.pages_read >= settings.pages {
                stats.truncated = true;
                continue;
            }
            queue.push_back((link.url, depth + 1));
        }

        stats.pages_queued = queue.len();
        emit(Event::Page {
            url: policy::short(&fetched.url),
            depth,
            links,
            files: new_files,
        });
        emit(Event::Progress {
            pages_read: stats.pages_read,
            pages_queued: stats.pages_queued,
            files: stats.files,
            page_limit: settings.pages,
        });
    }

    stats.pages_queued = queue.len();
    let message = if stats.pages_read == 0 {
        "No page could be read. The log says why.".to_string()
    } else if stats.files == 0 {
        "The page was read, but no file of the chosen types was found.".to_string()
    } else {
        format!(
            "Read {} and found {}.",
            pages(stats.pages_read),
            files(stats.files)
        )
    };
    Outcome {
        status: "done",
        message,
        stats,
    }
}

/// The part of an address robots.txt rules are matched against.
fn path_of(url: &Url) -> String {
    match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_string(),
    }
}

fn pages(count: usize) -> String {
    if count == 1 {
        "1 page".to_string()
    } else {
        format!("{count} pages")
    }
}

fn files(count: usize) -> String {
    if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    }
}

/* ----------------------------------------------------------------- fetching */

#[derive(Debug)]
struct Fetched {
    url: Url,
    body: String,
}

#[derive(Debug)]
enum FetchError {
    Cancelled,
    Refused(String),
    Blocked(String),
    RateLimited(String),
    NotHtml(String),
    TooLarge,
}

impl FetchError {
    fn message(self) -> String {
        match self {
            FetchError::Cancelled => "The crawl was stopped.".to_string(),
            FetchError::Refused(message)
            | FetchError::Blocked(message)
            | FetchError::RateLimited(message)
            | FetchError::NotHtml(message) => message,
            FetchError::TooLarge => "That page is too large to read as a page.".to_string(),
        }
    }
}

/// Wait until the crawl is asked to stop.
async fn cancelled(cancel: &mut watch::Receiver<bool>) {
    if *cancel.borrow() {
        return;
    }
    while cancel.changed().await.is_ok() {
        if *cancel.borrow() {
            return;
        }
    }
}

/// Read the `robots.txt` of one origin.
///
/// A site that answers `404` has no file, and the standard says its resources
/// may then be read. An answer the crawler cannot interpret — a server error,
/// a rate limit — blocks the site instead: guessing in favour of reading is
/// the wrong guess when the site just said no.
async fn read_robots(
    client: &Client,
    origin: &str,
    settings: &Settings,
    cancel: &mut watch::Receiver<bool>,
) -> Result<Robots, String> {
    let url = Url::parse(&format!("{origin}/robots.txt"))
        .map_err(|_| "The site's robots.txt address is not usable.".to_string())?;
    check_target(&url, settings).await?;

    let request = client
        .get(url.clone())
        .header(ACCEPT, ACCEPT_TEXT)
        .header(ACCEPT_ENCODING, IDENTITY);
    let response = tokio::select! {
        biased;
        _ = cancelled(cancel) => return Err("The crawl was stopped.".to_string()),
        result = request.send() => match result {
            Ok(response) => response,
            Err(_) => return Err("The site's robots.txt could not be reached.".to_string()),
        },
    };

    let status = response.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Err("The site is asking crawlers to slow down.".to_string());
    }
    if status.is_server_error() {
        return Err(format!(
            "The site's robots.txt had a problem ({status}), so it cannot say what is allowed."
        ));
    }
    if status.is_redirection() {
        // A redirect is a robots file the crawler cannot read. Reading the
        // site anyway would let a site redirect its rules out of the way.
        return Err("The site's robots.txt redirected, so it cannot be read.".to_string());
    }
    if !status.is_success() {
        // 401, 403, 404 and every other 4xx mean "no usable robots.txt"; the
        // standard lets a crawler read the site in that case.
        return Ok(Robots::default());
    }

    let body = match read_body(response, MAX_ROBOTS_BYTES, cancel).await {
        Ok(body) => body,
        Err(FetchError::Cancelled) => return Err("The crawl was stopped.".to_string()),
        Err(error) => return Err(error.message()),
    };
    Ok(robots::parse(&body, robots::agent()))
}

/// Fetch one page, following its redirects one checked hop at a time.
async fn fetch_page(
    client: &Client,
    url: &Url,
    settings: &Settings,
    cancel: &mut watch::Receiver<bool>,
) -> Result<Fetched, FetchError> {
    let mut current = url.clone();
    for hop in 0..=MAX_REDIRECTS {
        check_target(&current, settings)
            .await
            .map_err(FetchError::Blocked)?;

        let request = client
            .get(current.clone())
            .header(ACCEPT, ACCEPT_PAGE)
            .header(ACCEPT_ENCODING, IDENTITY);
        let response = tokio::select! {
            biased;
            _ = cancelled(cancel) => return Err(FetchError::Cancelled),
            result = request.send() => result.map_err(|error| {
                FetchError::Refused(format!("The page could not be reached: {error}"))
            })?,
        };

        let status = response.status();
        if status.is_redirection() {
            let Some(location) = response.headers().get(LOCATION) else {
                return Err(FetchError::Refused(format!(
                    "The site answered {status} without saying where to go."
                )));
            };
            let location = location.to_str().unwrap_or("").trim();
            let next = current.join(location).map_err(|_| {
                FetchError::Refused("The site redirected to an address that cannot be read.".into())
            })?;
            if hop == MAX_REDIRECTS {
                return Err(FetchError::Refused(
                    "The page redirected too many times.".to_string(),
                ));
            }
            if !policy::same_site(&next, &settings.hosts) {
                return Err(FetchError::Refused(
                    "The page redirected away from the site, so it was not followed.".to_string(),
                ));
            }
            current = next;
            continue;
        }

        if !status.is_success() {
            return Err(match status.as_u16() {
                401 | 403 => FetchError::Blocked(
                    "The page asked for a sign-in or refused the crawler.".to_string(),
                ),
                404 | 410 => FetchError::Refused("The page is not there any more.".to_string()),
                429 => FetchError::RateLimited("The site is asking crawlers to wait.".to_string()),
                code if (500..600).contains(&code) => {
                    FetchError::Refused(format!("The site had a problem ({status})."))
                }
                _ => FetchError::Refused(format!("The site answered {status}.")),
            });
        }

        if let Some(type_name) = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
        {
            if !type_name.to_ascii_lowercase().contains("html") {
                return Err(FetchError::NotHtml(format!(
                    "That address is not a web page ({type_name})."
                )));
            }
        }
        if let Some(encoding) = response
            .headers()
            .get(CONTENT_ENCODING)
            .and_then(|value| value.to_str().ok())
        {
            if !encoding.eq_ignore_ascii_case("identity") {
                return Err(FetchError::Refused(
                    "The page arrived compressed, so it could not be read.".to_string(),
                ));
            }
        }

        let body = read_body(response, MAX_PAGE_BYTES, cancel).await?;
        return Ok(Fetched { url: current, body });
    }

    Err(FetchError::Refused(
        "The page redirected too many times.".to_string(),
    ))
}

/// Read a response body, up to a limit, stopping as soon as the crawl is.
async fn read_body(
    response: reqwest::Response,
    limit: usize,
    cancel: &mut watch::Receiver<bool>,
) -> Result<String, FetchError> {
    let mut stream = response.bytes_stream();
    let mut body: Vec<u8> = Vec::new();
    loop {
        let next = tokio::select! {
            biased;
            _ = cancelled(cancel) => return Err(FetchError::Cancelled),
            chunk = stream.next() => chunk,
        };
        match next {
            Some(Ok(chunk)) => {
                if body.len() + chunk.len() > limit {
                    return Err(FetchError::TooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Some(Err(error)) => {
                return Err(FetchError::Refused(format!(
                    "The page stopped arriving: {error}"
                )));
            }
            None => break,
        }
    }
    Ok(String::from_utf8_lossy(&body).to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    fn request(url: &str) -> Request {
        Request {
            url: url.to_string(),
            ..Request::default()
        }
    }

    #[test]
    fn the_window_defaults_are_the_safe_ones() {
        let request = Request::default();
        assert_eq!(request.depth, 1, "one page, unless the user asks for more");
        assert_eq!(request.pages, 50);
        assert_eq!(
            request.types,
            vec!["video", "music", "documents", "archives", "programs"]
        );
        assert!(!request.types.iter().any(|kind| kind == "images"));
    }

    #[test]
    fn a_request_reads_its_limits_and_clamps_them() {
        let mut request = request("https://example.org/a/b");
        request.depth = 99;
        request.pages = 10_000;
        let settings = Settings::read(&request).expect("a crawl");
        assert_eq!(settings.depth, MAX_DEPTH);
        assert_eq!(settings.pages, MAX_PAGES);
        assert_eq!(settings.hosts, vec!["example.org", "www.example.org"]);
        assert_eq!(settings.start.path(), "/a/b");

        request.depth = 0;
        request.pages = 0;
        let settings = Settings::read(&request).expect("a crawl");
        assert_eq!(settings.depth, 1);
        assert_eq!(settings.pages, 1);
    }

    #[test]
    fn the_start_must_be_a_public_page() {
        assert!(Settings::read(&request("")).is_err());
        assert!(Settings::read(&request("not an address")).is_err());
        assert!(Settings::read(&request("ftp://example.org/a.zip")).is_err());
        assert!(Settings::read(&request("http://127.0.0.1/a")).is_err());
        assert!(Settings::read(&request("http://localhost/a")).is_err());
        assert!(Settings::read(&request("http://intranet/a")).is_err());
        assert!(
            Settings::read(&request("https://example.org/film.mp4")).is_err(),
            "a file is not a page to read"
        );
        assert!(Settings::read(&request("https://example.org/index.html")).is_ok());
    }

    #[test]
    fn a_request_without_a_file_type_is_refused() {
        let mut request = request("https://example.org/");
        request.types.clear();
        request.extensions.clear();
        assert!(Settings::read(&request).is_err());

        request.extensions = vec!["jpeg".to_string()];
        assert!(Settings::read(&request).is_ok());
    }

    #[test]
    fn the_fragment_of_the_start_is_not_part_of_the_crawl() {
        let settings =
            Settings::read(&request("https://example.org/docs#section")).expect("a crawl");
        assert_eq!(settings.start.as_str(), "https://example.org/docs");
    }

    #[test]
    fn an_event_reads_the_way_the_window_expects() {
        let progress = serde_json::to_value(Event::Progress {
            pages_read: 2,
            pages_queued: 3,
            files: 4,
            page_limit: 50,
        })
        .expect("an event");
        assert_eq!(progress["kind"], "progress");
        assert_eq!(progress["pagesRead"], 2);
        assert_eq!(progress["pagesQueued"], 3);
        assert_eq!(progress["pageLimit"], 50);
        assert!(progress.get("pages_read").is_none());

        let file = Candidate::from_url(
            &Url::parse("https://example.org/files/a.zip").unwrap(),
            "https://example.org/",
        )
        .expect("a file");
        let found = serde_json::to_value(Event::File { file }).expect("an event");
        assert_eq!(found["kind"], "file");
        assert_eq!(found["file"]["name"], "a.zip");
        assert_eq!(found["file"]["category"], "Compressed");
        assert_eq!(found["file"]["kind"], "archives");
        assert!(found["file"].get("page").is_some());
    }

    #[test]
    fn the_notes_carry_a_readable_address() {
        let url = Url::parse("https://example.org/files/a.zip?token=secret").unwrap();
        let line = policy::short(&url);
        assert_eq!(line, "example.org/files/a.zip");
        assert!(!line.contains("secret"));
    }

    /* ------------------------------------------------- a server on loopback */

    #[derive(Clone)]
    struct Reply {
        status: u16,
        location: Option<String>,
        body: String,
        delay: Duration,
    }

    impl Reply {
        fn html(body: &str) -> Self {
            Self {
                status: 200,
                location: None,
                body: body.to_string(),
                delay: Duration::ZERO,
            }
        }

        fn redirect(to: &str) -> Self {
            Self {
                status: 302,
                location: Some(to.to_string()),
                body: String::new(),
                delay: Duration::ZERO,
            }
        }

        fn slow(milliseconds: u64) -> Self {
            Self {
                status: 200,
                location: None,
                body: "<html></html>".to_string(),
                delay: Duration::from_millis(milliseconds),
            }
        }
    }

    /// A tiny HTTP server for the crawl to read.
    ///
    /// It answers one request per connection, which is all a crawl needs, and
    /// remembers every path it was asked for. A path that was not registered
    /// answers 404, which is what a site without robots.txt does.
    async fn serve(routes: Vec<(&str, Reply)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("a free port");
        let port = listener.local_addr().expect("the port").port();
        let routes: Arc<HashMap<String, Reply>> = Arc::new(
            routes
                .into_iter()
                .map(|(path, reply)| (path.to_string(), reply))
                .collect(),
        );
        let hits: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log = hits.clone();

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    continue;
                };
                let routes = routes.clone();
                let log = log.clone();
                tokio::spawn(async move {
                    let mut buffer = vec![0u8; 8_192];
                    let read = socket.read(&mut buffer).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                    let target = request.split_whitespace().nth(1).unwrap_or("/").to_string();
                    log.lock().unwrap().push(target.clone());
                    let path = target.split('?').next().unwrap_or("/").to_string();
                    let reply = routes.get(&path).cloned().unwrap_or(Reply {
                        status: 404,
                        location: None,
                        body: String::new(),
                        delay: Duration::ZERO,
                    });
                    if reply.delay > Duration::ZERO {
                        tokio::time::sleep(reply.delay).await;
                    }
                    let reason = match reply.status {
                        200 => "OK",
                        302 => "Found",
                        404 => "Not Found",
                        _ => "Error",
                    };
                    let mut head = format!(
                        "HTTP/1.1 {} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n",
                        reply.status,
                        reply.body.len()
                    );
                    if let Some(location) = reply.location {
                        head.push_str(&format!("Location: {location}\r\n"));
                    }
                    head.push_str("\r\n");
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(reply.body.as_bytes()).await;
                });
            }
        });

        (format!("http://127.0.0.1:{port}"), hits)
    }

    fn client() -> Client {
        Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .expect("a client")
    }

    /// The settings a test crawls with.
    ///
    /// A loopback address is refused by [`Settings::read`], which is the point
    /// of the production policy, so the test builds the same settings around
    /// its own server and switches on the loopback allowance.
    fn crawl_settings(
        start: &str,
        depth: u8,
        pages: usize,
        types: &[&str],
        exclude: &[&str],
    ) -> Settings {
        let start = Url::parse(start).expect("a test address");
        let kinds = types.iter().filter_map(|kind| FileKind::from_token(kind));
        Settings {
            hosts: policy::site_hosts(&start),
            start,
            depth: depth.clamp(1, MAX_DEPTH),
            pages: pages.clamp(1, MAX_PAGES),
            accepted: Accepted::new(kinds, &[]),
            exclusions: Exclusions::parse(
                &exclude
                    .iter()
                    .map(|rule| (*rule).to_string())
                    .collect::<Vec<_>>(),
            ),
            test_allow_local: true,
        }
    }

    async fn crawl(
        start: &str,
        depth: u8,
        pages: usize,
        types: &[&str],
        exclude: &[&str],
    ) -> (Outcome, Vec<Event>) {
        let settings = crawl_settings(start, depth, pages, types, exclude);
        let (_sender, receiver) = watch::channel(false);
        let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let mut emit = move |event: Event| sink.lock().unwrap().push(event);
        let outcome = run(settings, client(), receiver, &mut emit).await;
        let events = events.lock().unwrap().clone();
        (outcome, events)
    }

    fn files_of(events: &[Event]) -> Vec<String> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::File { file } => Some(file.name.clone()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn a_site_is_read_once_and_only_as_deep_as_asked() {
        let (base, hits) = serve(vec![
            (
                "/",
                Reply::html(
                    r#"<a href="/a.html">A</a><a href="/a.html">A again</a>
                       <a href="/files/one.zip">1</a><a href="/files/one.zip#again">1 again</a>
                       <a href="/private/skip.zip">skip</a>
                       <a href="/files/two.pdf">2</a>"#,
                ),
            ),
            (
                "/a.html",
                Reply::html(
                    r#"<a href="/b.html">B</a><a href="/files/two.pdf">2</a>
                       <a href="/deep.rar">r</a>"#,
                ),
            ),
            (
                "/b.html",
                Reply::html(r#"<a href="/files/late.zip">late</a>"#),
            ),
            ("/files/one.zip", Reply::html("not read")),
        ])
        .await;

        let (outcome, events) =
            crawl(&base, 2, 10, &["documents", "archives"], &["/private"]).await;
        assert_eq!(outcome.status, "done");
        assert_eq!(outcome.stats.pages_read, 2);
        assert_eq!(
            files_of(&events),
            vec!["one.zip", "two.pdf", "deep.rar"],
            "each address once, and only from the pages that were read"
        );
        assert_eq!(
            outcome.stats.duplicates, 1,
            "two.pdf is linked from both pages; the page reader already folds the repeats on one page"
        );
        assert!(
            outcome.stats.excluded >= 1,
            "the excluded path was left out"
        );

        let hits = hits.lock().unwrap().clone();
        assert!(hits.contains(&"/".to_string()));
        assert!(hits.contains(&"/a.html".to_string()));
        assert!(
            !hits.contains(&"/b.html".to_string()),
            "depth 2 reads the start page and the pages it links to, no further: {hits:?}"
        );
        assert!(
            !hits.iter().any(|hit| hit.contains("one.zip")),
            "the crawl offers files, it does not fetch them"
        );
    }

    #[tokio::test]
    async fn the_page_limit_stops_the_walk_and_says_so() {
        let mut home = String::new();
        for index in 0..10 {
            home.push_str(&format!(r#"<a href="/p{index}.html">p</a>"#));
        }
        let pages: Vec<String> = (0..10)
            .map(|index| format!(r#"<a href="/p{index}.html">p</a>"#))
            .collect();
        let paths: Vec<String> = (0..10).map(|index| format!("/p{index}.html")).collect();
        let mut routes: Vec<(&str, Reply)> = vec![("/", Reply::html(&home))];
        for (path, body) in paths.iter().zip(pages.iter()) {
            routes.push((path.as_str(), Reply::html(body)));
        }
        let (base, _hits) = serve(routes).await;

        let (outcome, _events) = crawl(&base, 3, 3, &["documents"], &[]).await;
        assert_eq!(outcome.stats.pages_read, 3);
        assert!(
            outcome.stats.truncated,
            "the limit was reached, so the site is not finished"
        );
    }

    #[tokio::test]
    async fn a_page_robots_txt_forbids_is_not_read() {
        let (base, hits) = serve(vec![
            (
                "/",
                Reply::html(r#"<a href="/private/page.html">p</a><a href="/files/a.zip">a</a>"#),
            ),
            (
                "/private/page.html",
                Reply::html(r#"<a href="/files/b.zip">b</a>"#),
            ),
            (
                "/robots.txt",
                Reply::html("User-agent: *\nDisallow: /private\n"),
            ),
        ])
        .await;

        let (outcome, events) = crawl(&base, 2, 10, &["archives"], &[]).await;
        assert_eq!(outcome.stats.pages_read, 1);
        assert_eq!(files_of(&events), vec!["a.zip"]);
        assert!(outcome.stats.restricted >= 1);
        assert!(
            events.iter().any(|event| matches!(
                event,
                Event::Note { message, .. } if message.contains("robots.txt")
            )),
            "the refusal is explained in the log"
        );
        let hits = hits.lock().unwrap().clone();
        assert!(
            !hits.contains(&"/private/page.html".to_string()),
            "a disallowed page is never requested: {hits:?}"
        );
    }

    #[tokio::test]
    async fn a_redirect_on_the_site_is_followed_and_rechecked() {
        let (base, hits) = serve(vec![
            ("/go", Reply::redirect("/landing.html")),
            (
                "/landing.html",
                Reply::html(r#"<a href="/files/a.zip">a</a>"#),
            ),
        ])
        .await;

        let (outcome, events) = crawl(&format!("{base}/go"), 1, 5, &["archives"], &[]).await;
        assert_eq!(outcome.stats.pages_read, 1);
        assert_eq!(files_of(&events), vec!["a.zip"]);
        let hits = hits.lock().unwrap().clone();
        assert!(hits.contains(&"/landing.html".to_string()));
    }

    #[tokio::test]
    async fn a_redirect_away_from_the_site_is_refused() {
        let (base, _hits) = serve(vec![(
            "/go",
            Reply::redirect("http://169.254.169.254/latest/meta-data/"),
        )])
        .await;

        let (outcome, events) = crawl(&format!("{base}/go"), 1, 5, &["documents"], &[]).await;
        assert_eq!(outcome.stats.pages_read, 0);
        assert!(outcome.stats.restricted >= 1);
        let notes: Vec<String> = events
            .iter()
            .filter_map(|event| match event {
                Event::Note { message, .. } => Some(message.clone()),
                _ => None,
            })
            .collect();
        assert!(
            notes.iter().any(|note| note.contains("redirected away")),
            "the redirect was stopped at the site boundary: {notes:?}"
        );
    }

    #[tokio::test]
    async fn a_private_address_is_still_refused_when_the_test_policy_is_on() {
        let request = Request {
            url: "https://example.org/".to_string(),
            ..Request::default()
        };
        let mut settings = Settings::read(&request).expect("settings");
        settings.test_allow_local = true;
        let metadata = Url::parse("http://169.254.169.254/latest/meta-data/").expect("an address");
        assert!(
            check_target(&metadata, &settings).await.is_err(),
            "a cloud metadata endpoint is never crawled"
        );
        let local = Url::parse("http://127.0.0.1:9/").expect("an address");
        assert!(check_target(&local, &settings).await.is_ok());
    }

    #[tokio::test]
    async fn a_crawl_stops_when_it_is_cancelled() {
        let (base, _hits) = serve(vec![("/", Reply::slow(500))]).await;
        let settings = crawl_settings(&base, 1, 5, &["documents"], &[]);
        let (sender, receiver) = watch::channel(false);
        let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();

        let task = tokio::spawn(async move {
            let mut emit = move |event: Event| sink.lock().unwrap().push(event);
            run(settings, client(), receiver, &mut emit).await
        });

        tokio::time::sleep(Duration::from_millis(80)).await;
        sender.send(true).expect("the crawl is listening");
        let outcome = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("the crawl stops without waiting for the page")
            .expect("the task finished");
        assert_eq!(outcome.status, "cancelled");
        assert_eq!(outcome.stats.pages_read, 0);

        let events = events.lock().unwrap().clone();
        assert!(matches!(
            events.last(),
            Some(Event::Done {
                status: "cancelled",
                ..
            })
        ));
    }
}
