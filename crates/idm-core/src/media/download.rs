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

use futures_util::StreamExt;
use reqwest::Url;

use crate::disk;
use crate::downloader::{self, Download, DownloadInfo, Reporter, Status};
use crate::http;
use crate::limits::Limits;
use crate::transfer::Source;

use super::{TrackSegment, MAX_SEGMENTS};

/// What one track download ended with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackOutcome {
    Done,
    Cancelled,
}

/// The numbers the window shows while a track downloads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrackProgress {
    pub downloaded: u64,
    /// The bytes the segments promise. Zero until the first one is probed.
    pub total: u64,
    pub segments_done: usize,
    pub segments_total: usize,
}

/// Where a finished track is reported.
pub type ProgressSink = Arc<dyn Fn(TrackProgress) + Send + Sync>;

/// Stops a track between segments, and the segment that is in flight.
pub struct TrackControl {
    cancelled: AtomicBool,
    current: Mutex<Option<Arc<Download>>>,
}

impl Default for TrackControl {
    fn default() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            current: Mutex::new(None),
        }
    }
}

impl TrackControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(download) = self
            .current
            .lock()
            .expect("the track control")
            .as_ref()
            .cloned()
        {
            download.cancel();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Hold the segment that is running so a cancel reaches it. `false` means
    /// the cancel arrived first.
    fn attach(&self, download: &Arc<Download>) -> bool {
        if self.is_cancelled() {
            return false;
        }
        *self.current.lock().expect("the track control") = Some(download.clone());
        !self.is_cancelled()
    }

    fn detach(&self) {
        *self.current.lock().expect("the track control") = None;
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
}

impl TrackSource {
    fn segment_file(&self, index: usize) -> PathBuf {
        self.work_dir.join(format!("{}.seg{index}", self.stem))
    }

    fn init_file(&self) -> PathBuf {
        self.work_dir.join(format!("{}.init", self.stem))
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
    let mut source = source;
    source.stem = safe_stem(&source.stem);
    if source.segments.is_empty() {
        return Err("the track has no segments".to_string());
    }
    if source.segments.len() > MAX_SEGMENTS {
        return Err(format!("the track has more than {MAX_SEGMENTS} segments"));
    }
    std::fs::create_dir_all(&source.work_dir)
        .map_err(|error| format!("the unfinished folder could not be made: {error}"))?;

    if source.dest.is_file() {
        // A previous run finished the join; the caller can use it.
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
                if super::looks_protected(&head) {
                    return Err(super::MediaError::Protected.to_string());
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
                Err(FetchFailure::Failed(message)) => {
                    return Err(format!("the init segment failed: {message}"))
                }
            }
        }
    }

    if source.init.is_some() {
        let file = source.init_file();
        if let Some(head) = read_head(&file, 8192) {
            if super::looks_protected(&head) {
                return Err(super::MediaError::Protected.to_string());
            }
        }
    }

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

        let result = fetch_segment(
            &limits,
            segment,
            &file,
            index,
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
            Ok(()) => {
                report(&state, &progress);
            }
            Err(FetchFailure::Cancelled) => return Ok(TrackOutcome::Cancelled),
            Err(FetchFailure::Failed(message)) => {
                return Err(format!("segment {index} failed: {message}"))
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
    join(&files, &source.dest)?;

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

enum FetchFailure {
    Cancelled,
    Failed(String),
}

/// Fetch one segment, by the engine's whole-file download or its ranged
/// transfer, depending on what the manifest said.
#[allow(clippy::too_many_arguments)]
async fn fetch_segment(
    limits: &Limits,
    segment: &TrackSegment,
    file: &Path,
    index: usize,
    state: &Arc<Mutex<ProgressState>>,
    progress: &Option<ProgressSink>,
    control: Option<&TrackControl>,
    proxy: Option<&str>,
    referer: Option<&str>,
    connections: u32,
    headers: &[(String, String)],
    header_origin: Option<&str>,
) -> Result<(), FetchFailure> {
    let headers = headers_for(&segment.url, headers, header_origin);
    match segment.range {
        Some(range) => {
            fetch_window(
                segment, file, range, index, state, progress, control, proxy, referer, &headers,
            )
            .await
        }
        None => {
            fetch_whole(
                limits,
                segment,
                file,
                index,
                state,
                progress,
                control,
                proxy,
                referer,
                connections,
                &headers,
            )
            .await
        }
    }
}

/// A segment whose playlist gave no byte range: an ordinary download.
#[allow(clippy::too_many_arguments)]
async fn fetch_whole(
    limits: &Limits,
    segment: &TrackSegment,
    file: &Path,
    index: usize,
    state: &Arc<Mutex<ProgressState>>,
    progress: &Option<ProgressSink>,
    control: Option<&TrackControl>,
    proxy: Option<&str>,
    referer: Option<&str>,
    connections: u32,
    headers: &[(String, String)],
) -> Result<(), FetchFailure> {
    let download = Download::new_in_with_headers(
        index as u64,
        segment.url.clone(),
        file.to_path_buf(),
        file.parent(),
        connections.clamp(1, 32),
        proxy,
        referer,
        headers,
    )
    .map_err(FetchFailure::Failed)?;

    if let Some(control) = control {
        if !control.attach(&download) {
            return Err(FetchFailure::Cancelled);
        }
    }

    let reporter: Arc<dyn Reporter> = Arc::new(SegmentReporter {
        state: state.clone(),
        sink: progress.clone(),
    });
    downloader::run(reporter, limits.clone(), download.clone()).await;

    if let Some(control) = control {
        control.detach();
    }
    if download.status() == Status::Cancelled || is_cancelled(control) {
        return Err(FetchFailure::Cancelled);
    }
    if download.status() != Status::Completed {
        return Err(FetchFailure::Failed(
            download
                .info()
                .error
                .unwrap_or_else(|| "the segment did not finish".to_string()),
        ));
    }

    mark_done(state, index, download.info().total);
    Ok(())
}

/// A segment with a byte range into a larger resource.
///
/// The engine's transfer is used directly: there is no whole file to plan, and
/// the part file is the segment file, so its length is the resume point.
#[allow(clippy::too_many_arguments)]
async fn fetch_window(
    segment: &TrackSegment,
    file: &Path,
    range: (u64, u64),
    index: usize,
    state: &Arc<Mutex<ProgressState>>,
    progress: &Option<ProgressSink>,
    control: Option<&TrackControl>,
    proxy: Option<&str>,
    referer: Option<&str>,
    headers: &[(String, String)],
) -> Result<(), FetchFailure> {
    let (start, end) = range;
    if end < start {
        return Err(FetchFailure::Failed(
            "the byte range ends before it starts".to_string(),
        ));
    }
    let expected = end - start + 1;
    let mut written = file_len(file).unwrap_or(0);
    if written == expected {
        mark_done(state, index, expected);
        return Ok(());
    }
    if written > expected {
        let _ = std::fs::remove_file(file);
        written = 0;
    }

    let source = Source::parse_with_headers(&segment.url, proxy, referer, headers)
        .map_err(FetchFailure::Failed)?;

    let mut attempt = 0u32;
    loop {
        if is_cancelled(control) {
            return Err(FetchFailure::Cancelled);
        }
        let from = start + written;
        if from > end {
            break;
        }
        match source.open(from, Some(end), false, None, None).await {
            Ok(mut stream) => {
                let mut out = std::fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(file)
                    .map_err(|error| FetchFailure::Failed(error.to_string()))?;
                use std::io::{Seek, SeekFrom, Write};
                out.seek(SeekFrom::Start(written))
                    .map_err(|error| FetchFailure::Failed(error.to_string()))?;

                while let Some(item) = stream.next().await {
                    if is_cancelled(control) {
                        return Err(FetchFailure::Cancelled);
                    }
                    let bytes = item.map_err(FetchFailure::Failed)?;
                    if bytes.is_empty() {
                        continue;
                    }
                    out.write_all(&bytes)
                        .map_err(|error| FetchFailure::Failed(error.to_string()))?;
                    written += bytes.len() as u64;
                    update(state, index, written.min(expected), expected);
                    report(state, progress);
                    if written >= expected {
                        break;
                    }
                }
                out.flush()
                    .map_err(|error| FetchFailure::Failed(error.to_string()))?;
                if written >= expected {
                    break;
                }
                // The response ended early; ask for the rest from where the
                // file stops, after a short wait.
                attempt += 1;
                if attempt >= 8 {
                    return Err(FetchFailure::Failed(format!(
                        "the response ended after {written} of {expected} bytes"
                    )));
                }
                tokio::time::sleep(http::backoff(attempt)).await;
            }
            Err(error) => {
                if !error.is_transient() {
                    return Err(FetchFailure::Failed(error.message));
                }
                attempt += 1;
                if attempt >= 8 {
                    return Err(FetchFailure::Failed(format!(
                        "the range failed after {attempt} attempts: {}",
                        error.message
                    )));
                }
                tokio::time::sleep(error.retry_after.unwrap_or_else(|| http::backoff(attempt)))
                    .await;
            }
        }
    }

    if file_len(file) != Some(expected) {
        return Err(FetchFailure::Failed(
            "the segment file is shorter than the range it should hold".to_string(),
        ));
    }
    mark_done(state, index, expected);
    Ok(())
}

/// Join the files in order into `dest`, through a temporary file.
fn join(files: &[PathBuf], dest: &Path) -> Result<(), String> {
    use std::io::{Read, Write};

    if let Some(parent) = dest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("the unfinished folder could not be made: {error}"))?;
    }
    let temporary = dest.with_file_name(format!(
        "{}.idmtrack",
        dest.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "track".to_string())
    ));
    {
        let mut out = std::fs::File::create(&temporary)
            .map_err(|error| format!("the track could not be written: {error}"))?;
        let mut buffer = vec![0u8; 1024 * 1024];
        for file in files {
            let mut input = std::fs::File::open(file)
                .map_err(|error| format!("{} could not be read: {error}", file.display()))?;
            loop {
                let read = input
                    .read(&mut buffer)
                    .map_err(|error| format!("{}: {error}", file.display()))?;
                if read == 0 {
                    break;
                }
                out.write_all(&buffer[..read])
                    .map_err(|error| format!("the track could not be written: {error}"))?;
            }
        }
        out.flush()
            .map_err(|error| format!("the track could not be written: {error}"))?;
        out.sync_all()
            .map_err(|error| format!("the track could not be written: {error}"))?;
    }
    disk::replace_file(&temporary, dest).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("the track could not be put in place: {error}")
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::fixtures;
    use crate::media::testserver::TestServer;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("idm-media-download-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        dir
    }

    fn server() -> TestServer {
        TestServer::start_with_files(vec![
            ("/v/init.mp4".to_string(), fixtures::init()),
            ("/v/seg0.m4s".to_string(), fixtures::segment("SEG0")),
            ("/v/seg1.m4s".to_string(), fixtures::segment("SEG1")),
        ])
    }

    fn track(server: &TestServer, dir: &Path) -> TrackSource {
        TrackSource {
            init: Some(TrackSegment {
                url: server.url("/v/init.mp4"),
                range: None,
            }),
            segments: vec![
                TrackSegment {
                    url: server.url("/v/seg0.m4s"),
                    range: None,
                },
                TrackSegment {
                    url: server.url("/v/seg1.m4s"),
                    range: None,
                },
            ],
            dest: dir.join("film.ts"),
            work_dir: dir.to_path_buf(),
            stem: "film".to_string(),
            connections: 2,
            proxy: None,
            referer: None,
            headers: Vec::new(),
            header_origin: None,
        }
    }

    #[tokio::test]
    async fn a_track_is_joined_in_order() {
        let dir = temp_dir("join");
        let server = server();
        let outcome = download_track(Limits::default(), track(&server, &dir), None, None)
            .await
            .expect("a track");
        assert_eq!(outcome, TrackOutcome::Done);

        let mut expected = fixtures::init();
        expected.extend(fixtures::segment("SEG0"));
        expected.extend(fixtures::segment("SEG1"));
        assert_eq!(
            std::fs::read(dir.join("film.ts")).expect("the track"),
            expected
        );
        assert!(!dir.join("film.init").exists(), "the pieces are gone");
        assert!(!dir.join("film.seg0").exists());
        assert!(!dir.join("film.seg1").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_cancelled_track_resumes_without_fetching_a_finished_segment_again() {
        let dir = temp_dir("resume");
        let server = server();
        let control = TrackControl::new();
        let stopper = control.clone();
        let sink: ProgressSink = Arc::new(move |progress| {
            if progress.segments_done >= 1 {
                stopper.cancel();
            }
        });

        let outcome = download_track(
            Limits::default(),
            track(&server, &dir),
            Some(control),
            Some(sink),
        )
        .await
        .expect("the first run");
        assert_eq!(outcome, TrackOutcome::Cancelled);
        assert!(
            !dir.join("film.ts").exists(),
            "no half track is put in place"
        );
        assert!(
            dir.join("film.seg0").is_file(),
            "the finished segment stays"
        );
        let first_count = server.count("/v/seg0.m4s");

        let outcome = download_track(Limits::default(), track(&server, &dir), None, None)
            .await
            .expect("the resumed run");
        assert_eq!(outcome, TrackOutcome::Done);
        assert_eq!(
            server.count("/v/seg0.m4s"),
            first_count,
            "a finished segment is not fetched again"
        );

        let mut expected = fixtures::init();
        expected.extend(fixtures::segment("SEG0"));
        expected.extend(fixtures::segment("SEG1"));
        assert_eq!(
            std::fs::read(dir.join("film.ts")).expect("the track"),
            expected
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn byte_ranges_are_fetched_from_one_resource() {
        let dir = temp_dir("ranges");
        let bytes: Vec<u8> = (0..300u32).map(|value| (value % 251) as u8).collect();
        let server = TestServer::start_with_files(vec![("/all.ts".to_string(), bytes.clone())]);
        let url = server.url("/all.ts");

        let outcome = download_track(
            Limits::default(),
            TrackSource {
                init: None,
                segments: vec![
                    TrackSegment {
                        url: url.clone(),
                        range: Some((0, 99)),
                    },
                    TrackSegment {
                        url: url.clone(),
                        range: Some((100, 199)),
                    },
                    TrackSegment {
                        url: url.clone(),
                        range: Some((200, 299)),
                    },
                ],
                dest: dir.join("all.ts"),
                work_dir: dir.clone(),
                stem: "all".to_string(),
                connections: 2,
                proxy: None,
                referer: None,
                headers: Vec::new(),
                header_origin: None,
            },
            None,
            None,
        )
        .await
        .expect("a ranged track");
        assert_eq!(outcome, TrackOutcome::Done);
        assert_eq!(std::fs::read(dir.join("all.ts")).expect("the track"), bytes);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_protected_init_segment_is_refused_before_any_media() {
        let dir = temp_dir("protected");
        let server = TestServer::start_with_files(vec![
            ("/v/init.mp4".to_string(), b"    pssh    ".to_vec()),
            ("/v/seg0.m4s".to_string(), fixtures::segment("SEG0")),
        ]);
        let source = TrackSource {
            init: Some(TrackSegment {
                url: server.url("/v/init.mp4"),
                range: None,
            }),
            segments: vec![TrackSegment {
                url: server.url("/v/seg0.m4s"),
                range: None,
            }],
            dest: dir.join("film.mp4"),
            work_dir: dir.clone(),
            stem: "film".to_string(),
            connections: 2,
            proxy: None,
            referer: None,
            headers: Vec::new(),
            header_origin: None,
        };
        let error = download_track(Limits::default(), source, None, None)
            .await
            .expect_err("protected");
        assert!(error.contains("protected"), "the message was: {error}");
        assert!(!dir.join("film.mp4").exists());
        assert_eq!(
            server.count("/v/seg0.m4s"),
            0,
            "no media is fetched from a protected init"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_flaky_server_is_survived_and_no_part_files_are_left() {
        let dir = temp_dir("flaky");
        let server = server();
        server.fail_next("/v/init.mp4", 500);
        server.fail_next("/v/seg0.m4s", 500);
        server.drop_next("/v/seg1.m4s", 5);

        let outcome = download_track(Limits::default(), track(&server, &dir), None, None)
            .await
            .expect("the retries");
        assert_eq!(outcome, TrackOutcome::Done);

        let mut expected = fixtures::init();
        expected.extend(fixtures::segment("SEG0"));
        expected.extend(fixtures::segment("SEG1"));
        assert_eq!(
            std::fs::read(dir.join("film.ts")).expect("the track"),
            expected
        );

        for entry in std::fs::read_dir(&dir).expect("the folder").flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(
                !name.contains(".part") && !name.ends_with(".idm.json"),
                "an orphan resume file was left: {name}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_cookie_that_the_site_needs_is_sent_and_never_echoed() {
        let server = server();
        server.require("/v/seg0.m4s", "cookie", "session=abc");
        let origin = format!("http://127.0.0.1:{}", server.port());

        let dir = temp_dir("cookie");
        let mut source = track(&server, &dir);
        source.segments.truncate(1);
        source.headers = vec![("cookie".to_string(), "session=abc".to_string())];
        source.header_origin = Some(origin.clone());
        download_track(Limits::default(), source, None, None)
            .await
            .expect("the cookie is enough");
        let _ = std::fs::remove_dir_all(&dir);

        let dir = temp_dir("cookie-missing");
        let mut source = track(&server, &dir);
        source.segments.truncate(1);
        source.header_origin = Some(origin);
        let error = download_track(Limits::default(), source, None, None)
            .await
            .expect_err("the site refuses an anonymous request");
        assert!(error.contains("403"), "the message was: {error}");
        assert!(
            !error.contains("session=abc"),
            "the cookie value must not be echoed: {error}"
        );
        assert!(!dir.join("film.ts").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn headers_travel_only_to_their_origin() {
        let server = server();
        let origin = format!("http://127.0.0.1:{}", server.port());

        let dir = temp_dir("headers-same");
        let mut source = track(&server, &dir);
        source.segments.truncate(1);
        source.headers = vec![("cookie".to_string(), "a=b".to_string())];
        source.header_origin = Some(origin);
        download_track(Limits::default(), source, None, None)
            .await
            .expect("a track");
        assert_eq!(
            server.header("/v/seg0.m4s", "cookie").as_deref(),
            Some("a=b"),
            "the manifest's origin receives its cookie"
        );
        let _ = std::fs::remove_dir_all(&dir);

        let dir = temp_dir("headers-other");
        let mut source = track(&server, &dir);
        source.init = None;
        source.segments = vec![TrackSegment {
            url: server.url("/v/seg1.m4s"),
            range: None,
        }];
        source.headers = vec![("cookie".to_string(), "secret=1".to_string())];
        source.header_origin = Some("https://other.example".to_string());
        download_track(Limits::default(), source, None, None)
            .await
            .expect("a track");
        assert_eq!(
            server.header("/v/seg1.m4s", "cookie"),
            None,
            "another origin never receives the cookie"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_refused_segment_is_a_failure_and_puts_no_track_in_place() {
        let dir = temp_dir("refused");
        let server = server();
        server.fail("/v/seg1.m4s", 403);
        let error = download_track(Limits::default(), track(&server, &dir), None, None)
            .await
            .expect_err("a refused segment");
        assert!(error.contains("403"), "the message was: {error}");
        assert!(!dir.join("film.ts").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
