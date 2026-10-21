//! The download engine — a port of the mechanics observed in `IDMan.exe`.
//!
//! What the reverse engineering established (see `ANALYSIS.md` for the
//! evidence):
//!
//! * IDM speaks HTTP over **raw WinSock sockets** (`WS2_32.DLL::socket`,
//!   `recv`, `send`, `ioctlsocket`), not WinHTTP/WinInet, and it terminates TLS
//!   itself with OpenSSL. Every request carries a `Range: bytes=<start>-<end>`
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
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;

use crate::segmenter::{self, Segment, MIN_SPLIT};

/// IDM's default `MaxConnectionsNumber`, read from
/// `HKCU\Software\DownloadManager\MCN`. The observed default was 8.
pub const DEFAULT_CONNECTIONS: u32 = 8;

/// IDM reads a single HTTP header line into a 0x200 (512 byte) buffer.
#[allow(dead_code)]
pub const HEADER_LINE_BUF: usize = 0x200;

/// IDM's stack copy buffer used when pumping a socket into a file: 0x2da0.
pub const COPY_BUF: usize = 0x2da0;

/// IDM caps the in-memory chunk buffer at 0x600000 (6 MiB) and switches to
/// memory mapped I/O for larger chunks.
#[allow(dead_code)]
pub const CHUNK_MEM_LIMIT: u64 = 0x600000;

/// How many times one segment may fail before the download is failed.
const MAX_ATTEMPTS: u32 = 8;

/// Minimum interval between two progress events pushed to the UI.
const EMIT_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfo {
    pub id: u64,
    pub url: String,
    pub dest: String,
    pub total: u64,
    pub downloaded: u64,
    pub connections: u32,
    pub status: Status,
    pub error: Option<String>,
}

/// Pause / cancel state shared by every task of one download.
pub struct Control {
    paused_tx: watch::Sender<bool>,
    paused_rx: watch::Receiver<bool>,
    cancel_tx: watch::Sender<bool>,
    cancel_rx: watch::Receiver<bool>,
}

impl Control {
    fn new() -> Self {
        let (paused_tx, paused_rx) = watch::channel(false);
        let (cancel_tx, cancel_rx) = watch::channel(false);
        Self {
            paused_tx,
            paused_rx,
            cancel_tx,
            cancel_rx,
        }
    }

    pub fn pause(&self) {
        let _ = self.paused_tx.send(true);
    }

    pub fn resume(&self) {
        let _ = self.paused_tx.send(false);
    }

    pub fn cancel(&self) {
        let _ = self.cancel_tx.send(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.cancel_rx.borrow()
    }

    /// Block while the download is paused. Returns early when cancelled.
    async fn wait_while_paused(&self) {
        let mut rx = self.paused_rx.clone();
        loop {
            if self.is_cancelled() {
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

/// On-disk resume state. IDM stores the equivalent in the registry under
/// `HKCU\Software\DownloadManager\<id>\ChList`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    url: String,
    total: u64,
    segments: Vec<Segment>,
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
    pub client: reqwest::Client,
    last_emit: Mutex<Instant>,
}

impl Download {
    pub fn new(
        id: u64,
        url: String,
        dest: PathBuf,
        connections: u32,
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

        let client = reqwest::Client::builder()
            .user_agent(concat!("my-idm-clone/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::limited(10))
            .pool_max_idle_per_host(connections.max(1) as usize)
            .build()
            .map_err(|e| e.to_string())?;

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
            client,
            last_emit: Mutex::new(last_emit),
        }))
    }

    pub fn info(&self) -> DownloadInfo {
        let total = self.total.load(Ordering::SeqCst);
        let (downloaded, connections) = {
            let segments = self.segments.lock().expect("segments lock poisoned");
            (
                segments.iter().map(|s| s.downloaded).sum::<u64>(),
                segments.len() as u32,
            )
        };
        let status = *self.status.lock().expect("status lock poisoned");
        let error = self.error.lock().expect("error lock poisoned").clone();

        DownloadInfo {
            id: self.id,
            url: self.url.clone(),
            dest: self.dest.to_string_lossy().to_string(),
            total,
            downloaded,
            connections,
            status,
            error,
        }
    }

    pub fn set_status(&self, status: Status) {
        *self.status.lock().expect("status lock poisoned") = status;
    }

    fn seg_snapshot(&self, id: u32) -> Option<(u64, u64, u64)> {
        let segments = self.segments.lock().expect("segments lock poisoned");
        segments
            .iter()
            .find(|s| s.id == id)
            .map(|s| (s.start, s.end, s.downloaded))
    }

    fn seg_end(&self, id: u32) -> Option<u64> {
        let segments = self.segments.lock().expect("segments lock poisoned");
        segments.iter().find(|s| s.id == id).map(|s| s.end)
    }

    fn seg_part(&self, id: u32) -> Option<PathBuf> {
        let segments = self.segments.lock().expect("segments lock poisoned");
        segments.iter().find(|s| s.id == id).map(|s| s.part.clone())
    }

    fn set_seg_downloaded(&self, id: u32, value: u64) {
        let mut segments = self.segments.lock().expect("segments lock poisoned");
        if let Some(segment) = segments.iter_mut().find(|s| s.id == id) {
            segment.downloaded = value.min(segment.len());
        }
    }
}

/// Push the current state of a download to the UI.
fn emit(app: &AppHandle, download: &Arc<Download>) {
    let _ = app.emit("download://update", download.info());
}

/// Push the state of a download to the UI, but not more often than
/// `EMIT_INTERVAL` unless `force` is set.
fn emit_throttled(app: &AppHandle, download: &Arc<Download>, force: bool) {
    let mut last = download.last_emit.lock().expect("last_emit lock poisoned");
    if !force && last.elapsed() < EMIT_INTERVAL {
        return;
    }
    *last = Instant::now();
    drop(last);
    emit(app, download);
}

struct Probe {
    total: u64,
    ranges: bool,
}

/// Ask the server for the first byte only. The answer reveals the total size
/// and proves whether the server honours `Range` requests — the same probe
/// IDM performs before it decides how many connections it may open.
async fn probe(client: &reqwest::Client, url: &str) -> Result<Probe, String> {
    let response = client
        .get(url)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|e| format!("cannot reach the server: {e}"))?;

    let status = response.status();
    let ranges = status == reqwest::StatusCode::PARTIAL_CONTENT;

    let mut total = 0u64;
    if let Some(value) = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(total_str) = value.rsplit('/').next() {
            total = total_str.trim().parse().unwrap_or(0);
        }
    } else if let Some(len) = response.content_length() {
        total = len;
    }
    drop(response);

    Ok(Probe { total, ranges })
}

/// Rebuild the segment list, resuming from the manifest when possible.
fn restore(download: &Arc<Download>, total: u64, max_connections: u32) -> Vec<Segment> {
    if let Ok(text) = std::fs::read_to_string(&download.manifest) {
        if let Ok(manifest) = serde_json::from_str::<Manifest>(&text) {
            if manifest.url == download.url
                && manifest.total == total
                && !manifest.segments.is_empty()
                && manifest.segments.iter().all(|s| s.part.exists())
            {
                let mut segments = manifest.segments;
                for segment in &mut segments {
                    // A crash can leave extra bytes behind. Cut the part file
                    // back to the recorded length, the way IDM truncates the
                    // file when it resumes ("Trucate at %I64d").
                    if let Ok(file) = OpenOptions::new().write(true).open(&segment.part) {
                        let _ = file.set_len(segment.downloaded);
                    }
                }
                let next_id = segments.iter().map(|s| s.id).max().unwrap_or(0) + 1;
                download
                    .next_segment_id
                    .store(next_id, Ordering::SeqCst);
                *download.segments.lock().expect("segments lock poisoned") = segments.clone();
                return segments;
            }
        }
    }

    let segments = segmenter::plan(total, max_connections, &download.dir, &download.stem);
    download
        .next_segment_id
        .store(segments.len() as u32, Ordering::SeqCst);
    *download.segments.lock().expect("segments lock poisoned") = segments.clone();
    segments
}

fn save_manifest(download: &Arc<Download>) {
    let segments = download
        .segments
        .lock()
        .expect("segments lock poisoned")
        .clone();
    let manifest = Manifest {
        url: download.url.clone(),
        total: download.total.load(Ordering::SeqCst),
        segments,
    };
    if let Ok(text) = serde_json::to_string_pretty(&manifest) {
        let _ = std::fs::write(&download.manifest, text);
    }
}

/// Hand the tail of the largest unfinished segment to a new connection.
/// This is IDM's "add an optional connection" behaviour.
fn maybe_grow(
    app: &AppHandle,
    download: &Arc<Download>,
    set: &mut tokio::task::JoinSet<Result<(), String>>,
) {
    let mut segments = download.segments.lock().expect("segments lock poisoned");
    if segments.len() as u32 >= download.max_connections {
        return;
    }

    let mut best: Option<usize> = None;
    let mut best_left = 0u64;
    for (index, segment) in segments.iter().enumerate() {
        if segment.done() {
            continue;
        }
        let left = segment.len() - segment.downloaded;
        if left > best_left {
            best_left = left;
            best = Some(index);
        }
    }

    let Some(index) = best else { return };
    if best_left < MIN_SPLIT {
        return;
    }

    let new_id = download.next_segment_id.fetch_add(1, Ordering::SeqCst);
    let Some(new_segment) = segmenter::split(&mut segments[index], new_id, &download.dir, &download.stem)
    else {
        return;
    };
    segments.push(new_segment.clone());
    drop(segments);

    set.spawn(segment_task(app.clone(), download.clone(), new_segment.id));
    emit(app, download);
}

/// Request one byte range and write it into the segment's part file.
async fn fetch_range(
    download: &Arc<Download>,
    seg_id: u32,
    seg_start: u64,
    from: u64,
    to: u64,
) -> Result<(), String> {
    // IDM always asks for an explicit range and always asks for an identity
    // encoding so the bytes on the wire line up with the file offsets.
    let response = download
        .client
        .get(&download.url)
        .header(reqwest::header::RANGE, format!("bytes={from}-{to}"))
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        return Ok(());
    }
    if !status.is_success() {
        return Err(format!("server answered {status}"));
    }
    if status == reqwest::StatusCode::OK && from > 0 {
        return Err("the server ignored the Range request".to_string());
    }

    // Verify the server really served the range we asked for. IDM checks the
    // `Content-Range` reply in `HTTPConnection::GetRetrRangeReply()`.
    if let Some(value) = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
    {
        let served = value.split_whitespace().nth(1).unwrap_or("");
        if let Some(served_start) = served
            .split('-')
            .next()
            .and_then(|s| s.parse::<u64>().ok())
        {
            if served_start != from {
                return Err(format!(
                    "the server served bytes from {served_start} instead of {from}"
                ));
            }
        }
    }

    let part = download.seg_part(seg_id).ok_or("segment disappeared")?;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(&part)
        .map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(from)).map_err(|e| e.to_string())?;

    let base = from - seg_start;
    let mut written = 0u64;
    let mut stream = response.bytes_stream();

    while let Some(item) = stream.next().await {
        let bytes = item.map_err(|e| e.to_string())?;
        if bytes.is_empty() {
            continue;
        }

        // The segment may have been shortened while this request was running,
        // because a new connection took over the tail of the range.
        let seg_end = download.seg_end(seg_id).unwrap_or(to + 1);
        let position = from + written;
        if position >= seg_end {
            break;
        }

        let take = ((seg_end - position).min(bytes.len() as u64)) as usize;
        file.write_all(&bytes[..take]).map_err(|e| e.to_string())?;
        written += take as u64;

        download.set_seg_downloaded(seg_id, base + written);
    }

    file.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Download one segment, retrying on transient errors.
async fn segment_task(
    app: AppHandle,
    download: Arc<Download>,
    seg_id: u32,
) -> Result<(), String> {
    let mut attempt = 0u32;

    loop {
        if download.control.is_cancelled() {
            return Ok(());
        }
        download.control.wait_while_paused().await;
        if download.control.is_cancelled() {
            return Ok(());
        }

        let Some((start, end, downloaded)) = download.seg_snapshot(seg_id) else {
            return Ok(());
        };
        if downloaded >= end - start {
            return Ok(());
        }

        let from = start + downloaded;
        let to = end - 1;

        match fetch_range(&download, seg_id, start, from, to).await {
            Ok(()) => {
                attempt = 0;
                emit_throttled(&app, &download, false);
            }
            Err(error) => {
                attempt += 1;
                if attempt >= MAX_ATTEMPTS {
                    return Err(format!(
                        "segment {seg_id} failed after {attempt} attempts: {error}"
                    ));
                }
                let backoff = Duration::from_millis(400 * attempt as u64);
                tokio::time::sleep(backoff).await;
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

    let join_target = tmp.clone();
    let join_segments = segments.clone();
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
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?;

    joined?;

    if dest.exists() {
        let _ = std::fs::remove_file(&dest);
    }
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;

    for segment in &segments {
        let _ = std::fs::remove_file(&segment.part);
    }
    let _ = std::fs::remove_file(&download.manifest);

    Ok(())
}

async fn drive(app: &AppHandle, download: &Arc<Download>) -> Result<(), String> {
    download.set_status(Status::Running);
    emit(app, download);

    let probe = probe(&download.client, &download.url).await?;
    if probe.total == 0 {
        return Err("the server did not report a file size".to_string());
    }
    download.total.store(probe.total, Ordering::SeqCst);

    // A server that answers a range request with 200 cannot be segmented.
    let max_connections = if probe.ranges {
        download.max_connections
    } else {
        1
    };

    let segments = restore(download, probe.total, max_connections);
    save_manifest(download);
    emit(app, download);

    let mut set = tokio::task::JoinSet::new();
    for segment in &segments {
        set.spawn(segment_task(app.clone(), download.clone(), segment.id));
    }

    let mut failure: Option<String> = None;
    loop {
        tokio::select! {
            joined = set.join_next() => {
                match joined {
                    Some(Ok(Ok(()))) => {}
                    Some(Ok(Err(error))) => {
                        failure = Some(error);
                        download.control.cancel();
                    }
                    Some(Err(error)) => {
                        failure = Some(error.to_string());
                        download.control.cancel();
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(300)) => {
                maybe_grow(app, download, &mut set);
                save_manifest(download);
                emit_throttled(app, download, false);
            }
        }

        if failure.is_some() || set.is_empty() {
            break;
        }
    }

    if let Some(error) = failure {
        return Err(error);
    }
    if download.control.is_cancelled() {
        return Err("cancelled".to_string());
    }

    assemble(download).await?;
    Ok(())
}

/// Entry point of one download.
pub async fn run(app: AppHandle, download: Arc<Download>) {
    let outcome = drive(&app, &download).await;

    match outcome {
        Ok(()) => download.set_status(Status::Completed),
        Err(error) => {
            if download.control.is_cancelled() {
                download.set_status(Status::Cancelled);
            } else {
                *download.error.lock().expect("error lock poisoned") = Some(error);
                download.set_status(Status::Failed);
            }
        }
    }

    emit(&app, &download);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;

    fn sample_data() -> Vec<u8> {
        (0..1024u32).map(|i| (i % 251) as u8).collect()
    }

    /// A tiny HTTP server that answers every request with the requested range.
    /// Returns the URL and a channel carrying the head of each request.
    fn spawn_range_server(data: Vec<u8>) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = [0u8; 4096];
                let Ok(read) = stream.read(&mut buf) else { break };
                let head = String::from_utf8_lossy(&buf[..read]).to_string();
                let _ = tx.send(head.clone());

                let range = head
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("range:"))
                    .map(|line| line.split(':').nth(1).unwrap_or("").trim().to_string());

                let (start, end) = match range {
                    Some(value) => {
                        let value = value.trim_start_matches("bytes=");
                        let mut parts = value.split('-');
                        let start: usize = parts.next().unwrap_or("0").parse().unwrap_or(0);
                        let end: usize = parts
                            .next()
                            .unwrap_or("")
                            .parse()
                            .unwrap_or(data.len().saturating_sub(1));
                        (start, end)
                    }
                    None => (0, data.len().saturating_sub(1)),
                };
                let end = end.min(data.len().saturating_sub(1));

                let slice = &data[start..=end];
                let response = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                    slice.len(),
                    start,
                    end,
                    data.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(slice);
                let _ = stream.flush();
            }
        });

        (format!("http://127.0.0.1:{port}/file.bin"), rx)
    }

    /// A server that ignores Range requests and always answers with 200.
    fn spawn_ignoring_server(data: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).unwrap_or(0);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    data.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(&data);
                let _ = stream.flush();
            }
        });

        format!("http://127.0.0.1:{port}/file.bin")
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "my-idm-clone-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[tokio::test]
    async fn range_request_lands_at_the_right_offset_in_the_part_file() {
        let data = sample_data();
        let (url, requests) = spawn_range_server(data.clone());
        let dir = temp_dir("range");

        let download = Download::new(1, url, dir.join("out.bin"), 4).expect("download");
        let mut segments = segmenter::plan(data.len() as u64, 1, &dir, "out");
        segments[0].end = 512;
        *download.segments.lock().expect("lock") = segments;

        fetch_range(&download, 0, 0, 0, 511).await.expect("fetch");

        let head = requests.recv().expect("request head").to_ascii_lowercase();
        assert!(head.contains("range: bytes=0-511"), "request was: {head}");
        assert!(
            head.contains("accept-encoding: identity"),
            "request was: {head}"
        );

        let (_, _, downloaded) = download.seg_snapshot(0).expect("segment");
        assert_eq!(downloaded, 512);

        let part = std::fs::read(segmenter::part_path(&dir, "out", 0)).expect("part file");
        assert_eq!(part, data[..512].to_vec());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_server_that_ignores_the_range_is_rejected() {
        let data = sample_data();
        let url = spawn_ignoring_server(data.clone());
        let dir = temp_dir("ignore");

        let download = Download::new(2, url, dir.join("out.bin"), 4).expect("download");
        let mut segments = segmenter::plan(data.len() as u64, 1, &dir, "out");
        segments[0].downloaded = 100;
        *download.segments.lock().expect("lock") = segments;

        let error = fetch_range(&download, 0, 0, 100, 999).await.expect_err("must fail");
        assert!(
            error.contains("ignored the Range request"),
            "error was: {error}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
