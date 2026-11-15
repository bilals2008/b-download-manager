//! The breadth-first walk that joins the request, the pages and the events.

use std::collections::{HashMap, HashSet, VecDeque};

use reqwest::{Client, Url};
use tokio::sync::watch;

use super::super::extract;
use super::super::policy::{self, Candidate, MAX_FILES, PAGE_DELAY};
use super::super::robots::Robots;
use super::events::{Event, Outcome, Stats};
use super::fetch::{cancelled, fetch_page, read_robots, FetchError};
use super::request::{check_candidate, Settings};

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
