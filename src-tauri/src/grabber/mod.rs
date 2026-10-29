//! The Site Grabber: one site, read on request, and the files it offers.
//!
//! The crawl itself is in [`crawl`]. This module owns the part around it: the
//! one run that may be in flight, what the window can ask (start, stop, the
//! current snapshot), and the hand-over of selected files to the download
//! engine. The hand-over goes through [`crate::actions::start`], the same path
//! the add dialog and the browser bridge use, so a file from here is an
//! ordinary download: same queue, same categories, same destination rules.
//!
//! Nothing is downloaded without the user selecting it. The crawl reads pages
//! and offers links; the window decides what to keep.

mod crawl;
mod extract;
mod policy;
mod robots;
mod window;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use reqwest::Url;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::watch;

use crate::actions::{self, StartOptions};
use crate::bridge::{batch, dest};
use crate::conflict::Choice;
use crate::state::AppState;

use crawl::{Event, Outcome};
use policy::{Candidate, MAX_FILES};

/// The event the window follows while a crawl runs.
pub const EVENT: &str = "grabber://event";

/// The most files one hand-over carries. The window sends the selection in
/// chunks of this size, so one long list does not have to fit in one request.
pub const MAX_HANDOVER: usize = 50;

/// The most log lines kept for the window. A site with a thousand broken links
/// must not turn its log into the application's memory.
const MAX_NOTES: usize = 300;

/// What the window reads when it opens, and again after every event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// `idle`, `running`, `cancelling`, `done` or `cancelled`.
    pub status: String,
    pub url: String,
    pub site: String,
    pub depth: u8,
    pub page_limit: usize,
    pub pages_read: usize,
    pub pages_queued: usize,
    pub files: Vec<Candidate>,
    pub notes: Vec<Note>,
    pub notes_omitted: usize,
    pub duplicates: usize,
    pub restricted: usize,
    pub excluded: usize,
    pub unsupported: usize,
    pub truncated: bool,
    pub message: String,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            status: "idle".to_string(),
            url: String::new(),
            site: String::new(),
            depth: 1,
            page_limit: 0,
            pages_read: 0,
            pages_queued: 0,
            files: Vec::new(),
            notes: Vec::new(),
            notes_omitted: 0,
            duplicates: 0,
            restricted: 0,
            excluded: 0,
            unsupported: 0,
            truncated: false,
            message: String::new(),
        }
    }
}

impl Snapshot {
    fn running(settings: &crawl::Settings) -> Self {
        Self {
            status: "running".to_string(),
            url: settings.start.as_str().to_string(),
            site: settings.hosts.first().cloned().unwrap_or_default(),
            depth: settings.depth,
            page_limit: settings.pages,
            message: "Reading the first page…".to_string(),
            ..Self::default()
        }
    }
}

/// One line of the window's log.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    /// `info`, `blocked` or `error`.
    pub level: String,
    /// The address, without its query.
    pub url: String,
    pub message: String,
}

/// The one crawl the application may have in flight.
#[derive(Default)]
pub struct Grabber {
    session: Mutex<Session>,
}

#[derive(Default)]
struct Session {
    /// The sender that asks the running crawl to stop. `None` when none runs.
    running: Option<watch::Sender<bool>>,
    snapshot: Snapshot,
}

impl Grabber {
    fn lock(&self) -> std::sync::MutexGuard<'_, Session> {
        self.session
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Claim the one run. A second start is refused rather than queued: two
    /// crawls of the same site would halve the politeness of both.
    fn begin(&self, snapshot: Snapshot) -> Result<watch::Receiver<bool>, String> {
        let mut session = self.lock();
        if session.running.is_some() {
            return Err("A crawl is already running. Stop it before starting another.".to_string());
        }
        let (sender, receiver) = watch::channel(false);
        session.running = Some(sender);
        session.snapshot = snapshot;
        Ok(receiver)
    }

    fn cancel(&self) -> Result<(), String> {
        let mut session = self.lock();
        let Some(sender) = session.running.as_ref() else {
            return Err("No crawl is running.".to_string());
        };
        let _ = sender.send(true);
        session.snapshot.status = "cancelling".to_string();
        session.snapshot.message = "Stopping the crawl…".to_string();
        Ok(())
    }

    fn snapshot(&self) -> Snapshot {
        self.lock().snapshot.clone()
    }

    fn record(&self, event: &Event) {
        let mut session = self.lock();
        let snapshot = &mut session.snapshot;
        match event {
            Event::Started {
                url,
                site,
                pages,
                depth,
            } => {
                snapshot.url = url.clone();
                snapshot.site = site.clone();
                snapshot.page_limit = *pages;
                snapshot.depth = *depth;
            }
            Event::Page { .. } => {}
            Event::File { file } => {
                if snapshot.files.len() < MAX_FILES {
                    snapshot.files.push(file.clone());
                }
            }
            Event::Note {
                level,
                url,
                message,
            } => {
                if snapshot.notes.len() >= MAX_NOTES {
                    snapshot.notes_omitted += 1;
                } else {
                    snapshot.notes.push(Note {
                        level: (*level).to_string(),
                        url: url.clone(),
                        message: message.clone(),
                    });
                }
            }
            Event::Progress {
                pages_read,
                pages_queued,
                files: _,
                page_limit,
            } => {
                snapshot.pages_read = *pages_read;
                snapshot.pages_queued = *pages_queued;
                snapshot.page_limit = *page_limit;
            }
            Event::Done {
                status,
                message,
                pages_read,
                pages_queued,
                duplicates,
                restricted,
                excluded,
                unsupported,
                truncated,
                ..
            } => {
                snapshot.status = (*status).to_string();
                snapshot.message = message.clone();
                snapshot.pages_read = *pages_read;
                snapshot.pages_queued = *pages_queued;
                snapshot.duplicates = *duplicates;
                snapshot.restricted = *restricted;
                snapshot.excluded = *excluded;
                snapshot.unsupported = *unsupported;
                snapshot.truncated = *truncated;
            }
        }
    }

    fn finish(&self, outcome: &Outcome) {
        let mut session = self.lock();
        session.running = None;
        session.snapshot.status = outcome.status.to_string();
        session.snapshot.message = outcome.message.clone();
        session.snapshot.pages_read = outcome.stats.pages_read;
        session.snapshot.pages_queued = outcome.stats.pages_queued;
        session.snapshot.duplicates = outcome.stats.duplicates;
        session.snapshot.restricted = outcome.stats.restricted;
        session.snapshot.excluded = outcome.stats.excluded;
        session.snapshot.unsupported = outcome.stats.unsupported;
        session.snapshot.truncated = outcome.stats.truncated;
    }
}

/// The client the grabber sends its requests with.
///
/// Redirects are followed by the crawl itself, one checked hop at a time, so
/// the client must not follow them silently. The timeout covers the body as
/// well, so one slow server cannot hold the crawl open forever.
fn build_client(proxy: Option<&str>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(policy::USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(policy::REQUEST_TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(2);

    if let Some(proxy) = proxy.map(str::trim).filter(|proxy| !proxy.is_empty()) {
        let proxy = reqwest::Proxy::all(proxy)
            .map_err(|error| format!("The proxy address is not usable: {error}"))?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map_err(|error| format!("The grabber's network could not be set up: {error}"))
}

/// Show the Site Grabber window.
#[tauri::command]
pub fn open_grabber_window(app: AppHandle) -> Result<(), String> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(error) = window::show(&handle) {
            idm_core::log::write(
                "warn",
                &format!("the site grabber window did not open: {error}"),
            );
        }
    })
    .map_err(|error| error.to_string())
}

/// Read one site, on request. The answer is immediate; the window follows the
/// `grabber://event` stream.
#[tauri::command]
pub async fn grabber_start(
    app: AppHandle,
    state: State<'_, AppState>,
    grabber: State<'_, Grabber>,
    request: crawl::Request,
) -> Result<(), String> {
    let settings = crawl::Settings::read(&request)?;

    // The start is checked before the window is told a crawl began, so a name
    // that leads next door is a sentence in the form rather than a run that
    // ends without reading anything.
    policy::check_public(&settings.start).await?;

    let client = build_client(state.limits().proxy().as_deref())?;
    let receiver = grabber.begin(Snapshot::running(&settings))?;

    idm_core::log::write(
        "info",
        &format!(
            "the site grabber started on {} (depth {}, at most {} pages)",
            settings.start, settings.depth, settings.pages
        ),
    );

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let sink = handle.clone();
        let mut emit = move |event: Event| {
            sink.state::<Grabber>().record(&event);
            let _ = sink.emit_to(window::WINDOW, EVENT, &event);
        };
        let outcome = crawl::run(settings, client, receiver, &mut emit).await;
        handle.state::<Grabber>().finish(&outcome);
        idm_core::log::write("info", &format!("the site grabber: {}", outcome.message));
    });

    Ok(())
}

/// Stop the crawl that is running.
#[tauri::command]
pub fn grabber_cancel(grabber: State<'_, Grabber>) -> Result<(), String> {
    grabber.cancel()
}

/// The snapshot the window opens on, and the one it can return to after a
/// close and a reopen.
#[tauri::command]
pub fn grabber_state(grabber: State<'_, Grabber>) -> Snapshot {
    grabber.snapshot()
}

/// One file the window selected.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverItem {
    pub url: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub category: String,
}

/// The files the window selected, and where they go.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handover {
    #[serde(default)]
    pub items: Vec<HandoverItem>,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub connections: Option<u32>,
    #[serde(default)]
    pub queue: Option<bool>,
    /// The page the files came from, sent as the referer of each download.
    #[serde(default)]
    pub referer: Option<String>,
}

/// What became of one selected file.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemResult {
    pub url: String,
    pub name: String,
    /// `started`, `queued`, `skipped`, `duplicate`, `invalid` or `refused`.
    pub status: &'static str,
    pub id: Option<u64>,
    pub dest: String,
    pub message: String,
}

/// Start the selected files through the engine, and answer for each one.
///
/// The folder is the one the user picked in the system dialog; it is created
/// when it is not there. A name that is taken becomes `name (2).ext`, and an
/// address the list already holds is reported rather than added twice, which
/// is what the browser hand-over does too.
#[tauri::command]
pub async fn grabber_download(
    app: AppHandle,
    state: State<'_, AppState>,
    request: Handover,
) -> Result<Vec<ItemResult>, String> {
    if request.items.is_empty() {
        return Err("No files were selected.".to_string());
    }
    if request.items.len() > MAX_HANDOVER {
        return Err(format!(
            "Send at most {MAX_HANDOVER} files at a time; the window sends them in chunks."
        ));
    }

    let folder = PathBuf::from(request.folder.trim());
    if folder.as_os_str().is_empty() || !folder.is_absolute() {
        return Err("Choose a folder to save the files to.".to_string());
    }
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("That folder could not be made: {error}"))?;

    let queue = request.queue.unwrap_or(false);
    let connections = request.connections.unwrap_or(8).clamp(1, 32);
    let referer = request
        .referer
        .as_deref()
        .map(str::trim)
        .filter(|referer| !referer.is_empty())
        .and_then(|referer| Url::parse(referer).ok())
        .filter(|url| policy::check_literal(url).is_ok())
        .map(|url| url.to_string());

    let mut known: HashSet<String> = state
        .list()
        .iter()
        .map(|download| canonical(&download.url))
        .collect();
    let mut results = Vec::with_capacity(request.items.len());

    for item in &request.items {
        let url = item.url.trim().to_string();
        let name = dest::sanitize(&item.name)
            .or_else(|| dest::name_from_url(&url))
            .unwrap_or_else(|| "download".to_string());

        let Some(parsed) = Url::parse(&url).ok() else {
            results.push(ItemResult {
                url,
                name,
                status: "invalid",
                id: None,
                dest: String::new(),
                message: "That is not an address a browser could open.".to_string(),
            });
            continue;
        };
        if let Err(problem) = policy::check_literal(&parsed) {
            results.push(ItemResult {
                url,
                name,
                status: "invalid",
                id: None,
                dest: String::new(),
                message: problem,
            });
            continue;
        }
        // The address was checked when it was listed; it is checked again now,
        // because a name may have changed what it points at since. The engine
        // still resolves it once more when it connects.
        if let Err(problem) = policy::check_public(&parsed).await {
            results.push(ItemResult {
                url,
                name,
                status: "refused",
                id: None,
                dest: String::new(),
                message: problem,
            });
            continue;
        }
        if !known.insert(canonical(&url)) {
            results.push(ItemResult {
                url,
                name,
                status: "duplicate",
                id: None,
                dest: String::new(),
                message: "This file is already in the download list.".to_string(),
            });
            continue;
        }

        let target = dest::unique(&folder, &name);
        let started = actions::start(
            &app,
            &state,
            url.clone(),
            target,
            StartOptions {
                connections: Some(connections),
                referer: referer.clone(),
                queue,
                category: batch::clean_category(&item.category),
                // A grabber has no dialog to choose a folder, so the folder
                // its category saves to decides where the file lands.
                use_category_folder: true,
                description: None,
                // A grabber has no dialog to ask about a taken name, so it
                // never replaces a file: the next free name is used.
                conflict: Choice::Rename,
            },
        );

        match started {
            Ok(started) => {
                let dest_name = std::path::Path::new(&started.dest)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| name.clone());
                results.push(ItemResult {
                    url,
                    name: dest_name,
                    status: if queue {
                        "queued"
                    } else if started.skipped {
                        "skipped"
                    } else {
                        "started"
                    },
                    id: started.id,
                    dest: started.dest,
                    message: String::new(),
                });
            }
            Err(refusal) => results.push(ItemResult {
                url,
                name,
                status: "refused",
                id: None,
                dest: refusal.path,
                message: refusal.message,
            }),
        }
    }

    let started = results
        .iter()
        .filter(|result| result.status == "started" || result.status == "queued")
        .count();
    idm_core::log::write(
        "info",
        &format!(
            "the site grabber handed over {started} of {} selected file(s) to {}",
            results.len(),
            folder.display()
        ),
    );
    Ok(results)
}

/// The same address however it was written.
fn canonical(url: &str) -> String {
    Url::parse(url.trim())
        .map(|parsed| parsed.to_string())
        .unwrap_or_else(|_| url.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> Candidate {
        Candidate::from_url(
            &Url::parse("https://example.org/files/a.zip").unwrap(),
            "https://example.org/",
        )
        .expect("a file")
    }

    #[test]
    fn the_snapshot_reads_the_way_the_window_expects() {
        let value = serde_json::to_value(Snapshot::default()).expect("a snapshot");
        assert_eq!(value["status"], "idle");
        assert_eq!(value["pageLimit"], 0);
        assert_eq!(value["notesOmitted"], 0);
        assert!(value.get("page_limit").is_none());
    }

    #[test]
    fn a_fresh_session_is_idle_and_empty() {
        let grabber = Grabber::default();
        let snapshot = grabber.snapshot();
        assert_eq!(snapshot.status, "idle");
        assert!(snapshot.files.is_empty());
        assert!(snapshot.notes.is_empty());
        assert_eq!(snapshot.depth, 1);
    }

    #[test]
    fn only_one_crawl_can_hold_the_session() {
        let grabber = Grabber::default();
        let first = grabber.begin(Snapshot::default());
        assert!(first.is_ok());
        assert!(grabber.begin(Snapshot::default()).is_err());
        assert_eq!(grabber.snapshot().status, "idle");

        grabber.cancel().expect("a running crawl can stop");
        assert_eq!(grabber.snapshot().status, "cancelling");

        grabber.finish(&Outcome {
            status: "cancelled",
            message: "The crawl was stopped.".to_string(),
            stats: crawl::Stats::default(),
        });
        assert_eq!(grabber.snapshot().status, "cancelled");
        assert!(grabber.begin(Snapshot::default()).is_ok());
    }

    #[test]
    fn stopping_without_a_crawl_is_refused() {
        let grabber = Grabber::default();
        assert!(grabber.cancel().is_err());
    }

    #[test]
    fn files_and_notes_are_collected_as_they_arrive() {
        let grabber = Grabber::default();
        grabber.begin(Snapshot::default()).expect("a run");
        grabber.record(&Event::File { file: candidate() });
        grabber.record(&Event::Note {
            level: "blocked",
            url: "example.org/private".to_string(),
            message: "robots.txt said no".to_string(),
        });
        grabber.record(&Event::Progress {
            pages_read: 2,
            pages_queued: 3,
            files: 1,
            page_limit: 50,
        });

        let snapshot = grabber.snapshot();
        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(snapshot.files[0].name, "a.zip");
        assert_eq!(snapshot.notes.len(), 1);
        assert_eq!(snapshot.notes[0].level, "blocked");
        assert_eq!(snapshot.pages_read, 2);
        assert_eq!(snapshot.pages_queued, 3);
    }

    #[test]
    fn the_log_stops_at_its_limit_and_says_how_many_it_left_out() {
        let grabber = Grabber::default();
        grabber.begin(Snapshot::default()).expect("a run");
        for index in 0..(MAX_NOTES + 4) {
            grabber.record(&Event::Note {
                level: "blocked",
                url: format!("example.org/{index}"),
                message: "left out".to_string(),
            });
        }
        let snapshot = grabber.snapshot();
        assert_eq!(snapshot.notes.len(), MAX_NOTES);
        assert_eq!(snapshot.notes_omitted, 4);
    }

    #[test]
    fn the_done_event_carries_the_counts_and_the_end_state() {
        let grabber = Grabber::default();
        grabber.begin(Snapshot::default()).expect("a run");
        grabber.record(&Event::Done {
            status: "done",
            message: "Read 2 pages and found 3 files.".to_string(),
            pages_read: 2,
            pages_queued: 0,
            files: 3,
            duplicates: 4,
            restricted: 5,
            excluded: 6,
            unsupported: 7,
            truncated: true,
        });
        let snapshot = grabber.snapshot();
        assert_eq!(snapshot.status, "done");
        assert_eq!(snapshot.message, "Read 2 pages and found 3 files.");
        assert_eq!(snapshot.duplicates, 4);
        assert_eq!(snapshot.restricted, 5);
        assert_eq!(snapshot.excluded, 6);
        assert_eq!(snapshot.unsupported, 7);
        assert!(snapshot.truncated);
    }

    #[test]
    fn a_handover_reads_the_names_the_window_sends() {
        let request: Handover = serde_json::from_str(
            r#"{
                "items": [{ "url": "https://example.org/a.zip", "name": "a.zip", "category": "Compressed" }],
                "folder": "C:/Users/me/Downloads",
                "connections": 4,
                "queue": true,
                "referer": "https://example.org/files"
            }"#,
        )
        .expect("a hand-over");
        assert_eq!(request.items.len(), 1);
        assert_eq!(request.connections, Some(4));
        assert_eq!(request.queue, Some(true));
        assert_eq!(request.items[0].category, "Compressed");
        assert_eq!(request.folder, "C:/Users/me/Downloads");
    }

    #[test]
    fn a_handover_without_choices_gets_the_defaults() {
        let request: Handover =
            serde_json::from_str(r#"{"items":[{"url":"https://example.org/a.zip"}]}"#)
                .expect("a minimal hand-over");
        assert_eq!(request.folder, "");
        assert_eq!(request.connections, None);
        assert_eq!(request.items[0].category, "");
    }

    #[test]
    fn a_client_is_built_with_and_without_a_proxy() {
        assert!(build_client(None).is_ok());
        assert!(build_client(Some("socks5://127.0.0.1:1080")).is_ok());
        assert!(build_client(Some("not a proxy")).is_err());
    }

    #[test]
    fn two_spellings_of_one_address_are_one_download() {
        assert_eq!(
            canonical("HTTPS://Example.ORG:443/a.zip"),
            canonical("https://example.org/a.zip")
        );
    }
}
