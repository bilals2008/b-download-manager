//! Reading one page and one robots file, with every hop checked.

use futures_util::StreamExt;
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_TYPE, LOCATION};
use reqwest::{Client, StatusCode, Url};
use tokio::sync::watch;

use super::super::policy::{
    self, MAX_PAGE_BYTES, MAX_REDIRECTS, MAX_ROBOTS_BYTES,
};
use super::super::robots::{self, Robots};
use super::request::{check_target, Settings};

const ACCEPT_PAGE: &str = "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1";
const ACCEPT_TEXT: &str = "text/plain,*/*;q=0.1";
const IDENTITY: &str = "identity";

#[derive(Debug)]
pub(super) struct Fetched {
    pub url: Url,
    pub body: String,
}

#[derive(Debug)]
pub(super) enum FetchError {
    Cancelled,
    Refused(String),
    Blocked(String),
    RateLimited(String),
    NotHtml(String),
    TooLarge,
}

impl FetchError {
    pub(super) fn message(self) -> String {
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
pub(super) async fn cancelled(cancel: &mut watch::Receiver<bool>) {
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
pub(super) async fn read_robots(
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
pub(super) async fn fetch_page(
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
