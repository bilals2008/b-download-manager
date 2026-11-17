//! The torrent client: the jobs the application drives.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::task::JoinHandle;

use crate::engine::{self, Control, Job, JobState, Limits, Source};
use crate::error::TorrentError;
use crate::lpd::Lpd;
use crate::magnet::{parse_magnet, MagnetInfo};
use crate::metainfo::{parse_torrent, Metainfo};
use crate::options::TorrentOptions;
use crate::peer::PeerInfo;
use crate::pieces::{file_progress, file_selection, piece_bitmap, present_bytes, PieceMap};
use crate::portmap::{PortMap, Status as PortMappingStatus};
use crate::safe::safe_path;
use crate::storage::pieces_on_disk;
use crate::util::base64_encode;

/// What a job is doing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state", content = "message")]
pub enum JobStatus {
    /// The job is there but has not started.
    Idle,
    /// The job is looking for peers.
    Looking,
    /// The trackers have not named a peer yet.
    NoPeers,
    /// Bytes are coming in.
    Downloading,
    /// The job is paused.
    Paused,
    /// Everything is there.
    Completed,
    /// Everything is there and the job is uploading.
    Seeding,
    /// The job stopped with an error.
    Failed(String),
}

impl JobStatus {
    /// The words the application shows.
    pub fn sentence(&self) -> String {
        match self {
            JobStatus::Idle => "Not started yet.".into(),
            JobStatus::Looking => "Looking for peers.".into(),
            JobStatus::NoPeers => "No peers yet.".into(),
            JobStatus::Downloading => "Downloading.".into(),
            JobStatus::Paused => "Paused.".into(),
            JobStatus::Completed => "Finished.".into(),
            JobStatus::Seeding => "Seeding.".into(),
            JobStatus::Failed(why) => why.clone(),
        }
    }
}

/// Where a job comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobSource {
    /// A magnet link.
    Magnet(String),
    /// The bytes of a `.torrent` file.
    Bytes(Vec<u8>),
}

/// What the application hands over to start a job.
#[derive(Clone, Debug)]
pub struct JobRequest {
    /// The magnet link or the `.torrent` file.
    pub source: JobSource,
    /// The folder the files are written into.
    pub directory: PathBuf,
    /// The files the reader chose; every file when nothing is chosen.
    pub wanted: Vec<usize>,
    /// Whether the job is queued paused instead of running at once.
    pub start_paused: bool,
    /// Whether pieces are fetched strictly in order.
    pub sequential: bool,
}

impl JobRequest {
    /// A job for a magnet link.
    pub fn magnet(link: &str, directory: PathBuf) -> JobRequest {
        JobRequest {
            source: JobSource::Magnet(link.to_owned()),
            directory,
            wanted: Vec::new(),
            start_paused: false,
            sequential: false,
        }
    }

    /// A job for a `.torrent` file.
    pub fn torrent(bytes: &[u8], directory: PathBuf) -> JobRequest {
        JobRequest {
            source: JobSource::Bytes(bytes.to_vec()),
            directory,
            wanted: Vec::new(),
            start_paused: false,
            sequential: false,
        }
    }
}

/// One file of a torrent, as the window shows it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFile {
    /// Where the file goes, relative to the folder the job writes into.
    pub path: String,
    /// How many bytes the file holds.
    pub length: u64,
    /// Where the file starts in the stream of all the bytes.
    pub offset: u64,
    /// How much of the file is on disk, from 0 to 1.
    pub progress: f32,
    /// Whether the whole file is there.
    pub done: bool,
    /// Whether the reader kept this file.
    pub selected: bool,
}

/// How a tracker is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TrackerHealth {
    /// The tracker has not answered yet.
    Working,
    /// The tracker answered.
    Ok,
    /// The tracker could not be reached.
    Error,
}

/// One tracker of the torrent, as the window shows it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerState {
    /// The address of the tracker.
    pub url: String,
    /// How the tracker is doing.
    pub state: TrackerHealth,
    /// What the tracker said the last time, when it said something.
    pub message: Option<String>,
    /// How many peers the tracker named.
    pub peers: u64,
    /// How many peers have the whole torrent.
    pub seeders: u64,
    /// How many peers have part of it.
    pub leechers: u64,
    /// How many seconds until the tracker is asked again.
    pub next_in: Option<u64>,
}

/// A job, as the application sees it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentJob {
    /// The info hash of the torrent.
    pub info_hash: String,
    /// The name of the torrent.
    pub name: String,
    /// The folder the files are written into.
    pub directory: PathBuf,
    /// What the magnet link said, when the job came from one.
    pub magnet: Option<MagnetInfo>,
    /// What the torrent file said, when the job has it.
    pub metainfo: Option<Metainfo>,
    /// The files the reader chose.
    pub selected: Vec<usize>,
    /// The files of the torrent.
    pub files: Vec<JobFile>,
    /// Where the pieces are.
    pub piece_map: PieceMap,
    /// Which pieces are on disk, as a bitmap: one bit per piece, the first
    /// piece in the highest bit of the first byte, in base64.
    pub piece_bits: String,
    /// The piece that is coming in right now, when one is.
    pub piece_inflight: Option<u64>,
    /// The peers the trackers named.
    pub peers: Vec<PeerInfo>,
    /// The trackers of the torrent.
    pub trackers: Vec<TrackerState>,
    /// What the job is doing.
    pub status: JobStatus,
    /// How many bytes came in.
    pub downloaded: u64,
    /// How many bytes went out.
    pub uploaded: u64,
    /// How many bytes the torrent holds.
    pub total_length: u64,
    /// Whether the job uploads.
    pub seeding: bool,
    /// How many bytes a second come in.
    pub down_speed: u64,
    /// How many bytes a second go out.
    pub up_speed: u64,
    /// How many seconds are left, when the speed says so.
    pub eta: Option<u64>,
    /// How many peers have the whole torrent.
    pub seeders: u64,
    /// How many peers have part of it.
    pub leechers: u64,
    /// When the job was added, in milliseconds since the epoch.
    pub added_at: u64,
    /// What went wrong, when something did.
    pub error: Option<String>,
}

/// A job of the client.
struct Entry {
    id: String,
    name: String,
    directory: PathBuf,
    source: Source,
    control: Arc<Control>,
    state: Arc<Mutex<JobState>>,
    task: Option<JoinHandle<()>>,
    added_at: u64,
    sequential: bool,
}

/// The torrent jobs of the application.
pub struct TorrentClient {
    jobs: Mutex<HashMap<String, Entry>>,
    limits: Mutex<Limits>,
    /// The reader's choices, shared with every running job.
    options: Arc<Mutex<TorrentOptions>>,
    /// The local discovery, when the reader allows it.
    lpd: Arc<Mutex<Option<Arc<Lpd>>>>,
    /// The router's port mapping, when the reader asks for it, with the
    /// port it was asked for.
    portmap: Mutex<Option<(u16, Arc<PortMap>)>>,
}

impl TorrentClient {
    /// A client without jobs, running under the crate's quiet defaults.
    pub fn new() -> TorrentClient {
        TorrentClient {
            jobs: Mutex::new(HashMap::new()),
            limits: Mutex::new(Limits::default()),
            options: Arc::new(Mutex::new(TorrentOptions::default())),
            lpd: Arc::new(Mutex::new(None)),
            portmap: Mutex::new(None),
        }
    }

    /// A client running under the reader's choices.
    pub fn with_options(options: TorrentOptions) -> TorrentClient {
        let client = TorrentClient::new();
        client.set_options(options);
        client
    }

    /// Take the reader's choices. The proxy reaches the next announce of
    /// every running job; the local discovery and the port mapping start or
    /// stop here, and a changed port is asked for again.
    pub fn set_options(&self, next: TorrentOptions) {
        let port = next.listen_port();
        if let Ok(mut held) = self.options.lock() {
            *held = next.clone();
        }
        if let Ok(mut lpd) = self.lpd.lock() {
            if next.lpd {
                if lpd.is_none() {
                    *lpd = Lpd::start(port).ok();
                }
            } else if let Some(running) = lpd.take() {
                running.stop();
            }
        }
        if let Ok(mut mapping) = self.portmap.lock() {
            let held_port = mapping.as_ref().map(|(port, _)| *port);
            if !next.port_mapping {
                if let Some((_, running)) = mapping.take() {
                    running.stop();
                }
            } else if held_port != Some(port) {
                if let Some((_, running)) = mapping.take() {
                    running.stop();
                }
                if let Ok(running) = PortMap::start(port) {
                    *mapping = Some((port, running));
                }
            }
        }
    }

    /// The choices the client runs under, as they are now.
    pub fn options(&self) -> TorrentOptions {
        self.options
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default()
    }

    /// What the router's port mapping is doing.
    pub fn port_mapping_status(&self) -> PortMappingStatus {
        self.portmap
            .lock()
            .ok()
            .and_then(|held| held.as_ref().map(|(_, mapping)| mapping.status()))
            .unwrap_or(PortMappingStatus::Off)
    }

    /// Stop the local discovery and give the router's mapping back. The
    /// application calls this when it closes.
    pub fn shutdown(&self) {
        if let Ok(mut lpd) = self.lpd.lock() {
            if let Some(running) = lpd.take() {
                running.stop();
            }
        }
        if let Ok(mut mapping) = self.portmap.lock() {
            if let Some((_, running)) = mapping.take() {
                running.stop();
            }
        }
    }

    /// Add a job and start it. The answer is the info hash, which is the
    /// name of the job.
    pub async fn add_job(&self, request: JobRequest) -> Result<String, TorrentError> {
        let (id, name, source) = match request.source {
            JobSource::Magnet(link) => {
                let magnet = parse_magnet(&link)?;
                let name = magnet.display_name();
                (magnet.info_hash.clone(), name, Source::Magnet(magnet))
            }
            JobSource::Bytes(bytes) => {
                let metainfo = parse_torrent(&bytes)?;
                for file in &metainfo.files {
                    if safe_path(&file.path).is_none() {
                        return Err(TorrentError::Unsafe(format!(
                            "{} would leave the folder",
                            file.path
                        )));
                    }
                }
                let name = metainfo.name.clone();
                (metainfo.info_hash.clone(), name, Source::Metainfo(metainfo))
            }
        };
        if safe_path(&name).is_none() {
            return Err(TorrentError::Unsafe(format!(
                "{name} would leave the folder"
            )));
        }
        tokio::fs::create_dir_all(&request.directory).await?;

        let control = Arc::new(Control::new());
        control.set_limits(*self.limits.lock().expect("the limits"));
        if request.start_paused {
            control.set_paused(true);
        }
        let state = Arc::new(Mutex::new(JobState::new()));
        if let Source::Magnet(magnet) = &source {
            if let Ok(mut state) = state.lock() {
                state.magnet = Some(magnet.clone());
            }
        }
        if let Ok(mut state) = state.lock() {
            state.wanted = request.wanted;
            // Set before the engine's first round, so the window never shows
            // "Not started yet." for a job that is already queued.
            state.status = if request.start_paused {
                JobStatus::Paused
            } else {
                JobStatus::Looking
            };
        }
        {
            let mut jobs = self.jobs.lock().expect("the jobs");
            if jobs.contains_key(&id) {
                return Err(TorrentError::Failed(format!(
                    "{id} is already a job of this session"
                )));
            }
            jobs.insert(
                id.clone(),
                Entry {
                    id: id.clone(),
                    name,
                    directory: request.directory,
                    source,
                    control: control.clone(),
                    state: state.clone(),
                    task: None,
                    added_at: now_millis(),
                    sequential: request.sequential,
                },
            );
        }
        self.start(&id).await?;
        Ok(id)
    }

    /// Start a job that was paused.
    pub async fn start(&self, id: &str) -> Result<(), TorrentError> {
        let mut jobs = self.jobs.lock().expect("the jobs");
        let entry = jobs
            .get_mut(id)
            .ok_or_else(|| TorrentError::Failed(format!("{id} is not a job of this session")))?;
        if entry.task.is_some() {
            return Ok(());
        }
        // Start only spawns the loop; the pause flag belongs to the job (set
        // in add_job, toggled by pause/resume) and must survive a start.
        let job = Arc::new(Job {
            id: entry.id.clone(),
            directory: entry.directory.clone(),
            source: entry.source.clone(),
            control: entry.control.clone(),
            state: entry.state.clone(),
            options: self.options.clone(),
            lpd: self.lpd.clone(),
            sequential: entry.sequential,
        });
        entry.task = Some(tokio::spawn(engine::run(job)));
        Ok(())
    }

    /// Stop a job where it is; the bytes that are there stay.
    pub fn pause(&self, id: &str) -> Result<(), TorrentError> {
        let jobs = self.jobs.lock().expect("the jobs");
        let entry = jobs
            .get(id)
            .ok_or_else(|| TorrentError::Failed(format!("{id} is not a job of this session")))?;
        entry.control.set_paused(true);
        Ok(())
    }

    /// Let a job go on.
    pub fn resume(&self, id: &str) -> Result<(), TorrentError> {
        let jobs = self.jobs.lock().expect("the jobs");
        let entry = jobs
            .get(id)
            .ok_or_else(|| TorrentError::Failed(format!("{id} is not a job of this session")))?;
        entry.control.set_paused(false);
        Ok(())
    }

    /// Stop every job of the session where it is.
    pub fn pause_all(&self) {
        let jobs = self.jobs.lock().expect("the jobs");
        for entry in jobs.values() {
            entry.control.set_paused(true);
        }
    }

    /// Let every job of the session go on.
    pub fn resume_all(&self) {
        let jobs = self.jobs.lock().expect("the jobs");
        for entry in jobs.values() {
            entry.control.set_paused(false);
        }
    }

    /// Remove a job, and with it the files it wrote.
    pub async fn remove(&self, id: &str, delete_files: bool) -> Result<(), TorrentError> {
        let entry = {
            let mut jobs = self.jobs.lock().expect("the jobs");
            jobs.remove(id)
        };
        let Some(entry) = entry else {
            return Err(TorrentError::Failed(format!(
                "{id} is not a job of this session"
            )));
        };
        entry.control.cancel();
        if let Some(task) = entry.task {
            task.abort();
        }
        // The job's task may have been cut off mid-round; the local network
        // is told here that the torrent is no longer held.
        if let Ok(held) = self.lpd.lock() {
            if let Some(lpd) = held.as_ref() {
                if let Some(hash) =
                    crate::util::hex_decode(id).and_then(|bytes| <[u8; 20]>::try_from(bytes).ok())
                {
                    lpd.forget(hash);
                }
            }
        }
        if delete_files {
            let _ = tokio::fs::remove_dir_all(&entry.directory).await;
        }
        Ok(())
    }

    /// Say whether the job uploads after it is finished.
    pub fn set_seeding(&self, id: &str, seeding: bool) -> Result<(), TorrentError> {
        let jobs = self.jobs.lock().expect("the jobs");
        let entry = jobs
            .get(id)
            .ok_or_else(|| TorrentError::Failed(format!("{id} is not a job of this session")))?;
        entry.control.set_seeding(seeding);
        if let Ok(mut state) = entry.state.lock() {
            state.seeding = seeding;
        }
        Ok(())
    }

    /// The limits the reader set for the torrents: how fast a job may
    /// upload, the ratio a finished job stops sharing at, and how many
    /// minutes it keeps sharing. A zero limit or ratio never stops anything.
    pub fn set_limits(&self, upload_limit: u64, seed_ratio: f64, seed_minutes: u64) {
        let limits = Limits {
            upload_limit,
            seed_ratio,
            seed_minutes,
        };
        {
            let mut held = self.limits.lock().expect("the limits");
            *held = limits;
        }
        let jobs = self.jobs.lock().expect("the jobs");
        for entry in jobs.values() {
            entry.control.set_limits(limits);
        }
    }

    /// Change the files a job takes. A file that is already whole keeps its
    /// bytes; a file that is dropped is not deleted, it is only not written
    /// to any more.
    pub fn set_files(&self, id: &str, wanted: Vec<usize>) -> Result<(), TorrentError> {
        let jobs = self.jobs.lock().expect("the jobs");
        let entry = jobs
            .get(id)
            .ok_or_else(|| TorrentError::Failed(format!("{id} is not a job of this session")))?;
        let mut state = entry
            .state
            .lock()
            .map_err(|_| TorrentError::Failed("the job is not running".into()))?;
        state.wanted = wanted;
        Ok(())
    }

    /// Check the files on disk against the torrent again: the pieces that are
    /// there are found by hashing them, and the job goes on with those.
    pub fn recheck(&self, id: &str) -> Result<(), TorrentError> {
        let jobs = self.jobs.lock().expect("the jobs");
        let entry = jobs
            .get(id)
            .ok_or_else(|| TorrentError::Failed(format!("{id} is not a job of this session")))?;
        let (metainfo, wanted) = {
            let state = entry
                .state
                .lock()
                .map_err(|_| TorrentError::Failed("the job is not running".into()))?;
            let metainfo = match &entry.source {
                Source::Metainfo(metainfo) => Some(metainfo.clone()),
                Source::Magnet(_) => state.metainfo.clone(),
            };
            (
                metainfo.ok_or_else(|| {
                    TorrentError::Failed("the torrent has no file list yet".into())
                })?,
                state.wanted.clone(),
            )
        };
        let chosen = file_selection(&metainfo, &wanted);
        let have = pieces_on_disk(&entry.directory, &metainfo, &chosen)?;
        let piece_map = PieceMap::new(metainfo.piece_length, metainfo.total_length);
        if let Ok(mut state) = entry.state.lock() {
            state.have = have;
            state.downloaded = present_bytes(&piece_map, &state.have);
        }
        Ok(())
    }

    /// What the job is doing.
    pub fn status(&self, id: &str) -> Option<JobStatus> {
        let jobs = self.jobs.lock().expect("the jobs");
        let entry = jobs.get(id)?;
        let state = entry.state.lock().ok()?;
        Some(state.status.clone())
    }

    /// How many bytes the job sent out.
    pub fn uploaded_bytes(&self, id: &str) -> u64 {
        let jobs = self.jobs.lock().expect("the jobs");
        let Some(entry) = jobs.get(id) else {
            return 0;
        };
        entry.state.lock().map(|state| state.uploaded).unwrap_or(0)
    }

    /// The job, as the application sees it.
    pub fn job(&self, id: &str) -> Option<TorrentJob> {
        let jobs = self.jobs.lock().expect("the jobs");
        let entry = jobs.get(id)?;
        let state = entry.state.lock().ok()?;
        Some(Self::describe(entry, &state))
    }

    /// Every job of the client.
    pub fn jobs(&self) -> Vec<TorrentJob> {
        let jobs = self.jobs.lock().expect("the jobs");
        jobs.values()
            .filter_map(|entry| {
                let state = entry.state.lock().ok()?;
                Some(Self::describe(entry, &state))
            })
            .collect()
    }

    fn describe(entry: &Entry, state: &JobState) -> TorrentJob {
        let metainfo = state.metainfo.clone().or_else(|| match &entry.source {
            Source::Metainfo(metainfo) => Some(metainfo.clone()),
            Source::Magnet(_) => None,
        });
        let magnet = state.magnet.clone().or_else(|| match &entry.source {
            Source::Magnet(magnet) => Some(magnet.clone()),
            Source::Metainfo(_) => None,
        });
        let (selected, piece_map, total_length, files) = match &metainfo {
            Some(metainfo) => {
                let selected = file_selection(metainfo, &state.wanted);
                let progress = file_progress(metainfo, &state.have);
                let files = metainfo
                    .files
                    .iter()
                    .enumerate()
                    .zip(progress)
                    .map(|((index, file), (progress, done))| JobFile {
                        path: file.path.clone(),
                        length: file.length,
                        offset: file.offset,
                        progress,
                        done,
                        selected: selected.contains(&index),
                    })
                    .collect();
                let have = state.have.iter().filter(|has| **has).count() as u64;
                (
                    selected,
                    PieceMap::with_have(metainfo.piece_length, metainfo.total_length, have),
                    metainfo.total_length,
                    files,
                )
            }
            None => (Vec::new(), PieceMap::new(0, 0), 0, Vec::new()),
        };
        TorrentJob {
            info_hash: entry.id.clone(),
            name: entry.name.clone(),
            directory: entry.directory.clone(),
            magnet,
            metainfo,
            selected,
            files,
            piece_map,
            piece_bits: base64_encode(&piece_bitmap(&state.have)),
            piece_inflight: state.inflight,
            peers: state.peers.clone(),
            trackers: state.trackers.clone(),
            status: state.status.clone(),
            downloaded: state.downloaded,
            uploaded: state.uploaded,
            total_length,
            seeding: state.seeding,
            down_speed: state.down_speed,
            up_speed: state.up_speed,
            eta: state.eta,
            seeders: state.seeders,
            leechers: state.leechers,
            added_at: entry.added_at,
            error: state.error.clone(),
        }
    }
}

impl Default for TorrentClient {
    fn default() -> Self {
        TorrentClient::new()
    }
}

/// The milliseconds since the epoch, when the clock says so.
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}
