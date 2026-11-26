//! Downloading one track: its init segment and every segment after it.
//!
//! A track is not a new kind of download. Each segment is fetched by the same
//! engine the rest of the application uses: a plain segment is an ordinary
//! [`crate::downloader::Download`] with its own part files and manifest, so a
//! cancelled run resumes from the bytes on disk. A segment whose manifest gave
//! a byte range is fetched through the engine's ranged transfer, into a file
//! of its own that resumes from its length.
//!
//! The finished track is the segments joined in order, in the unfinished
//! folder. The media layer above (the app) hands that file to the remuxer; this
//! module never looks inside the bytes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::{stream, StreamExt};
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::disk;
use crate::downloader::{self, Download, DownloadInfo, Reporter, Status};
use crate::http;
use crate::limits::Limits;
use crate::log;
use crate::transfer::{FailureKind, Source, TransferError};

use super::{TrackSegment, MAX_SEGMENTS};

/// What one track download ended with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackOutcome {
    Done,
    Cancelled,
}

/// Why a track could not be downloaded, in the form a caller can act on.
///
/// A message string cannot answer the question a retry has to ask: was this
/// refusal the address's fault (ask the provider for a new one), the network's
/// (ask again), the disk's, or a body that is not the media the job wanted?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackProblem {
    /// The server refused the address, the identity it was signed for, or the
    /// range asked for. Only a new address can answer this.
    Denied,
    /// The bytes behind the address are not the bytes that were expected:
    /// another size, another validator, another representation.
    Changed,
    /// Asking again may work: a busy or unreachable server, a short read.
    Transient,
    /// What came back is not the media the job asked for.
    Media,
    /// The local disk could not hold the bytes.
    Storage,
}

impl TrackProblem {
    pub fn name(self) -> &'static str {
        match self {
            TrackProblem::Denied => "denied",
            TrackProblem::Changed => "changed",
            TrackProblem::Transient => "transient",
            TrackProblem::Media => "media",
            TrackProblem::Storage => "storage",
        }
    }
}

/// A failed track: what kind of failure it was, and the words for the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackError {
    pub problem: TrackProblem,
    pub message: String,
}

impl TrackError {
    pub fn new(problem: TrackProblem, message: impl Into<String>) -> Self {
        Self {
            problem,
            message: message.into(),
        }
    }

    pub fn denied(message: impl Into<String>) -> Self {
        Self::new(TrackProblem::Denied, message)
    }

    pub fn changed(message: impl Into<String>) -> Self {
        Self::new(TrackProblem::Changed, message)
    }

    pub fn transient(message: impl Into<String>) -> Self {
        Self::new(TrackProblem::Transient, message)
    }

    pub fn media(message: impl Into<String>) -> Self {
        Self::new(TrackProblem::Media, message)
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::new(TrackProblem::Storage, message)
    }

    /// The failure a refusal status means, with the provider's words kept.
    pub fn refused(status: u16, message: impl Into<String>) -> Self {
        match status {
            401 | 403 | 404 | 410 | 416 => Self::denied(message),
            _ => Self::transient(message),
        }
    }

    /// The same problem, said with a longer sentence.
    pub fn wrapped(self, prefix: impl std::fmt::Display) -> Self {
        Self {
            problem: self.problem,
            message: format!("{prefix}{}", self.message),
        }
    }

    /// Whether a new address is the only answer.
    pub fn is_denial(&self) -> bool {
        self.problem == TrackProblem::Denied
    }
}

impl std::fmt::Display for TrackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TrackError {}

impl From<String> for TrackError {
    fn from(message: String) -> Self {
        Self::transient(message)
    }
}

impl From<&str> for TrackError {
    fn from(message: &str) -> Self {
        Self::transient(message.to_string())
    }
}

/// The numbers the window shows while a track downloads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackProgress {
    pub downloaded: u64,
    /// The bytes the segments promise. Zero until the first one is probed.
    pub total: u64,
    pub segments_done: usize,
    pub segments_total: usize,
}

/// Where a finished track is reported.
pub type ProgressSink = Arc<dyn Fn(TrackProgress) + Send + Sync>;

/// Stops a track between segments, and the segments that are in flight.
pub struct TrackControl {
    cancelled: AtomicBool,
    /// Every fragment being fetched right now. Fragments are fetched side by
    /// side, so this is a list and not one slot: a cancel that reached only
    /// the last one would leave the others running.
    current: Mutex<Vec<Arc<Download>>>,
}

impl Default for TrackControl {
    fn default() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            current: Mutex::new(Vec::new()),
        }
    }
}

impl TrackControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        let running = self.current.lock().expect("the track control").clone();
        for download in running {
            download.cancel();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Hold a fragment that is running so a cancel reaches it. `false` means
    /// the cancel arrived first.
    fn attach(&self, download: &Arc<Download>) -> bool {
        if self.is_cancelled() {
            return false;
        }
        let mut current = self.current.lock().expect("the track control");
        // A fragment that is no longer running cannot be stopped: a track of
        // hundreds of them must not collect one entry per fragment.
        current.retain(|held| held.is_running());
        current.push(download.clone());
        drop(current);
        !self.is_cancelled()
    }

    fn detach(&self, download: &Arc<Download>) {
        self.current
            .lock()
            .expect("the track control")
            .retain(|held| !Arc::ptr_eq(held, download));
    }
}

/// One track to download.
pub struct TrackSource {
    /// The init segment, when the playlist named one. It comes first.
    pub init: Option<TrackSegment>,
    pub segments: Vec<TrackSegment>,
    /// Where the joined track is written, in the unfinished folder.
    pub dest: PathBuf,
    /// Where the segment files and their part files live.
    pub work_dir: PathBuf,
    /// The prefix of the segment file names. It is a bare name, never a path.
    pub stem: String,
    pub connections: u32,
    pub proxy: Option<String>,
    pub referer: Option<String>,
    /// Extra headers the page's browser sent, already filtered to the allowed
    /// names. They are attached only to addresses on `header_origin`.
    pub headers: Vec<(String, String)>,
    pub header_origin: Option<String>,
    /// Which representation these bytes are, when the caller can name it.
    ///
    /// The address of a track changes on every refresh (a new signature, a
    /// new CDN host) while the representation stays the same; the reverse also
    /// happens, and that is the dangerous one: a refresh that lands on another
    /// encode must never be resumed from the bytes of the old one. The
    /// fingerprint is the only fact that can tell the two apart, so it is
    /// written beside the bytes and compared before anything is fetched.
    pub fingerprint: Option<String>,
}

impl TrackSource {
    fn segment_file(&self, index: usize) -> PathBuf {
        self.work_dir.join(format!("{}.seg{index}", self.stem))
    }

    fn init_file(&self) -> PathBuf {
        self.work_dir.join(format!("{}.init", self.stem))
    }

    /// Where the fingerprint of the representation on disk is written.
    fn representation_file(&self) -> PathBuf {
        self.work_dir.join(format!("{}.track-id", self.stem))
    }
}

/// Download every segment of one track and join them into `dest`.
///
/// A segment already on disk is not fetched again, which is what makes a
/// cancelled track resume: the engine kept its parts, and a completed segment
/// only ever appears under its final name.
pub async fn download_track(
    limits: Limits,
    source: TrackSource,
    control: Option<Arc<TrackControl>>,
    progress: Option<ProgressSink>,
) -> Result<TrackOutcome, String> {
    download_track_typed(limits, source, control, progress)
        .await
        .map_err(|error| error.to_string())
}

/// The same track download, reporting why it failed rather than only what to
/// show the user.
pub async fn download_track_typed(
    limits: Limits,
    source: TrackSource,
    control: Option<Arc<TrackControl>>,
    progress: Option<ProgressSink>,
) -> Result<TrackOutcome, TrackError> {
    let mut source = source;
    source.stem = safe_stem(&source.stem);
    if source.segments.is_empty() {
        return Err(TrackError::media("the track has no segments"));
    }
    if source.segments.len() > MAX_SEGMENTS {
        return Err(TrackError::media(format!(
            "the track has more than {MAX_SEGMENTS} segments"
        )));
    }
    std::fs::create_dir_all(&source.work_dir).map_err(|error| {
        TrackError::storage(format!("the unfinished folder could not be made: {error}"))
    })?;

    // Before a single byte is read or written: do the bytes on disk belong to
    // the representation this run is about to fetch? The answer is written
    // beside them, so a refresh that lands on another encode drops the old
    // bytes instead of joining two encodes into one file.
    take_representation(&source)?;

    if source.dest.is_file() {
        // A previous run finished the join; the caller can use it. The guard
        // above has already proved it is the same representation.
        call(
            &progress,
            TrackProgress {
                downloaded: source.dest.metadata().map(|m| m.len()).unwrap_or(0),
                total: source.dest.metadata().map(|m| m.len()).unwrap_or(0),
                segments_done: source.segments.len(),
                segments_total: source.segments.len(),
            },
        );
        return Ok(TrackOutcome::Done);
    }

    // A protected init segment is the one place a manifest cannot tell DRM
    // from clear content. Read the box headers once it is on disk and refuse
    // before a single media segment is fetched.
    if source.init.is_some() {
        let file = source.init_file();
        if file.is_file() {
            if let Some(head) = read_head(&file, 8192) {
                if let Some(scheme) = super::protection_in(&head) {
                    return Err(TrackError::media(
                        super::MediaError::protected(scheme).to_string(),
                    ));
                }
            }
        }
    }

    let state = Arc::new(Mutex::new(ProgressState::new(source.segments.len())));

    // The init segment is fetched like any other: it is one address with one
    // set of bytes, and its absence is an error not a silent skip.
    if let Some(init) = &source.init {
        let file = source.init_file();
        let expected = init.range.map(|(start, end)| end - start + 1);
        let already = match expected {
            Some(expected) => file_len(&file) == Some(expected),
            None => file.is_file(),
        };
        if !already {
            let result = fetch_segment(
                &limits,
                init,
                &file,
                source.segments.len(),
                &state,
                &progress,
                control.as_deref(),
                source.proxy.as_deref(),
                source.referer.as_deref(),
                source.connections,
                &source.headers,
                source.header_origin.as_deref(),
            )
            .await;
            match result {
                Ok(()) => {}
                Err(FetchFailure::Cancelled) => return Ok(TrackOutcome::Cancelled),
                Err(FetchFailure::Failed(error)) => {
                    return Err(error.wrapped("the init segment failed: "))
                }
            }
        }
    }

    if source.init.is_some() {
        let file = source.init_file();
        if let Some(head) = read_head(&file, 8192) {
            if let Some(scheme) = super::protection_in(&head) {
                return Err(TrackError::media(
                    super::MediaError::protected(scheme).to_string(),
                ));
            }
        }
    }

    // The fragments of a track are separate files, so they are fetched beside
    // each other: one connection each, and as many at once as the track was
    // given connections. Fetching them one after another made a segmented
    // video take the sum of every fragment's latency, which is why a measure
    // of the same link may crawl here while the player itself fills it. Every
    // fragment already has its own file and its own resume point, so the
    // joined track is the same whichever order they finish in.
    let mut waiting: Vec<(usize, &TrackSegment)> = Vec::new();
    for (index, segment) in source.segments.iter().enumerate() {
        if is_cancelled(control.as_deref()) {
            return Ok(TrackOutcome::Cancelled);
        }
        let file = source.segment_file(index);
        let expected = segment.range.map(|(start, end)| end - start + 1);
        let complete = match expected {
            Some(expected) => file_len(&file) == Some(expected),
            // Without a range the engine writes the file only after the whole
            // segment is assembled, so its presence proves it is complete.
            None => file.is_file(),
        };
        if complete {
            mark_done(
                &state,
                index,
                expected.unwrap_or_else(|| file_len(&file).unwrap_or(0)),
            );
            report(&state, &progress);
            continue;
        }
        waiting.push((index, segment));
    }

    // A single fragment keeps every connection the track was given, because
    // there is nothing beside it to share them with.
    let width = source.connections.clamp(1, 32) as usize;
    let per_fragment = if source.segments.len() > 1 {
        1
    } else {
        source.connections
    };

    // The futures are built one by one rather than through a closure: a
    // closure that borrows the track is not general enough over lifetimes to
    // be spawned by the callers that run a track on their own task.
    let mut fragments = Vec::with_capacity(waiting.len());
    for (index, segment) in waiting {
        fragments.push(fetch_fragment(
            &limits,
            &source,
            segment,
            index,
            &state,
            &progress,
            control.as_deref(),
            per_fragment,
        ));
    }
    let mut running = stream::iter(fragments).buffer_unordered(width);
    while let Some((index, result)) = running.next().await {
        match result {
            Ok(()) => report(&state, &progress),
            Err(FetchFailure::Cancelled) => return Ok(TrackOutcome::Cancelled),
            Err(FetchFailure::Failed(error)) => {
                return Err(error.wrapped(format!("segment {index} failed: ")))
            }
        }
    }

    if is_cancelled(control.as_deref()) {
        return Ok(TrackOutcome::Cancelled);
    }

    let mut files = Vec::with_capacity(source.segments.len() + 1);
    if source.init.is_some() {
        files.push(source.init_file());
    }
    for index in 0..source.segments.len() {
        files.push(source.segment_file(index));
    }
    join(&files, &source.dest).map_err(TrackError::storage)?;

    // Only now is the joined file the track; the pieces can go.
    let _ = std::fs::remove_file(source.init_file());
    for index in 0..source.segments.len() {
        let file = source.segment_file(index);
        let _ = std::fs::remove_file(&file);
        // The engine names its manifest after the destination file.
        let _ = std::fs::remove_file(manifest_of(&file));
    }

    call(
        &progress,
        TrackProgress {
            downloaded: source.dest.metadata().map(|m| m.len()).unwrap_or(0),
            total: source.dest.metadata().map(|m| m.len()).unwrap_or(0),
            segments_done: source.segments.len(),
            segments_total: source.segments.len(),
        },
    );

    Ok(TrackOutcome::Done)
}

/// Drop every byte on disk that belongs to another representation, and record
/// which representation these bytes are.
///
/// A track is fetched in pieces, so a run can be stopped and started many
/// times over hours; the address is renewed in between and may land on a
/// different encode of the same picture. Joining the two would produce a file
/// whose second half is a different video, which is far worse than downloading
/// the track again. The sidecar is written *before* the first fetch, so even a
/// run that dies immediately leaves a truthful note behind.
fn take_representation(source: &TrackSource) -> Result<(), TrackError> {
    let Some(fingerprint) = source.fingerprint.as_deref() else {
        return Ok(());
    };
    let sidecar = source.representation_file();
    let known = std::fs::read_to_string(&sidecar)
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    if known.as_deref() == Some(fingerprint) {
        return Ok(());
    }
    let dropped = clear_track_files(source);
    std::fs::write(&sidecar, fingerprint).map_err(|error| {
        TrackError::storage(format!(
            "the note of which track these bytes are could not be written: {error}"
        ))
    })?;
    if dropped > 0 {
        log::write(
            "warn",
            &format!(
                "the bytes of another representation were dropped ({dropped} files) before {} was fetched again",
                source.stem
            ),
        );
    }
    Ok(())
}

/// Remove the init segment, every segment and the joined track, with the
/// engine's part files and manifests. Returns how many files went.
fn clear_track_files(source: &TrackSource) -> usize {
    let mut dropped = 0usize;
    let init_prefix = format!("{}.init", source.stem);
    let segment_prefix = format!("{}.seg", source.stem);
    if let Ok(entries) = std::fs::read_dir(&source.work_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(&init_prefix) && !name.starts_with(&segment_prefix) {
                continue;
            }
            if std::fs::remove_file(entry.path()).is_ok() {
                dropped += 1;
            }
        }
    }
    if std::fs::remove_file(&source.dest).is_ok() {
        dropped += 1;
    }
    if std::fs::remove_file(join_temporary(&source.dest)).is_ok() {
        dropped += 1;
    }
    dropped
}

#[derive(Default)]
struct ProgressState {
    /// `(downloaded, total)` per segment index. The total is zero until the
    /// engine has probed that segment.
    segments: HashMap<usize, (u64, u64)>,
    done: usize,
    count: usize,
}

impl ProgressState {
    fn new(count: usize) -> Self {
        Self {
            segments: HashMap::new(),
            done: 0,
            count,
        }
    }

    fn progress(&self) -> TrackProgress {
        let mut downloaded = 0u64;
        let mut total = 0u64;
        for (have, want) in self.segments.values() {
            downloaded += *have;
            total += *want;
        }
        TrackProgress {
            downloaded,
            total,
            segments_done: self.done,
            segments_total: self.count,
        }
    }
}

struct SegmentReporter {
    state: Arc<Mutex<ProgressState>>,
    sink: Option<ProgressSink>,
}

impl Reporter for SegmentReporter {
    fn report(&self, info: &DownloadInfo) {
        let mut state = self.state.lock().expect("the track progress");
        let index = info.id as usize;
        let total = if info.total > 0 {
            info.total
        } else {
            state
                .segments
                .get(&index)
                .map(|(_, total)| *total)
                .unwrap_or(0)
        };
        let downloaded = if info.status == Status::Completed && total > 0 {
            total
        } else {
            info.downloaded
        };
        state.segments.insert(index, (downloaded, total));
        if info.status == Status::Completed {
            // Count the indexes that are complete, not every report.
            state.done = (0..state.count)
                .filter(|index| {
                    state
                        .segments
                        .get(index)
                        .is_some_and(|(have, want)| *want > 0 && have >= want)
                })
                .count();
        }
        let progress = state.progress();
        drop(state);
        call(&self.sink, progress);
    }
}

fn update(state: &Arc<Mutex<ProgressState>>, index: usize, downloaded: u64, total: u64) {
    let mut state = state.lock().expect("the track progress");
    state.segments.insert(index, (downloaded, total));
}

fn mark_done(state: &Arc<Mutex<ProgressState>>, index: usize, size: u64) {
    let mut state = state.lock().expect("the track progress");
    state.segments.insert(index, (size, size));
    state.done = (0..state.count)
        .filter(|index| {
            state
                .segments
                .get(index)
                .is_some_and(|(have, want)| *want > 0 && have >= want)
        })
        .count();
}

fn report(state: &Arc<Mutex<ProgressState>>, sink: &Option<ProgressSink>) {
    let progress = state.lock().expect("the track progress").progress();
    call(sink, progress);
}

fn call(sink: &Option<ProgressSink>, progress: TrackProgress) {
    if let Some(sink) = sink {
        sink(progress);
    }
}

/// The headers that may travel to this address: only the ones the page sent
/// for the manifest's own origin.
fn headers_for(
    url: &str,
    headers: &[(String, String)],
    origin: Option<&str>,
) -> Vec<(String, String)> {
    if headers.is_empty() {
        return Vec::new();
    }
    let Some(origin) = origin else {
        return Vec::new();
    };
    let (Ok(url), Ok(origin)) = (Url::parse(url), Url::parse(origin)) else {
        return Vec::new();
    };
    if url.origin() != origin.origin() {
        return Vec::new();
    }
    headers.to_vec()
}

fn is_cancelled(control: Option<&TrackControl>) -> bool {
    control.is_some_and(TrackControl::is_cancelled)
}

fn file_len(file: &Path) -> Option<u64> {
    std::fs::metadata(file).ok().map(|meta| meta.len())
}

fn read_head(file: &Path, limit: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut input = std::fs::File::open(file).ok()?;
    let mut buffer = vec![0u8; limit];
    let read = input.read(&mut buffer).ok()?;
    buffer.truncate(read);
    Some(buffer)
}

/// The engine names a manifest `<destination file name>.idm.json`; for a
/// segment destination that is the whole file name plus the suffix.
fn manifest_of(file: &Path) -> PathBuf {
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "segment".to_string());
    file.with_file_name(format!("{name}.idm.json"))
}

/// A prefix that cannot leave the unfinished folder.
fn safe_stem(stem: &str) -> String {
    let stem = stem.trim();
    let safe: String = stem
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '"' | '<' | '>' | '|' | '?' => '_',
            other => other,
        })
        .collect();
    if safe.is_empty() || safe == "." || safe == ".." {
        "track".to_string()
    } else {
        safe
    }
}

/// Fetch one fragment of a track, reporting its index with the outcome.
#[allow(clippy::too_many_arguments)]
async fn fetch_fragment(
    limits: &Limits,
    source: &TrackSource,
    segment: &TrackSegment,
    index: usize,
    state: &Arc<Mutex<ProgressState>>,
    progress: &Option<ProgressSink>,
    control: Option<&TrackControl>,
    connections: u32,
) -> (usize, Result<(), FetchFailure>) {
    let file = source.segment_file(index);
    let result = fetch_segment(
        limits,
        segment,
        &file,
        index,
        state,
        progress,
        control,
        source.proxy.as_deref(),
        source.referer.as_deref(),
        connections,
        &source.headers,
        source.header_origin.as_deref(),
    )
    .await;
    (index, result)
}

mod segment;

use segment::{fetch_segment, join, join_temporary, FetchFailure};

#[cfg(test)]
mod tests;
