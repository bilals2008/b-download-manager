//! The window's request, read into the addresses and limits the crawl uses.

use reqwest::Url;
use serde::Deserialize;

use super::super::policy::{self, Accepted, Exclusions, FileKind, MAX_DEPTH, MAX_PAGES};

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
pub(super) async fn check_target(url: &Url, settings: &Settings) -> Result<(), String> {
    if let Some(answer) = loopback_allowed(url, settings) {
        return answer;
    }
    policy::check_public(url).await
}

/// The spelling check every offered file goes through. A file is not fetched
/// here, so its name is not resolved: the hand-over resolves it again.
pub(super) fn check_candidate(url: &Url, settings: &Settings) -> Result<(), String> {
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
