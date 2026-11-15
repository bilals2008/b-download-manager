//! The torrent client: the jobs the application drives.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::task::JoinHandle;

use crate::engine::{self, Control, Job, JobState, Source};
use crate::error::TorrentError;
use crate::magnet::{parse_magnet, MagnetInfo};
use crate::metainfo::{parse_torrent, Metainfo, TorrentFile};
use crate::peer::PeerInfo;
use crate::pieces::{file_selection, PieceMap};
use crate::safe::safe_path;

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
}

impl JobRequest {
    /// A job for a magnet link.
    pub fn magnet(link: &str, directory: PathBuf) -> JobRequest {
        JobRequest {
            source: JobSource::Magnet(link.to_owned()),
            directory,
            wanted: Vec::new(),
        }
    }

    /// A job for a `.torrent` file.
    pub fn torrent(bytes: &[u8], directory: PathBuf) -> JobRequest {
        JobRequest {
            source: JobSource::Bytes(bytes.to_vec()),
            directory,
            wanted: Vec::new(),
        }
    }
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
    pub files: Vec<TorrentFile>,
    /// Where the pieces are.
    pub piece_map: PieceMap,
    /// The peers the trackers named.
    pub peers: Vec<PeerInfo>,
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
}

/// A job of the client.
struct Entry {
    id: String,
    name: String,
    directory: PathBuf,
    source: Source,
    wanted: Vec<usize>,
    control: Arc<Control>,
    state: Arc<Mutex<JobState>>,
    task: Option<JoinHandle<()>>,
}

/// The torrent jobs of the application.
pub struct TorrentClient {
    jobs: Mutex<HashMap<String, Entry>>,
}

impl TorrentClient {
    /// A client without jobs.
    pub fn new() -> TorrentClient {
        TorrentClient {
            jobs: Mutex::new(HashMap::new()),
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
            return Err(TorrentError::Unsafe(format!("{name} would leave the folder")));
        }
        tokio::fs::create_dir_all(&request.directory).await?;

        let control = Arc::new(Control::new());
        let state = Arc::new(Mutex::new(JobState::new()));
        if let Source::Magnet(magnet) = &source {
            if let Ok(mut state) = state.lock() {
                state.magnet = Some(magnet.clone());
            }
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
                    wanted: request.wanted,
                    control: control.clone(),
                    state: state.clone(),
                    task: None,
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
        entry.control.set_paused(false);
        let job = Arc::new(Job {
            id: entry.id.clone(),
            name: entry.name.clone(),
            directory: entry.directory.clone(),
            source: entry.source.clone(),
            wanted: entry.wanted.clone(),
            control: entry.control.clone(),
            state: entry.state.clone(),
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
            Some(metainfo) => (
                file_selection(metainfo, &entry.wanted),
                PieceMap::new(metainfo.piece_length, metainfo.total_length),
                metainfo.total_length,
                metainfo.files.clone(),
            ),
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
            peers: state.peers.clone(),
            status: state.status.clone(),
            downloaded: state.downloaded,
            uploaded: state.uploaded,
            total_length,
            seeding: state.seeding,
        }
    }
}

impl Default for TorrentClient {
    fn default() -> Self {
        TorrentClient::new()
    }
}
