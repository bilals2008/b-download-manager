//! The download engine — a port of the mechanics observed in the reference
//! binary.
//!
//! What the reverse engineering established (see `ANALYSIS.md` for the
//! evidence):
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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
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
use crate::transfer::{FailureKind, Probe, Source, TransferError, RANGE_IGNORED_MESSAGE};
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
    fn is_consistent(&self) -> bool {
        let mut expected_start = 0u64;
        for segment in &self.segments {
            if segment.start != expected_start || segment.end < segment.start {
                return false;
            }
            if segment.downloaded > segment.len() {
                return false;
            }
            expected_start = segment.end;
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
}

impl Download {
    pub fn new(
        id: u64,
        url: String,
        dest: PathBuf,
        connections: u32,
        proxy: Option<&str>,
        referer: Option<&str>,
    ) -> Result<Arc<Download>, String> {
        let dir = dest
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

        let stem = dest
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("download{id}"));
        let manifest = dir.join(format!("{stem}.idm.json"));

        let source = Source::parse(&url, proxy, referer)?;
        let referer = referer
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty());

        let last_emit = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);

        Ok(Arc::new(Download {
            id,
            url,
            dest,
            dir,
            stem,
            manifest,
            total: AtomicU64::new(0),
            max_connections: connections.max(1),
            segments: Mutex::new(Vec::new()),
            status: Mutex::new(Status::Queued),
            error: Mutex::new(None),
            control: Control::new(),
            next_segment_id: AtomicU32::new(0),
            source,
            referer,
            limiter: Arc::new(SpeedLimiter::unlimited()),
            resumable: Mutex::new(None),
            validator: Mutex::new(None),
            driver: AsyncMutex::new(()),
            head: Mutex::new(Vec::new()),
            degraded: AtomicBool::new(false),
            max_attempts: AtomicU32::new(MAX_ATTEMPTS),
            epoch: AtomicU64::new(0),
            cancelled_epoch: AtomicU64::new(0),
            stopped_epoch: AtomicU64::new(0),
            run_requests: AtomicU64::new(0),
            cancelled_request: AtomicU64::new(0),
            running: AtomicBool::new(false),
            discarded: AtomicBool::new(false),
            allow_replace: AtomicBool::new(false),
            category: Mutex::new(None),
            description: Mutex::new(None),
            last_emit: Mutex::new(last_emit),
        }))
    }

    pub fn info(&self) -> DownloadInfo {
        let total = self.total.load(Ordering::SeqCst);
        let (mut downloaded, connections, segments) = {
            let segments = self.segments.lock().expect("segments lock poisoned");
            (
                segments.iter().map(|s| s.downloaded).sum::<u64>(),
                segments.len() as u32,
                segments
                    .iter()
                    .map(|s| SegmentInfo {
                        id: s.id,
                        start: s.start,
                        size: if s.is_unknown() { None } else { Some(s.len()) },
                        downloaded: s.downloaded,
                        status: s.status,
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let status = *self.status.lock().expect("status lock poisoned");
        let error = self.error.lock().expect("error lock poisoned").clone();
        let resumable = *self.resumable.lock().expect("resumable lock poisoned");

        // A finished file is finished. Its ranges were joined and its part
        // files and manifest removed, so the segment list of a download
        // restored from the index adds up to nothing; without this the row
        // would show the full size and 0% of it.
        if status == Status::Completed {
            downloaded = downloaded.max(total);
        }

        DownloadInfo {
            id: self.id,
            url: self.url.clone(),
            dest: self.dest.to_string_lossy().to_string(),
            total,
            downloaded,
            connections,
            status,
            error,
            referer: self.referer.clone(),
            segments,
            resumable,
            speed_limit: self.limiter.rate(),
            category: self
                .category
                .lock()
                .expect("category lock poisoned")
                .clone(),
            description: self
                .description
                .lock()
                .expect("description lock poisoned")
                .clone(),
        }
    }

    /// Record the category and the note the user gave the download.
    ///
    /// Both come from a text field, so they are trimmed and cut to the length
    /// the index will hold; an empty value is no value.
    pub fn set_meta(&self, category: Option<String>, description: Option<String>) {
        self.set_category(category);
        self.set_description(description);
    }

    /// Record the category alone, leaving the note as it is.
    ///
    /// Moving a download between categories is metadata: the destination and
    /// everything on disk stay untouched.
    pub fn set_category(&self, category: Option<String>) {
        *self.category.lock().expect("category lock poisoned") = clean_meta(category, 40);
    }

    /// Record the note alone, leaving the category as it is.
    pub fn set_description(&self, description: Option<String>) {
        *self.description.lock().expect("description lock poisoned") = clean_meta(description, 500);
    }

    /// The bandwidth cap of this download, in bytes per second. Zero means no
    /// cap.
    pub fn speed_limit(&self) -> u64 {
        self.limiter.rate()
    }

    /// Record whether the file can be resumed, once the probe has answered.
    pub fn set_resumable(&self, resumable: bool) {
        *self.resumable.lock().expect("resumable lock poisoned") = Some(resumable);
    }

    /// Record the validator the probe was answered with.
    fn set_validator(&self, validator: Option<String>) {
        *self.validator.lock().expect("validator lock poisoned") = validator;
    }

    /// The validator the probe was answered with, when there is one.
    fn validator(&self) -> Option<String> {
        self.validator
            .lock()
            .expect("validator lock poisoned")
            .clone()
    }

    /// Allow the destination file to be replaced when the transfer finishes.
    ///
    /// This is set only from an explicit Replace; every other start leaves it
    /// off, and `assemble` refuses to overwrite a file that appeared while the
    /// download was running.
    pub fn set_allow_replace(&self, allow: bool) {
        self.allow_replace.store(allow, Ordering::SeqCst);
    }

    /// Forget every byte received so far and remove the resume state.
    ///
    /// Used by Retry when the server cannot resume: the next run plans fresh
    /// ranges instead of continuing from parts that cannot be continued.
    pub fn discard_progress(&self) -> Result<(), String> {
        if self.is_running() {
            return Err("the download is still running".to_string());
        }
        remove_orphan_parts(&self.dir, &self.stem, &[]);
        let _ = std::fs::remove_file(&self.manifest);
        self.segments
            .lock()
            .expect("segments lock poisoned")
            .clear();
        self.total.store(0, Ordering::SeqCst);
        self.next_segment_id.store(0, Ordering::SeqCst);
        self.head.lock().expect("head lock poisoned").clear();
        self.degraded.store(false, Ordering::SeqCst);
        *self.resumable.lock().expect("resumable lock poisoned") = None;
        self.set_validator(None);
        Ok(())
    }

    /// Remove the parts and the manifest of a download that is leaving the list.
    ///
    /// A row that no window can show has nothing left to resume, so its bytes
    /// go with it. A task that is still winding down writes the manifest back
    /// at its end; the flag makes it clean up there instead.
    pub fn forget_progress(&self) {
        self.discarded.store(true, Ordering::SeqCst);
        remove_orphan_parts(&self.dir, &self.stem, &[]);
        let _ = std::fs::remove_file(&self.manifest);
    }

    /// The epoch of the newest task of this download.
    pub fn current_epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// The state the engine reports for this download.
    pub fn status(&self) -> Status {
        *self.status.lock().expect("status lock poisoned")
    }

    pub fn set_status(&self, status: Status) {
        *self.status.lock().expect("status lock poisoned") = status;
    }

    /// Forget the message of the last failure, before a new attempt starts.
    pub fn clear_error(&self) {
        *self.error.lock().expect("error lock poisoned") = None;
    }

    /// Cap the bandwidth of this download. Zero means no cap.
    pub fn set_speed_limit(&self, bytes_per_second: u64) {
        self.limiter.set_rate(bytes_per_second);
    }

    /// Hold every connection of this download back.
    ///
    /// The connections are marked here as well as told to wait, so the table
    /// of the progress window says `Paused` at once instead of waiting for
    /// every stream to notice.
    pub fn pause(&self) {
        self.control.pause();
        self.mark_segments(SegmentStatus::Paused, true);
    }

    /// Start a new task for this download and return its epoch.
    ///
    /// A task that is already running is left alone: its epoch is older than
    /// the new one, so a cancel still reaches it.
    pub fn next_epoch(&self) -> u64 {
        self.epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Ask for a new run of this download.
    ///
    /// The request is claimed here rather than inside the task, because a
    /// cancel can arrive while the task is still waiting for the previous run
    /// to let go of the driver. A request that a cancel has covered must not
    /// start afterwards.
    pub fn request_run(&self) -> u64 {
        self.run_requests.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Whether a run request was stopped or replaced before it started.
    pub fn is_superseded(&self, request: u64) -> bool {
        request <= self.cancelled_request.load(Ordering::SeqCst)
            || request < self.run_requests.load(Ordering::SeqCst)
    }

    /// Stop the task that runs this download.
    pub fn cancel(&self) {
        let current = self.epoch.load(Ordering::SeqCst);
        self.cancelled_epoch.fetch_max(current, Ordering::SeqCst);
        // Every run request made so far is covered by this cancel, including
        // one whose task has not reached the driver yet.
        let request = self.run_requests.load(Ordering::SeqCst);
        self.cancelled_request.fetch_max(request, Ordering::SeqCst);
        // A paused range waits on the pause channel, not on the network, so a
        // cancel has to release it or the task would wait forever. The recheck
        // of the epoch in every loop is what stops it once it wakes.
        self.control.resume();
    }

    /// Stop the tasks of this download without calling it cancelled.
    ///
    /// This is what the engine does when one range fails: the other ranges
    /// stop, and the download is reported as failed, not as cancelled.
    pub fn stop(&self) {
        let current = self.epoch.load(Ordering::SeqCst);
        self.stopped_epoch.fetch_max(current, Ordering::SeqCst);
        self.control.resume();
    }

    /// Whether the task with this epoch was stopped.
    pub fn is_cancelled(&self, epoch: u64) -> bool {
        epoch <= self.cancelled_epoch.load(Ordering::SeqCst)
            || epoch <= self.stopped_epoch.load(Ordering::SeqCst)
    }

    /// Whether the task with this epoch was cancelled by the user.
    pub fn is_cancelled_by_user(&self, epoch: u64) -> bool {
        epoch <= self.cancelled_epoch.load(Ordering::SeqCst)
    }

    /// Whether a task is running this download right now.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Rebuild a download the app remembers from an earlier run.
    ///
    /// The parts and the manifest are still on disk, so the download can be
    /// resumed. It comes back paused: nothing starts on its own.
    pub fn from_known(known: &crate::session::KnownDownload) -> Option<Arc<Download>> {
        let download = Download::new(
            known.id,
            known.url.clone(),
            PathBuf::from(&known.dest),
            known.connections,
            None,
            known.referer.as_deref(),
        )
        .ok()?;

        if let Some(manifest) = read_manifest(&download) {
            let validator = manifest.validator.clone();
            download.total.store(manifest.total, Ordering::SeqCst);
            let next_id = manifest.segments.iter().map(|s| s.id).max().unwrap_or(0) + 1;
            download.next_segment_id.store(next_id, Ordering::SeqCst);
            *download.segments.lock().expect("segments lock poisoned") = manifest.segments;
            // The validator outlives the process with the plan it belongs to.
            download.set_validator(validator);
        } else {
            download.total.store(known.total, Ordering::SeqCst);
        }

        // The manifest on disk is the proof that the file can be resumed.
        if download.manifest.exists() {
            download.set_resumable(true);
        }
        download.set_speed_limit(known.speed_limit);
        download.set_meta(known.category.clone(), known.description.clone());
        // The Replace answer outlives the process: the file it replaces is
        // still there, and the finished download has to be allowed to put
        // itself over it.
        download.set_allow_replace(known.replace);
        download.reset_segment_statuses();

        download.set_status(match known.status {
            Status::Completed => Status::Completed,
            Status::Failed => Status::Failed,
            _ => Status::Paused,
        });
        *download.error.lock().expect("error lock poisoned") = known.error.clone();

        Some(download)
    }

    /// Write the resume state of this download to disk.
    pub fn save_manifest(&self) {
        save_manifest(self);
    }

    /// The index entry for this download.
    pub fn known(&self) -> crate::session::KnownDownload {
        let info = self.info();
        crate::session::KnownDownload {
            id: info.id,
            url: info.url,
            dest: info.dest,
            connections: self.max_connections,
            total: info.total,
            downloaded: info.downloaded,
            status: info.status,
            error: info.error,
            referer: self.referer.clone(),
            speed_limit: self.speed_limit(),
            category: info.category,
            description: info.description,
            replace: self.allow_replace.load(Ordering::SeqCst),
        }
    }

    fn seg_snapshot(&self, id: u32) -> Option<(u64, u64, u64)> {
        let segments = self.segments.lock().expect("segments lock poisoned");
        segments
            .iter()
            .find(|s| s.id == id)
            .map(|s| (s.start, s.end, s.downloaded))
    }

    fn seg_part(&self, id: u32) -> Option<PathBuf> {
        let segments = self.segments.lock().expect("segments lock poisoned");
        segments.iter().find(|s| s.id == id).map(|s| s.part.clone())
    }

    fn set_seg_downloaded(&self, id: u32, value: u64) {
        self.record_progress(id, value, SegmentStatus::Receiving);
    }

    /// Record how far a connection has got and what it is doing, in one lock.
    fn record_progress(&self, id: u32, downloaded: u64, status: SegmentStatus) {
        let mut segments = self.segments.lock().expect("segments lock poisoned");
        if let Some(segment) = segments.iter_mut().find(|s| s.id == id) {
            segment.downloaded = downloaded.min(segment.len());
            segment.status = status;
        }
    }

    /// Set the state of one connection.
    fn set_seg_status(&self, id: u32, status: SegmentStatus) {
        let mut segments = self.segments.lock().expect("segments lock poisoned");
        if let Some(segment) = segments.iter_mut().find(|s| s.id == id) {
            segment.status = status;
        }
    }

    /// Mark every connection of the plan. Ranges that are already complete
    /// keep their state.
    fn mark_segments(&self, status: SegmentStatus, only_unfinished: bool) {
        let mut segments = self.segments.lock().expect("segments lock poisoned");
        for segment in segments.iter_mut() {
            if only_unfinished && segment.done() {
                segment.status = SegmentStatus::Done;
            } else {
                segment.status = status;
            }
        }
    }

    /// A download that is read back from disk has no connections open, so its
    /// ranges are waiting. The bytes they hold are still there.
    fn reset_segment_statuses(&self) {
        let mut segments = self.segments.lock().expect("segments lock poisoned");
        for segment in segments.iter_mut() {
            segment.status = if segment.done() {
                SegmentStatus::Done
            } else {
                SegmentStatus::Pending
            };
        }
    }

    /// Close a segment whose length the server never told us: the bytes that
    /// arrived are the whole file.
    fn finish_unknown_segment(&self, id: u32) {
        let mut segments = self.segments.lock().expect("segments lock poisoned");
        if let Some(segment) = segments.iter_mut().find(|s| s.id == id) {
            if segment.end == UNKNOWN_END {
                segment.end = segment.start + segment.downloaded;
                segment.status = SegmentStatus::Done;
            }
        }
    }
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

/// Whether the manifest on disk describes the file the server just showed.
fn can_resume(manifest: &Manifest, probe: &Probe) -> bool {
    // The probe only proves the server honours ranges if it answered `206`.
    if !probe.ranges {
        return false;
    }
    let current = probe.validator();
    match probe.total {
        Some(total) => {
            if manifest.total != total {
                return false;
            }
            // When both sides name a version, they have to agree. A server
            // that stopped sending a validator leaves the size as the only
            // proof, and an older manifest has no validator at all.
            match (&manifest.validator, current) {
                (Some(old), Some(new)) => *old == new,
                _ => true,
            }
        }
        // Without a size, only a matching validator proves that the bytes on
        // disk are the first bytes of the file the server still serves.
        None => {
            manifest.total == 0
                && manifest.segments.len() == 1
                && manifest.segments[0].is_unknown()
                && matches!((&manifest.validator, current), (Some(old), Some(new)) if *old == new)
        }
    }
}

/// Rebuild the segment list, resuming from the manifest when possible.
fn restore(download: &Arc<Download>, probe: &Probe, max_connections: u32) -> Vec<Segment> {
    // The head is rebuilt from the range that starts at the first byte: from
    // the part on disk when the download resumes, from the stream when it
    // starts over. An old run's head belongs to none of them.
    download.head.lock().expect("head lock poisoned").clear();

    if let Some(manifest) = read_manifest(download) {
        if can_resume(&manifest, probe) {
            let mut segments = manifest.segments;
            // A crash can leave extra bytes behind. Cut the part file back
            // to the recorded length, the way the reference binary truncates
            // the file when it resumes ("Trucate at %I64d").
            for segment in &mut segments {
                if let Ok(file) = OpenOptions::new().write(true).open(&segment.part) {
                    let _ = file.set_len(segment.downloaded);
                }
                // The connections of a manifest are not running now.
                segment.status = if segment.done() {
                    SegmentStatus::Done
                } else {
                    SegmentStatus::Pending
                };
            }
            // The other ranges are checked against the head of the file. A
            // resumed download already has the head in its first part.
            if let Some(first) = segments.iter().find(|segment| segment.start == 0) {
                let len = first.downloaded.min(verify::HEAD_BUFFER as u64) as usize;
                let head = read_head(&first.part, 0, len);
                *download.head.lock().expect("head lock poisoned") = head;
            }
            let next_id = segments.iter().map(|s| s.id).max().unwrap_or(0) + 1;
            download.next_segment_id.store(next_id, Ordering::SeqCst);
            *download.segments.lock().expect("segments lock poisoned") = segments.clone();
            return segments;
        }

        // The plan on disk belongs to another version of the file, or the
        // server no longer honours ranges. Its bytes are not used again, and
        // every part goes, not only the ones whose ids the new plan reuses.
        remove_orphan_parts(&download.dir, &download.stem, &[]);
        let _ = std::fs::remove_file(&download.manifest);
        log::write(
            "info",
            "the resume state did not match the server, the file starts over",
        );
    }

    let segments = match probe.total {
        Some(total) => segmenter::plan(total, max_connections, &download.dir, &download.stem),
        // The server did not tell us the size, so the file is fetched by a
        // single connection, from the first byte to the end of the response.
        None => segmenter::plan_unknown(&download.dir, &download.stem),
    };
    // A fresh plan starts from scratch: every part file of an earlier run is
    // stale, including the ones whose ids the new plan does not reuse.
    remove_orphan_parts(&download.dir, &download.stem, &[]);
    download
        .next_segment_id
        .store(segments.len() as u32, Ordering::SeqCst);
    *download.segments.lock().expect("segments lock poisoned") = segments.clone();
    segments
}

/// Read the manifest of a download, when it is there and still matches.
fn read_manifest(download: &Arc<Download>) -> Option<Manifest> {
    let text = std::fs::read_to_string(&download.manifest).ok()?;
    let manifest: Manifest = serde_json::from_str(&text).ok()?;
    if manifest.url != crate::transfer::redacted(&download.url)
        || manifest.segments.is_empty()
        // A range that recorded bytes must still have them. A range that
        // recorded none has nothing to lose: its part is created when the
        // first byte arrives, so a crash before that is not a reason to
        // throw the rest of the plan away.
        || manifest
            .segments
            .iter()
            .any(|segment| segment.downloaded > 0 && !segment.part.exists())
        || !manifest.is_consistent()
    {
        return None;
    }
    Some(manifest)
}

pub fn save_manifest(download: &Download) {
    let segments = download
        .segments
        .lock()
        .expect("segments lock poisoned")
        .clone();
    let manifest = Manifest {
        // The manifest lies next to the file, which may be in a folder that
        // is shared, so the password of the URL is not in it.
        url: crate::transfer::redacted(&download.url),
        total: download.total.load(Ordering::SeqCst),
        segments,
        validator: download.validator(),
    };
    let Ok(text) = serde_json::to_string_pretty(&manifest) else {
        return;
    };

    // The manifest is written to a temporary file and renamed into place: a
    // crash in the middle of the write must not destroy the resume state.
    let temp = download.manifest.with_extension("json.tmp");
    if std::fs::write(&temp, text).is_ok() {
        let _ = std::fs::rename(&temp, &download.manifest);
    }
}

/// Hand the tail of the slowest segment to a new connection.
///
/// This is the "add an optional connection" behaviour of the reference
/// download manager: the running connection keeps the bytes it already wrote,
/// and a fresh connection continues from the point it reached.
///
/// The segment that is chosen is the one that is furthest behind in
/// *progress*, not the one with the most bytes left. Splitting by progress
/// is what keeps the connections balanced: a segment that is short but
/// nearly finished is not worth a new connection, while a segment that is
/// slow keeps holding the whole file back.
fn maybe_grow(
    reporter: &Arc<dyn Reporter>,
    limits: &Limits,
    download: &Arc<Download>,
    set: &mut tokio::task::JoinSet<Result<(), String>>,
    epoch: u64,
) {
    let mut segments = download.segments.lock().expect("segments lock poisoned");
    if segments.len() as u32 >= download.max_connections {
        return;
    }
    // A server that was seen to ignore a range request does not get any
    // more connections.
    if download.degraded.load(Ordering::SeqCst) {
        return;
    }

    // The slowest unfinished segment, by the share of its range that has
    // arrived. The fastest one sets the bar the slowest one has to fall
    // behind before another connection is worth its keep.
    let mut slowest: Option<usize> = None;
    let mut slowest_ratio = f64::MAX;
    let mut fastest_ratio = 0.0f64;
    let mut unfinished = 0usize;

    for (index, segment) in segments.iter().enumerate() {
        if segment.done() {
            continue;
        }
        let len = segment.len();
        if len == 0 || len == UNKNOWN_END {
            continue;
        }
        unfinished += 1;
        let ratio = segment.downloaded as f64 / len as f64;
        if ratio > fastest_ratio {
            fastest_ratio = ratio;
        }
        if ratio < slowest_ratio {
            slowest_ratio = ratio;
            slowest = Some(index);
        }
    }

    let Some(index) = slowest else { return };
    let segment = &segments[index];
    let left = segment.len() - segment.downloaded;
    if left < MIN_SPLIT {
        return;
    }
    // A single unfinished range has nothing to lag behind: the next
    // alternative connection is what gives the file another one. With more
    // than one, a new connection is only worth it when the slowest really
    // lags the fastest, or every range would be split in half for no gain.
    if unfinished > 1 && slowest_ratio > fastest_ratio * 0.8 {
        return;
    }

    let new_id = download.next_segment_id.fetch_add(1, Ordering::SeqCst);
    let Some(new_segment) =
        segmenter::split(&mut segments[index], new_id, &download.dir, &download.stem)
    else {
        return;
    };
    segments.push(new_segment.clone());
    drop(segments);

    // The new range starts empty, even when an earlier run left a file with
    // the same name behind.
    let _ = std::fs::remove_file(&new_segment.part);

    set.spawn(segment_task(
        reporter.clone(),
        limits.clone(),
        download.clone(),
        new_segment.id,
        epoch,
    ));
    emit(reporter, download);
}

/// Request one byte range and write it into the segment's part file.
async fn fetch_range(
    download: &Arc<Download>,
    limits: &Limits,
    seg_id: u32,
    seg_start: u64,
    from: u64,
    to: Option<u64>,
    epoch: u64,
) -> Result<(), TransferError> {
    // One connection slot of the whole application. The slot is held for the
    // length of the request and given back when the response is done.
    let _permit = limits
        .budget
        .acquire()
        .await
        .ok_or_else(|| TransferError::msg("the connection budget is closed"))?;

    download.set_seg_status(seg_id, SegmentStatus::Connecting);

    // The whole file in one range, or a file whose size is unknown: the
    // server may answer with the whole file instead of the range. A range
    // that starts after the first byte is never the whole file, whatever its
    // end says.
    let total = download.total.load(Ordering::SeqCst);
    let covers_all = from == 0
        && match to {
            None => true,
            Some(to) => total > 0 && to + 1 == total && seg_start == 0,
        };

    // The validator proves that every range is answered from the same version
    // of the file. A server whose copy changed answers the whole file
    // instead, which `open` turns into an error.
    let validator = download.validator();
    let expected_total = (total > 0).then_some(total);
    let mut stream = download
        .source
        .open(from, to, covers_all, validator.as_deref(), expected_total)
        .await?;
    download.set_seg_status(seg_id, SegmentStatus::Receiving);

    let Some(part) = download.seg_part(seg_id) else {
        // The range was given up while the connection was opening: another
        // connection owns the file now, and this one has nothing to write.
        return Ok(());
    };
    let mut file = OpenOptions::new()
        .create(true)
        // The part file holds the bytes of one range, from the first byte of
        // the range. It is never truncated here: a resumed download writes
        // over the bytes it has and keeps the rest.
        .truncate(false)
        .write(true)
        .open(&part)
        .map_err(|e| TransferError::permanent(e.to_string()))?;

    let base = from - seg_start;
    file.seek(SeekFrom::Start(base))
        .map_err(|e| TransferError::permanent(e.to_string()))?;

    let mut written = 0u64;
    // Set when the response was dropped because the user paused or stopped
    // the download. Such a response is not the end of a file whose size the
    // server never told, so the range must not be closed as if it were.
    let mut interrupted = false;

    while let Some(item) = stream.next().await {
        // A pause or a cancel reaches a range that is streaming: the response
        // is dropped, which closes its connection, and the task waits for the
        // resume or ends. A resume asks for the range again from the byte the
        // part file ended on.
        if download.control.is_paused() || download.is_cancelled(epoch) {
            interrupted = true;
            if download.control.is_paused() {
                download.set_seg_status(seg_id, SegmentStatus::Paused);
            }
            break;
        }

        let bytes = item.map_err(TransferError::msg)?;
        if bytes.is_empty() {
            continue;
        }

        // The segment may have been shortened while this request was running,
        // because a new connection took over the tail of the range. A range
        // that is gone was given up as a whole: the other connection fetches
        // the file.
        let Some((_, seg_end, _)) = download.seg_snapshot(seg_id) else {
            return Ok(());
        };
        let position = from + written;
        if position >= seg_end {
            break;
        }

        let take = ((seg_end - position).min(bytes.len() as u64)) as usize;

        // The bandwidth caps are applied before the bytes hit the disk.
        limits.global_speed.reserve(take as u64).await;
        download.limiter.reserve(take as u64).await;

        file.write_all(&bytes[..take])
            .map_err(|e| TransferError::permanent(e.to_string()))?;
        written += take as u64;

        download.record_progress(seg_id, base + written, SegmentStatus::Receiving);

        // The head of the file is the data of the first range. It is kept
        // while it streams in, so the other ranges can be compared with it
        // as soon as it is there.
        if seg_start == 0 {
            let mut head = download.head.lock().expect("head lock poisoned");
            if head.len() < verify::HEAD_BUFFER {
                let take = (verify::HEAD_BUFFER - head.len()).min(take);
                head.extend_from_slice(&bytes[..take]);
            }
        }

        // The range may have been rolled back while this request was
        // running, because the data was not the data of this range.
        let Some((_, _, current)) = download.seg_snapshot(seg_id) else {
            // The plan was given up (the download fell back to one
            // connection) while this chunk was being written.
            return Ok(());
        };
        if current < base + written {
            return Err(TransferError::msg(ROLLED_BACK_MESSAGE));
        }
    }

    file.flush()
        .map_err(|e| TransferError::permanent(e.to_string()))?;

    // A segment whose length the server never told us ends with the response.
    // An interrupted response is not an end: the rest is asked for again after
    // the resume, from the byte the part file stopped on.
    let Some((_, end, downloaded)) = download.seg_snapshot(seg_id) else {
        return Ok(());
    };
    if end == UNKNOWN_END {
        if !interrupted {
            download.finish_unknown_segment(seg_id);
        }
        return Ok(());
    }

    let expected = end.saturating_sub(from);
    if !interrupted && written < expected {
        // The connection closed before the end of the range, and the rest of
        // the range is still to come. Asking again from the byte the part
        // file reached is what a resumed download does anyway.
        return Err(TransferError::msg(format!(
            "the response ended after {written} of {expected} bytes"
        )));
    }
    if downloaded >= end - seg_start {
        // A known range that has every byte is done even if the pause landed
        // exactly on the last one.
        download.set_seg_status(seg_id, SegmentStatus::Done);
    }

    Ok(())
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

/// Give up on the other connections and fetch the file with one.
///
/// The reference download manager does this when the data of several
/// connections does not match the data that was downloaded earlier. Its own
/// message says that the file was taken over by one connection because of
/// data corruption, when newly received data in several connections did not
/// match the same data downloaded earlier.
fn downgrade_to_one_connection(download: &Arc<Download>, total: Option<u64>) -> Option<Segment> {
    let new_id = download.next_segment_id.fetch_add(1, Ordering::SeqCst);
    let part = segmenter::part_path(&download.dir, &download.stem, new_id);

    let mut segments = download.segments.lock().expect("segments lock poisoned");

    let segment = Segment {
        id: new_id,
        start: 0,
        end: total.unwrap_or(UNKNOWN_END),
        downloaded: 0,
        part,
        status: SegmentStatus::Pending,
    };
    // The bytes that were fetched cannot be trusted, so the file is fetched
    // again from the start. Parts another connection is still writing go too;
    // a file that is open on Windows refuses the delete, and the sweep after
    // the download catches that one.
    remove_orphan_parts(
        &download.dir,
        &download.stem,
        std::slice::from_ref(&segment),
    );
    let _ = std::fs::remove_file(&segment.part);
    segments.clear();
    segments.push(segment.clone());

    download.degraded.store(true, Ordering::SeqCst);
    log::write(
        "warn",
        "the data of the ranges did not match, the download goes on with one connection",
    );
    Some(segment)
}

/// Check every range that has data against the head of the file.
fn verify_ranges(download: &Arc<Download>) {
    let segments: Vec<u32> = download
        .segments
        .lock()
        .expect("segments lock poisoned")
        .iter()
        .filter(|s| s.start > 0 && s.downloaded > 0)
        .map(|s| s.id)
        .collect();

    for id in segments {
        let _ = verify_segment(download, id);
    }
}

/// Check the data of a segment against the head of the file.
///
/// A segment that starts with the head of the file holds data that belongs
/// to the first range: the server sent the whole file instead of the range
/// that was asked for. The segment is rolled back and fetched again, the way
/// the reference binary gives the bytes back and starts the chunk over.
fn verify_segment(download: &Arc<Download>, seg_id: u32) -> Result<(), String> {
    let Some((start, _, downloaded)) = download.seg_snapshot(seg_id) else {
        return Ok(());
    };
    if downloaded == 0 {
        return Ok(());
    }
    // The first range is the head of the file.
    if start == 0 {
        return Ok(());
    }

    let head = download.head.lock().expect("head lock poisoned").clone();
    if head.is_empty() {
        return Ok(());
    }

    let Some(part) = download.seg_part(seg_id) else {
        return Ok(());
    };
    let chunk = read_head(&part, 0, verify::HEAD_BYTES);

    match verify::check(&head, &chunk, false) {
        verify::Verdict::Good => Ok(()),
        verify::Verdict::Repeated => {
            // The bytes of this range are not the bytes of this range. Give
            // them back and fetch the range again.
            download.set_seg_downloaded(seg_id, 0);
            log::write(
                "warn",
                "a range held the head of the file, the range was given back",
            );
            Err(CORRUPT_MESSAGE.to_string())
        }
    }
}

/// Download one segment, retrying on transient errors.
async fn segment_task(
    reporter: Arc<dyn Reporter>,
    limits: Limits,
    download: Arc<Download>,
    seg_id: u32,
    epoch: u64,
) -> Result<(), String> {
    let mut attempt = 0u32;

    loop {
        if download.is_cancelled(epoch) {
            return Ok(());
        }
        download.control.wait_while_paused(epoch, &download).await;
        if download.is_cancelled(epoch) {
            return Ok(());
        }

        let Some((start, end, downloaded)) = download.seg_snapshot(seg_id) else {
            return Ok(());
        };
        let unknown = end == UNKNOWN_END;
        if !unknown && downloaded >= end - start {
            download.set_seg_status(seg_id, SegmentStatus::Done);
            return Ok(());
        }

        let from = start + downloaded;
        let to = if unknown { None } else { Some(end - 1) };

        match fetch_range(&download, &limits, seg_id, start, from, to, epoch).await {
            Ok(()) => {
                // The first range of the download is the head of the file.
                if start == 0 {
                    verify_ranges(&download);
                }
                // A range that holds the head of the file is not the range
                // that was asked for. The range is given back and the
                // download goes on with a single connection.
                verify_segment(&download, seg_id)?;
                attempt = 0;
                emit_throttled(&reporter, &download, false);
            }
            Err(error) => {
                // A failure that asking again cannot fix is not asked again:
                // a 404 stays a 404, a changed file stays changed, and a disk
                // that cannot be written stays unwritable.
                if error.kind != FailureKind::Transient {
                    download.set_seg_status(seg_id, SegmentStatus::Failed);
                    return Err(format!("segment {seg_id} failed: {}", error.message));
                }
                attempt += 1;
                if attempt >= download.max_attempts.load(Ordering::SeqCst) {
                    download.set_seg_status(seg_id, SegmentStatus::Failed);
                    return Err(format!(
                        "segment {seg_id} failed after {attempt} attempts: {}",
                        error.message
                    ));
                }
                // The server may tell us how long to wait; otherwise the wait
                // grows with every attempt.
                download.set_seg_status(seg_id, SegmentStatus::Retrying);
                emit_throttled(&reporter, &download, true);
                let wait = error.retry_after.unwrap_or_else(|| http::backoff(attempt));
                if !wait_while_active(&download, epoch, wait).await {
                    return Ok(());
                }
            }
        }
    }
}

/// Join every part file into the destination file and rename it into place.
async fn assemble(download: &Arc<Download>) -> Result<(), String> {
    let segments = download
        .segments
        .lock()
        .expect("segments lock poisoned")
        .clone();
    let dest = download.dest.clone();
    let tmp = download.dir.join(format!("{}.idmtmp", download.stem));

    // A file whose size the server never told us is held in one part file
    // from the first byte to the end, so there is nothing to join.
    if segments.iter().any(|s| s.is_unknown()) {
        let Some(segment) = segments.first() else {
            return Err("the download has no parts".to_string());
        };
        // The part is about to become the destination, so its bytes have to
        // reach the disk before its name does.
        OpenOptions::new()
            .write(true)
            .open(&segment.part)
            .and_then(|file| file.sync_all())
            .map_err(|e| format!("{}: {e}", segment.part.display()))?;
        guard_replace(download)?;
        disk::replace_file(&segment.part, &dest).map_err(|e| e.to_string())?;
        cleanup(download);
        return Ok(());
    }

    let join_target = tmp.clone();
    let join_segments = segments.clone();
    let total = download.total.load(Ordering::SeqCst);
    let joined = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut out = File::create(&join_target).map_err(|e| e.to_string())?;

        for segment in &join_segments {
            let mut file = File::open(&segment.part)
                .map_err(|e| format!("{}: {e}", segment.part.display()))?;
            let mut remaining = segment.len();
            let mut buffer = vec![0u8; COPY_BUF];

            while remaining > 0 {
                let want = remaining.min(COPY_BUF as u64) as usize;
                let read = file.read(&mut buffer[..want]).map_err(|e| e.to_string())?;
                if read == 0 {
                    return Err(format!(
                        "{} is shorter than the range it should hold",
                        segment.part.display()
                    ));
                }
                out.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
                remaining -= read as u64;
            }
        }

        out.flush().map_err(|e| e.to_string())?;
        // The rename makes the joined file the destination. The bytes behind
        // that name have to be on the disk before it is, or a crash leaves a
        // destination that is shorter than the file it claims to be.
        out.sync_all().map_err(|e| e.to_string())?;

        // The file that is about to replace the destination has to be the
        // size the server promised, or it is not the file that was asked for.
        if total > 0 {
            let length = out.metadata().map_err(|e| e.to_string())?.len();
            if length != total {
                return Err(format!(
                    "the assembled file is {length} bytes, the server promised {total}"
                ));
            }
        }

        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?;

    if let Err(error) = joined {
        // The destination is untouched: the half written file only ever
        // lived in the temporary file.
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }

    guard_replace(download)?;
    if let Err(error) = disk::replace_file(&tmp, &dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error.to_string());
    }

    cleanup(download);
    Ok(())
}

/// Refuse to put the finished file over one that was not there when the
/// download started.
///
/// A Replace was confirmed before the transfer began, so it is the one case
/// where the destination may already hold a file. Everything else leaves a
/// file that appeared meanwhile alone, and reports why.
fn guard_replace(download: &Arc<Download>) -> Result<(), String> {
    if download.dest.exists() && !download.allow_replace.load(Ordering::SeqCst) {
        return Err(format!(
            "{} appeared while the download was running; it was not replaced",
            download.dest.display()
        ));
    }
    Ok(())
}

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
fn cleanup(download: &Arc<Download>) {
    remove_orphan_parts(&download.dir, &download.stem, &[]);
    let _ = std::fs::remove_file(&download.manifest);
}

/// Check that the destination volume still has room for the download.
fn check_free_space(download: &Arc<Download>, total: Option<u64>) -> Result<(), String> {
    let Some(free) = disk::free_space(&download.dest) else {
        return Ok(());
    };
    if free < DISK_MARGIN {
        return Err("the disk is full".to_string());
    }
    if let Some(total) = total {
        if free < total + DISK_MARGIN {
            return Err(format!(
                "not enough space on the disk: {free} bytes free, {} needed",
                total + DISK_MARGIN
            ));
        }
    }
    Ok(())
}

/// Ask the server how big the file is, and ask again when it says no.
///
/// A server that is busy answers `429`; the answer carries a body, and that
/// body is not the size of the file. The probe therefore fails on such an
/// answer, and the download waits and asks again.
async fn probe_with_retries(download: &Arc<Download>, epoch: u64) -> Result<Probe, String> {
    let mut attempt = 0u32;

    loop {
        if download.is_cancelled(epoch) {
            return Err("cancelled".to_string());
        }
        match download.source.probe().await {
            Ok(probe) => return Ok(probe),
            Err(error) => {
                attempt += 1;
                if !error.is_transient() || attempt >= download.max_attempts.load(Ordering::SeqCst)
                {
                    return Err(error.message);
                }
                let wait = error.retry_after.unwrap_or_else(|| http::backoff(attempt));
                if !wait_while_active(download, epoch, wait).await {
                    return Err("cancelled".to_string());
                }
            }
        }
    }
}

async fn drive(
    reporter: &Arc<dyn Reporter>,
    limits: &Limits,
    download: &Arc<Download>,
    epoch: u64,
) -> Result<(), String> {
    // A pause that arrived between the command that started this run and the
    // run itself holds the connections back; the status says so from the
    // first event instead of claiming traffic that is not moving.
    download.set_status(if download.control.is_paused() {
        Status::Paused
    } else {
        Status::Running
    });
    emit(reporter, download);
    log::write(
        "info",
        &format!(
            "starting {} -> {} with {} connections",
            download.url,
            download.dest.display(),
            download.max_connections
        ),
    );

    let probe = probe_with_retries(download, epoch).await?;

    // The validator of the probe travels with every range as `If-Range`, so
    // a file that changes between the probe and a range is refused instead of
    // being mixed into the bytes already on disk.
    download.set_validator(probe.validator());

    // A file can be resumed when the server honours ranges and either knows
    // how big the file is or gives a validator that proves the bytes on disk
    // are still the first bytes of the file it serves now.
    download.set_resumable(probe.ranges && (probe.total.is_some() || probe.validator().is_some()));

    // A server that answers a range request with 200 cannot be segmented.
    let host = crate::limits::host_of(&download.url);
    let site_max = host
        .as_deref()
        .map(|host| limits.sites.resolve(host, download.max_connections))
        .unwrap_or(download.max_connections);
    let allowed = site_max.min(download.max_connections);
    // The number of connections a file deserves: one per 10 KB that is left,
    // the way the reference binary counts them.
    let max_connections = if probe.ranges {
        match probe.total {
            Some(total) => segmenter::connections_for(total, allowed),
            None => 1,
        }
    } else {
        1
    };

    check_free_space(download, probe.total)?;

    download
        .total
        .store(probe.total.unwrap_or(0), Ordering::SeqCst);

    let segments = restore(download, &probe, max_connections);
    save_manifest(download);
    emit(reporter, download);
    log::write(
        "info",
        &format!(
            "the file is {} bytes in {} ranges, the server honours ranges: {}",
            probe
                .total
                .map(|total| total.to_string())
                .unwrap_or_else(|| "of an unknown size".to_string()),
            segments.len(),
            probe.ranges
        ),
    );

    let mut set = tokio::task::JoinSet::new();
    for segment in &segments {
        set.spawn(segment_task(
            reporter.clone(),
            limits.clone(),
            download.clone(),
            segment.id,
            epoch,
        ));
    }

    let mut failure: Option<String> = None;
    let mut last_space_check = Instant::now();
    let mut last_manifest_save = Instant::now();
    loop {
        tokio::select! {
            joined = set.join_next() => {
                match joined {
                    Some(Ok(Ok(()))) => {}
                    Some(Ok(Err(error))) => {
                        if error.contains(CORRUPT_MESSAGE) || error.contains(RANGE_IGNORED_MESSAGE) {
                            // The server sent bytes that are not the range
                            // that was asked for. The download goes on with a
                            // single connection, which asks for the whole
                            // file and cannot be given the wrong part of it.
                            let already_degraded =
                                download.degraded.load(Ordering::SeqCst);
                            if !already_degraded {
                                if let Some(segment) =
                                    downgrade_to_one_connection(download, probe.total)
                                {
                                    set.spawn(segment_task(
                                        reporter.clone(),
                                        limits.clone(),
                                        download.clone(),
                                        segment.id,
                                        epoch,
                                    ));
                                    continue;
                                }
                            }
                        }
                        // A task of the plan that was given up reports the
                        // rollback of its range, not a failure of the
                        // download.
                        if error.contains(ROLLED_BACK_MESSAGE) {
                            continue;
                        }
                        failure = Some(error);
                        download.stop();
                    }
                    Some(Err(error)) => {
                        failure = Some(error.to_string());
                        download.stop();
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(300)) => {
                // A paused download writes nothing, so it has no new resume
                // state to record, no new connection to open, and no new
                // reason to look at the free space of the disk.
                if !download.control.is_paused() {
                    maybe_grow(reporter, limits, download, &mut set, epoch);
                    // The manifest is the resume state. Writing it on every
                    // tick would rewrite the same JSON several times a second
                    // for the length of the download; a couple of seconds of
                    // lost progress after a crash is the cheaper trade.
                    if last_manifest_save.elapsed() >= MANIFEST_INTERVAL {
                        last_manifest_save = Instant::now();
                        save_manifest(download);
                    }
                    if last_space_check.elapsed() >= SPACE_CHECK_INTERVAL {
                        last_space_check = Instant::now();
                        if let Err(error) = check_free_space(download, probe.total) {
                            failure = Some(error);
                            download.stop();
                        }
                    }
                }
                // The bytes that were in flight when the pause arrived still
                // land, so the count keeps being reported while paused.
                emit_throttled(reporter, download, false);
            }
        }

        if failure.is_some() || set.is_empty() {
            break;
        }
    }

    if let Some(error) = failure {
        return Err(error);
    }
    if download.is_cancelled(epoch) {
        return Err("cancelled".to_string());
    }

    assemble(download).await?;
    Ok(())
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

#[cfg(test)]
mod tests;
