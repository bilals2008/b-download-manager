//! The download engine — a port of the mechanics observed in the reference
//! binary.
//!
//! What the reverse engineering established (the record is in
//! `planning/02-rea-plan.md` and `planning/09-remaining-work.md`):
//!
//! * The reference download manager speaks HTTP over **raw WinSock sockets**
//!   (`WS2_32.DLL::socket`, `recv`, `send`, `ioctlsocket`), not
//!   WinHTTP/WinInet, and it terminates TLS itself with OpenSSL. Every request
//!   carries a `Range: bytes=<start>-<end>`
//!   header and `Accept-Encoding: identity` so that byte offsets stay aligned
//!   with file offsets.
//! * The file is split into **chunks** (`ChunksList` / `ChunksListItem`), each
//!   owning a contiguous byte range and each writing into its own temporary
//!   part file. The list is persisted so a download can be resumed.
//! * A connection that dies has its chunk **reassigned** ("The file chunk %d
//!   is reassigned to connection %d"), and new connections are **added on the
//!   fly** ("Add alternative connection (%ld) for connection %ld").
//! * The finished file is produced by **joining the parts** ("Assembling all
//!   downloaded portions into one file...") and renaming it into place.
//!
//! The Rust port keeps those mechanics and swaps the Win32 primitives for
//! `tokio` + `reqwest`.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex as AsyncMutex};

use crate::disk;
use crate::http;
use crate::limits::{Limits, SpeedLimiter};
use crate::log;
use crate::segmenter::{self, Segment, SegmentStatus, MIN_SPLIT, UNKNOWN_END};
use crate::transfer::{
    FailureKind, Probe, Source, TransferError, CHANGED_PREFIX, RANGE_IGNORED_MESSAGE,
};
use crate::verify;

/// Default `MaxConnectionsNumber` of the reference download manager, read
/// from `HKCU\Software\DownloadManager\MCN`. The observed default was 8.
pub const DEFAULT_CONNECTIONS: u32 = 8;

/// The reference binary reads a single HTTP header line into a 0x200
/// (512 byte) buffer.
#[allow(dead_code)]
pub const HEADER_LINE_BUF: usize = 0x200;

/// Stack copy buffer of the reference binary used when pumping a socket into
/// a file: 0x2da0.
pub const COPY_BUF: usize = 0x2da0;

/// The reference binary caps the in-memory chunk buffer at 0x600000 (6 MiB)
/// and switches to memory mapped I/O for larger chunks.
#[allow(dead_code)]
pub const CHUNK_MEM_LIMIT: u64 = 0x600000;

/// How many times one segment may fail before the download is failed.
const MAX_ATTEMPTS: u32 = 8;

/// Minimum interval between two progress events pushed to the UI.
const EMIT_INTERVAL: Duration = Duration::from_millis(120);

/// How often the resume state is written to disk while bytes are arriving.
///
/// The manifest changes on every chunk, but writing it that often would wear
/// the disk for no gain: a crash loses at most this much progress, and the
/// bytes that are already on disk are truncated back to the recorded length
/// when the download resumes.
const MANIFEST_INTERVAL: Duration = Duration::from_secs(2);

/// How often a waiting task looks up from a backoff to see whether the user
/// stopped or paused the download.
const STOP_CHECK_INTERVAL: Duration = Duration::from_millis(200);

/// How often the engine looks at the free space of the destination volume.
const SPACE_CHECK_INTERVAL: Duration = Duration::from_secs(2);

/// The room the engine keeps free on the destination volume, so a download
/// does not fill the disk to the last byte.
const DISK_MARGIN: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// One connection of a download, as the progress window lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentInfo {
    pub id: u32,
    pub start: u64,
    /// The length of the range. `None` when the server did not tell the size
    /// of the file, so the range ends with the response.
    pub size: Option<u64>,
    pub downloaded: u64,
    pub status: SegmentStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfo {
    pub id: u64,
    pub url: String,
    pub dest: String,
    pub total: u64,
    pub downloaded: u64,
    /// The number of ranges the download was split into.
    pub connections: u32,
    pub status: Status,
    pub error: Option<String>,
    /// The page the link was on, when the browser knew it. A redownload uses
    /// it again.
    pub referer: Option<String>,
    /// The connections of the download, with what each one has and is doing.
    pub segments: Vec<SegmentInfo>,
    /// Whether the download can be resumed after a stop. `None` until the
    /// server has answered the probe.
    pub resumable: Option<bool>,
    /// The bandwidth cap of this download, in bytes per second. Zero means
    /// no cap.
    pub speed_limit: u64,
    /// The category the user picked, when they picked one.
    pub category: Option<String>,
    /// The note the user typed for this download, when they typed one.
    pub description: Option<String>,
    /// The number of connections this host allowed the last time it was
    /// asked, when it was asked before.
    pub learned_connections: Option<u32>,
    /// The country the answering server is in, as a two-letter code.
    pub country: Option<String>,
    /// The round trip to the server, in milliseconds.
    pub rtt_ms: Option<u64>,
}

/// Pause state shared by every task of one download.
pub struct Control {
    paused_tx: watch::Sender<bool>,
    paused_rx: watch::Receiver<bool>,
}

impl Control {
    fn new() -> Self {
        let (paused_tx, paused_rx) = watch::channel(false);
        Self {
            paused_tx,
            paused_rx,
        }
    }

    pub fn pause(&self) {
        let _ = self.paused_tx.send(true);
    }

    /// Whether the download is paused right now.
    pub fn is_paused(&self) -> bool {
        *self.paused_rx.borrow()
    }

    pub fn resume(&self) {
        let _ = self.paused_tx.send(false);
    }

    /// Block while the download is paused.
    async fn wait_while_paused(&self, epoch: u64, download: &Arc<Download>) {
        let mut rx = self.paused_rx.clone();
        loop {
            if download.is_cancelled(epoch) {
                return;
            }
            let paused = *rx.borrow();
            if !paused {
                return;
            }
            if rx.changed().await.is_err() {
                return;
            }
        }
    }
}

/// On-disk resume state. The reference download manager stores the
/// equivalent in the registry under
/// `HKCU\Software\DownloadManager\<id>\ChList`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    url: String,
    total: u64,
    segments: Vec<Segment>,
    /// The validator the server gave when this plan was made. A resume whose
    /// probe reports another validator is not the same file.
    #[serde(default)]
    validator: Option<String>,
}

impl Manifest {
    /// Whether the chunk list of this manifest can be trusted.
    ///
    /// The reference binary walks its chunk list when it rebuilds it from the
    /// registry and compares the start of every chunk with the position it has
    /// reached. A chunk that starts before that position reports a "Chunk order
    /// violation error", and the binary rolls the chunk file back to the size
    /// the list says it should have.
    ///
    /// The same damage reaches a manifest: a crash in the middle of a write
    /// can leave ranges that overlap or that are out of order. A broken list
    /// is not resumed from, the download starts over.
    ///
    /// The ranges are walked in file order, not in the order they were planned:
    /// a split appends the tail of a range to the end of the list, so the
    /// list is rarely in file order. What has to hold is that the ranges
    /// cover the file once each, with no gap and no overlap.
    fn is_consistent(&self) -> bool {
        let mut ranges: Vec<(u64, u64)> = self
            .segments
            .iter()
            .map(|segment| (segment.start, segment.end))
            .collect();
        ranges.sort_unstable();

        let mut expected_start = 0u64;
        for (start, end) in ranges {
            if start != expected_start || end < start {
                return false;
            }
            expected_start = end;
        }
        for segment in &self.segments {
            if segment.downloaded > segment.len() {
                return false;
            }
        }
        self.total == 0 || expected_start == self.total
    }
}

pub struct Download {
    pub id: u64,
    pub url: String,
    pub dest: PathBuf,
    pub dir: PathBuf,
    pub stem: String,
    pub manifest: PathBuf,
    /// Whether the parts live in a folder of their own rather than beside the
    /// destination. A staged download owns its folder: when the last byte of
    /// it is joined the folder goes too. Beside the destination the folder is
    /// the user's and is never removed.
    pub staged: bool,
    pub total: AtomicU64,
    pub max_connections: u32,
    pub segments: Mutex<Vec<Segment>>,
    pub status: Mutex<Status>,
    pub error: Mutex<Option<String>>,
    pub control: Control,
    pub next_segment_id: AtomicU32,
    /// Where the bytes come from: an HTTP server or an FTP server.
    pub source: Source,
    /// The page the link was on, when the browser knew it.
    pub referer: Option<String>,
    /// The bandwidth cap for this download.
    limiter: Arc<SpeedLimiter>,
    /// Whether the file can be resumed from the parts on disk, once the probe
    /// has answered.
    resumable: Mutex<Option<bool>>,
    /// The validator the probe was answered with, sent as `If-Range` on
    /// every range. It proves all the bytes come from one version of the file.
    validator: Mutex<Option<String>>,
    /// Held for the length of a run, so two tasks never drive the same
    /// download at once. A resume that arrives while a cancelled task is still
    /// winding down waits here instead of writing over its range plan.
    driver: AsyncMutex<()>,
    /// The head of the file, kept once it is known. The head of every other
    /// range is compared with it, the way the reference binary compares the
    /// start of a chunk with the start of the file.
    head: Mutex<Vec<u8>>,
    /// Set when the server was seen to ignore a range request. The download
    /// does not open any more connections after that.
    degraded: AtomicBool,
    /// How many times a segment may fail before the download is failed.
    max_attempts: AtomicU32,
    /// The epoch of the task that runs this download. Every restart gets a
    /// new one, so a task that was left over from an earlier run can be told
    /// apart from the one that runs now.
    epoch: AtomicU64,
    /// The newest epoch that was cancelled by the user. A task is cancelled
    /// when its own epoch is not newer than this.
    cancelled_epoch: AtomicU64,
    /// The newest epoch that was stopped because something went wrong.
    stopped_epoch: AtomicU64,
    /// The number of runs the application has asked for. A request is claimed
    /// before the task is spawned, so a stop that arrives while the task waits
    /// for the previous run to finish still covers it.
    run_requests: AtomicU64,
    /// The newest run request that a cancel covered.
    cancelled_request: AtomicU64,
    /// Whether a task is running this download right now.
    running: AtomicBool,
    /// Set when the row left the list. A task that is still winding down would
    /// otherwise write the resume state of a download nothing can show.
    discarded: AtomicBool,
    /// Whether the destination may be replaced at the end of the transfer.
    /// Only an explicit Replace in the File Exists dialog sets this, so a
    /// file that appears while a download runs is never overwritten.
    allow_replace: AtomicBool,
    /// The category and the note the user gave the download. They are kept
    /// with the index, so the list looks the same after a restart.
    category: Mutex<Option<String>>,
    description: Mutex<Option<String>>,
    last_emit: Mutex<Instant>,
    /// What each connection has taken, and when, for the pacing rules.
    pace: Mutex<crate::pace::Watch>,
    /// The cap learned for this host, kept for the row.
    learned_connections: Mutex<Option<u32>>,
    /// The country of the server that answers, and the round trip to it.
    country: Mutex<Option<String>>,
    rtt_ms: Mutex<Option<u64>>,
    /// The ranges whose tail was handed to a new connection in this run, so a
    /// range is not split over and over.
    redirected: Mutex<Vec<u32>>,
    /// The status the server last refused a request with, kept as a number.
    /// A caller that has to decide whether an address must be renewed cannot
    /// read that decision back out of the message it was rendered into.
    last_status: AtomicU16,
}

/// Where the engine reports the state of a download.
///
/// The engine knows nothing about windows. The desktop app hands it a
/// reporter that pushes the state into the user interface, and the tests hand
/// it one that throws the updates away. That keeps the window toolkit out of
/// the engine, so the engine can be run and tested without a display.
pub trait Reporter: Send + Sync {
    fn report(&self, info: &DownloadInfo);
}

/// A reporter that drops every update.
#[allow(dead_code)] // used by the tests and by a headless client
pub struct SilentReporter;

impl Reporter for SilentReporter {
    fn report(&self, _info: &DownloadInfo) {}
}

/// Push the current state of a download to the reporter.
fn emit(reporter: &Arc<dyn Reporter>, download: &Arc<Download>) {
    reporter.report(&download.info());
}

/// Push the state of a download to the reporter, but not more often than
/// `EMIT_INTERVAL` unless `force` is set.
fn emit_throttled(reporter: &Arc<dyn Reporter>, download: &Arc<Download>, force: bool) {
    let mut last = download.last_emit.lock().expect("last_emit lock poisoned");
    if !force && last.elapsed() < EMIT_INTERVAL {
        return;
    }
    *last = Instant::now();
    drop(last);
    emit(reporter, download);
}

/// Wait between attempts, but let a pause or a stop be seen before the wait
/// is over.
///
/// A backoff can be half a minute long. Sleeping through it would leave a
/// cancelled download holding the driver for the rest of it, and a resume
/// would sit behind a task the user had already stopped.
async fn wait_while_active(download: &Arc<Download>, epoch: u64, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    loop {
        if download.is_cancelled(epoch) {
            return false;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        tokio::time::sleep(left.min(STOP_CHECK_INTERVAL)).await;
    }
}

/// Read the first bytes of the data of a range.
///
/// A part file holds the bytes of one range from its first byte, so the head
/// of the range is the head of the file.
fn read_head(path: &std::path::Path, offset: u64, len: usize) -> Vec<u8> {
    use std::io::{Read as _, Seek, SeekFrom};

    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return Vec::new();
    }
    let mut buffer = vec![0u8; len];
    let Ok(read) = file.read(&mut buffer) else {
        return Vec::new();
    };
    buffer.truncate(read);
    buffer
}

/// The message the engine uses when the data of a range is not the data of
/// that range.
const CORRUPT_MESSAGE: &str =
    "the server sent the whole file instead of the range that was asked for";

/// The message a task uses when the range it was working on was given up.
const ROLLED_BACK_MESSAGE: &str = "the range was rolled back";

/// How many ranges one run may hand to another connection.
///
/// A fast connection would otherwise shred a plan into hundreds of tiny
/// ranges, and every hand-over costs a connection slot and a request.
const REDIRECT_LIMIT: usize = 4;

/// Remove the part files of a download that no range in `keep` owns.
///
/// A split, a downgrade or a crash can leave parts whose ids are no longer in
/// the plan. Nobody will ever join those bytes, so they go.
fn remove_orphan_parts(dir: &Path, stem: &str, keep: &[Segment]) {
    let prefix = format!("{stem}.part");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(id) = name.strip_prefix(&prefix) else {
            continue;
        };
        // Only `{stem}.part<number>` belongs to this download: another file
        // whose name merely starts with the prefix is not ours.
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        if !keep.iter().any(|segment| segment.part == path) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Remove the part files and the manifest of a finished download.
///
/// Everything the engine wrote for this download goes: the ranges, the
/// joined temporary that became the file, the manifest and its write
/// temporary. A staged download then loses the folder it owned, once it is
/// empty.
fn cleanup(download: &Arc<Download>) {
    remove_orphan_parts(&download.dir, &download.stem, &[]);
    let _ = std::fs::remove_file(join_temp_path(&download.dir, &download.stem));
    let _ = std::fs::remove_file(manifest_temp_path(&download.manifest));
    let _ = std::fs::remove_file(&download.manifest);
    download.prune_staged_dir();
}

/// The temporary file the joined parts are written to before they become the
/// destination file.
pub fn join_temp_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}.idmtmp"))
}

/// The temporary file a manifest is written to and renamed from.
fn manifest_temp_path(manifest: &Path) -> PathBuf {
    manifest.with_extension("json.tmp")
}

/// Entry point of one download.
///
/// The whole run is claimed with a request number at this call, so a stop that
/// arrives after the task was spawned covers it. A caller that needs to cover
/// the time between spawning the task and its first poll has to claim the
/// request itself with [`Download::request_run`] and call [`run_claimed`].
pub async fn run(reporter: Arc<dyn Reporter>, limits: Limits, download: Arc<Download>) {
    let request = download.request_run();
    run_claimed(reporter, limits, download, request).await;
}

/// Run one download whose start was claimed with a request number.
///
/// Every run gets a new epoch, so a download that was cancelled and started
/// again does not fight with the task that was left over. Only one task drives
/// a download at a time: a resume that arrives while the task of a cancelled
/// run is still closing its ranges waits here, then continues from the parts
/// and the manifest the previous task left, instead of writing a second range
/// plan over the first.
pub async fn run_claimed(
    reporter: Arc<dyn Reporter>,
    limits: Limits,
    download: Arc<Download>,
    request: u64,
) {
    let _driver = download.driver.lock().await;

    // A request that was stopped or replaced while this task waited for the
    // driver must not start. The download may have been removed, cancelled, or
    // resumed by a newer request; either way, this task is stale.
    if download.is_superseded(request) {
        return;
    }
    // A finished download has nothing to fetch. A second resume that slipped
    // in before the complete window opened must not download the file again.
    if download.status() == Status::Completed {
        return;
    }

    let epoch = download.next_epoch();
    download.running.store(true, Ordering::SeqCst);
    // A download that is started again is not the download that failed: its
    // old message goes before the first byte of the new attempt.
    *download.error.lock().expect("error lock poisoned") = None;
    let outcome = drive(&reporter, &limits, &download, epoch).await;

    // A newer task that was started while this one ran owns the state now.
    if download.epoch.load(Ordering::SeqCst) != epoch {
        return;
    }

    match outcome {
        Ok(()) => {
            log::write("info", &format!("finished {}", download.dest.display()));
            download.mark_segments(SegmentStatus::Done, false);
            *download.error.lock().expect("error lock poisoned") = None;
            download.set_status(Status::Completed)
        }
        Err(error) => {
            if download.is_cancelled_by_user(epoch) {
                // Nothing is in flight on a download that was stopped. The
                // ranges hold what they got; a resume asks for the rest. They
                // say so, instead of claiming to be waiting for a connection
                // that will never come.
                download.mark_segments(SegmentStatus::Stopped, true);
                download.set_status(Status::Cancelled);
            } else {
                log::write("error", &format!("{} failed: {error}", download.url));
                download.mark_segments(SegmentStatus::Failed, true);
                *download.error.lock().expect("error lock poisoned") = Some(error);
                download.set_status(Status::Failed);
            }
        }
    }

    // The part files are gone once the file is assembled, so a finished
    // download has no resume state to write. Everything else does: the last
    // bytes a stopped or failed run received are what the next run continues
    // from. A row that left the list while this task was winding down keeps
    // nothing: it cleans up instead of writing state nothing can resume.
    if download.discarded.load(Ordering::SeqCst) {
        cleanup(&download);
    } else if download.status() != Status::Completed {
        save_manifest(&download);
    }
    download.running.store(false, Ordering::SeqCst);
    emit(&reporter, &download);
}

/// Trim a metadata value and cut it to the length the index holds.
fn clean_meta(value: Option<String>, limit: usize) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(limit).collect::<String>())
}

mod assembly;
mod drive;
mod probe;
mod ranges;
mod resume;
mod state;

pub use resume::{discard_relocated, relocate_resume_state, save_manifest, Relocated};

use drive::drive;

#[cfg(test)]
mod tests;
