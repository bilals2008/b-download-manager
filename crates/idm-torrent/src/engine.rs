//! The engine of a job: the trackers, the peers, the pieces and the files.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::TorrentError;
use crate::lpd::Lpd;
use crate::magnet::MagnetInfo;
use crate::metainfo::{metainfo_from_metadata, Metainfo, TorrentFile};
use crate::options::TorrentOptions;
use crate::peer::{client_name, Message, PeerInfo};
use crate::peerconn::PeerSession;
use crate::pieces::{file_selection, present_bytes, PieceMap};
use crate::rates;
use crate::session::{JobStatus, TrackerState};
use crate::storage::{pieces_on_disk, Storage};
use crate::tracker;
use crate::util::{hex_encode, sha1_bytes};

/// What the reader allows a job: how fast it may upload, and when a finished
/// job stops sharing.
///
/// A zero upload limit is no limit, and a zero ratio or a zero number of
/// minutes never stops the sharing; a job without limits seeds until the
/// reader says otherwise. The limits are a plain value so they can be handed
/// to a job and read back without holding a lock over the engine.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Limits {
    /// How many bytes a second may go out; zero is no limit.
    pub upload_limit: u64,
    /// The uploaded over downloaded ratio a finished job stops at; zero never
    /// stops.
    pub seed_ratio: f64,
    /// How many minutes a finished job keeps sharing; zero never stops.
    pub seed_minutes: u64,
}

/// What a job is made of.
#[derive(Clone)]
pub(crate) enum Source {
    /// A magnet link; the metadata comes from a peer later.
    Magnet(MagnetInfo),
    /// A `.torrent` file.
    Metainfo(Metainfo),
}

/// What the application can say about a job while it runs.
pub(crate) struct Control {
    paused: AtomicBool,
    cancelled: AtomicBool,
    seeding: AtomicBool,
    /// The limits sit behind a mutex because the ratio is a fraction and the
    /// reader may change them at any moment of a running job.
    limits: Mutex<Limits>,
}

impl Control {
    pub fn new() -> Control {
        Control {
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            seeding: AtomicBool::new(false),
            limits: Mutex::new(Limits::default()),
        }
    }

    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn seeding(&self) -> bool {
        self.seeding.load(Ordering::Relaxed)
    }

    pub fn set_seeding(&self, seeding: bool) {
        self.seeding.store(seeding, Ordering::Relaxed);
    }

    /// The limits the reader set, as they are now. A job reads them every
    /// round, so a change is felt without restarting it.
    pub fn limits(&self) -> Limits {
        *self.limits.lock().expect("the limits")
    }

    /// Take the limits the reader set for the whole session.
    pub fn set_limits(&self, limits: Limits) {
        if let Ok(mut held) = self.limits.lock() {
            *held = limits;
        }
    }
}

/// What a job knows about itself right now.
pub(crate) struct JobState {
    pub status: JobStatus,
    pub downloaded: u64,
    pub uploaded: u64,
    pub peers: Vec<PeerInfo>,
    pub have: Vec<bool>,
    /// The piece that is coming in right now, when one is.
    pub inflight: Option<u64>,
    pub metainfo: Option<Metainfo>,
    pub magnet: Option<MagnetInfo>,
    pub files: Vec<TorrentFile>,
    pub seeding: bool,
    pub error: Option<String>,
    /// The files the reader kept; empty is every file.
    pub wanted: Vec<usize>,
    /// The trackers of the job, and how each of them is doing.
    pub trackers: Vec<TrackerState>,
    /// How many bytes a second come in.
    pub down_speed: u64,
    /// How many bytes a second go out.
    pub up_speed: u64,
    /// How many seconds are left, when the speed says so.
    pub eta: Option<u64>,
    /// How many peers the trackers said have the whole torrent.
    pub seeders: u64,
    /// How many peers the trackers said have part of it.
    pub leechers: u64,
    /// When the job started sharing, so the minutes the reader set can run.
    pub seeding_since: Option<Instant>,
}

impl JobState {
    pub fn new() -> JobState {
        JobState {
            status: JobStatus::Idle,
            downloaded: 0,
            uploaded: 0,
            peers: Vec::new(),
            have: Vec::new(),
            inflight: None,
            metainfo: None,
            magnet: None,
            files: Vec::new(),
            seeding: false,
            error: None,
            wanted: Vec::new(),
            trackers: Vec::new(),
            down_speed: 0,
            up_speed: 0,
            eta: None,
            seeders: 0,
            leechers: 0,
            seeding_since: None,
        }
    }
}

/// One job.
pub(crate) struct Job {
    pub id: String,
    pub directory: std::path::PathBuf,
    pub source: Source,
    pub control: Arc<Control>,
    pub state: Arc<Mutex<JobState>>,
    /// The reader's choices, read again every round so a change is felt
    /// without restarting the job.
    pub options: Arc<Mutex<TorrentOptions>>,
    /// The local discovery of the client, when the reader allows it.
    pub lpd: Arc<Mutex<Option<Arc<Lpd>>>>,
    /// Whether pieces are fetched strictly in order.
    pub sequential: bool,
}

impl Job {
    /// The trackers of the job, from the magnet or from the torrent file.
    pub fn trackers(&self) -> Vec<String> {
        match &self.source {
            Source::Magnet(magnet) => magnet.trackers.clone(),
            Source::Metainfo(metainfo) => metainfo.trackers.clone(),
        }
    }

    /// The metadata, when the job already has it.
    pub fn metainfo(&self) -> Option<&Metainfo> {
        match &self.source {
            Source::Metainfo(metainfo) => Some(metainfo),
            Source::Magnet(_) => None,
        }
    }
}

/// The next piece to ask a peer for. In order, only the first missing one: a
/// peer that lacks it yields nothing, so the caller drops the session and
/// tries another peer instead of skipping ahead. Out of order, the first
/// missing piece the peer actually has, which is today's fastest fill.
fn choose_piece(missing: &[u64], peer_has: impl Fn(u32) -> bool, sequential: bool) -> Option<u64> {
    let first = *missing.first()?;
    if sequential {
        return peer_has(first as u32).then_some(first);
    }
    missing
        .iter()
        .copied()
        .find(|index| peer_has(*index as u32))
}

/// Run a job until it is done, or until it is removed.
pub(crate) async fn run(job: Arc<Job>) {
    let peer_id = crate::util::peer_id();
    let peer_id_text = hex_encode(&peer_id);
    let mut addresses: Vec<SocketAddr> = Vec::new();
    let mut known: Vec<PeerInfo> = Vec::new();
    let mut tried: Vec<SocketAddr> = Vec::new();
    let mut next_announce = Instant::now();
    let mut interval = Duration::from_secs(1800);
    let mut session: Option<PeerSession> = None;
    let mut metainfo = job.metainfo().cloned();
    let mut storage: Option<Arc<Storage>> = None;
    let mut have: Vec<bool> = Vec::new();
    let mut downloaded = 0u64;
    let mut uploaded = 0u64;
    // A peer learns what we have once per session; a new peer learns it again.
    let mut announced = false;

    // The speeds, the time left and the speed of every peer are counted once
    // a second while the job runs. The counting stops with the job.
    let speeds = tokio::spawn(rates::tick(job.clone()));
    trackers::list_trackers(&job);
    discovery::lpd_announce(&job);

    loop {
        if session.is_none() {
            announced = false;
        }
        if job.control.cancelled() {
            break;
        }
        if job.control.paused() {
            set_status(&job, JobStatus::Paused);
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        }
        let options = job
            .options
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default();

        // An upload limit the reader changed is handed to the peer that is
        // there; the next peer is told when it connects.
        if let Some(peer) = session.as_mut() {
            peer.set_upload_limit(job.control.limits().upload_limit);
        }

        // The trackers are asked now and then who has the torrent. What each
        // one answers is kept as that tracker's row, so the window can show
        // which tracker is quiet and which one has nobody.
        if Instant::now() >= next_announce {
            next_announce = Instant::now() + interval;
            tried.clear();
            // A magnet does not know its size until a peer has told it, and a
            // tracker reads `left=0` as "this one has everything", so an
            // unknown size is sent as the smallest real one instead. Reporting
            // zero made a new magnet look like a seeder and get no peers back.
            let left = metainfo.as_ref().map(|m| m.total_length).unwrap_or(1);
            let mut tracker_ok = false;
            for url in job.trackers() {
                let answer = tracker::announce(
                    &url,
                    &job.id,
                    &peer_id_text,
                    options.listen_port(),
                    "started",
                    left,
                    options.proxy.as_deref(),
                )
                .await;
                if let Ok(result) = &answer {
                    tracker_ok = true;
                    for address in &result.peers {
                        if !addresses.contains(address) {
                            addresses.push(*address);
                            known.push(PeerInfo {
                                address: address.to_string(),
                                ..PeerInfo::default()
                            });
                        }
                    }
                    if result.interval > 0 {
                        interval = Duration::from_secs(result.interval.clamp(30, 3600));
                    }
                }
                if let Ok(mut state) = job.state.lock() {
                    trackers::note_tracker(&mut state, &url, &answer, interval.as_secs());
                    state.seeders = state.trackers.iter().map(|t| t.seeders).max().unwrap_or(0);
                    state.leechers = state.trackers.iter().map(|t| t.leechers).max().unwrap_or(0);
                    set_peers(&mut state, &known, &session);
                }
            }

            if !tracker_ok {
                discovery::dht_fallback(
                    &job,
                    options.listen_port(),
                    &mut addresses,
                    &mut known,
                    &session,
                )
                .await;
                // The table is asked again in a minute, not in half an hour:
                // a swarm's peers come and go, and the first walk may have
                // started before the table knew any node.
                next_announce = Instant::now() + Duration::from_secs(60);
            }
            if options.lpd {
                discovery::lpd_peers(&job, &mut addresses, &mut known, &session).await;
            }
        }

        // A magnet has no metadata until a peer gives it.
        if metainfo.is_none() {
            if session.is_none() {
                match connect(&mut session, &addresses, &mut tried, &job.id, &peer_id, 0).await {
                    Ok(()) => {}
                    Err(_) => {
                        set_status(&job, JobStatus::NoPeers);
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                }
            }
            let peer = session.as_mut().expect("a peer");
            set_status(&job, JobStatus::Looking);
            // A peer that trickles bytes or only sends keep-alives would
            // otherwise hold the job forever: the file list gets a deadline
            // of its own, and a peer that misses it is dropped for the next.
            match tokio::time::timeout(Duration::from_secs(45), peer.fetch_metadata()).await {
                Ok(Ok(metadata)) => {
                    match metainfo_from_metadata(&metadata, job.trackers(), Some(job.id.as_str())) {
                        Ok(found) => {
                            metainfo = Some(found);
                            if let Ok(mut state) = job.state.lock() {
                                state.metainfo = metainfo.clone();
                                state.files = metainfo
                                    .as_ref()
                                    .map(|m| m.files.clone())
                                    .unwrap_or_default();
                            }
                            // The peer was asked for the file list before the
                            // piece count was known, so the session holds no
                            // map of what it has. Dropping it and letting the
                            // peer be tried again — this time with the count —
                            // is what turns the metadata into a download
                            // instead of a wait for a second peer.
                            session = None;
                            tried.clear();
                            continue;
                        }
                        Err(_) => {
                            // A peer that serves metadata of another torrent is
                            // not a peer of this one; ask the next one.
                            session = None;
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    }
                }
                Err(_) | Ok(Err(_)) => {
                    session = None;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
        }

        let metainfo = metainfo.as_ref().expect("the metadata");
        let piece_map = PieceMap::new(metainfo.piece_length, metainfo.total_length);

        // The files are opened once the metadata is there. What is already on
        // disk is found by hashing it, so a job that was stopped goes on from
        // where it stopped instead of from the beginning.
        if storage.is_none() {
            let wanted = job
                .state
                .lock()
                .map(|state| state.wanted.clone())
                .unwrap_or_default();
            let chosen = file_selection(metainfo, &wanted);
            match Storage::new(&job.directory, metainfo, &chosen).await {
                Ok(files) => {
                    let count = piece_map.count as usize;
                    // The folder the storage opened is the folder that is
                    // read back, so what is hashed is what will be written.
                    have = pieces_on_disk(files.root(), metainfo, &chosen)
                        .unwrap_or_else(|_| vec![false; count]);
                    have.resize(count, false);
                    downloaded = present_bytes(&piece_map, &have);
                    storage = Some(Arc::new(files));
                    if let Ok(mut state) = job.state.lock() {
                        state.have = have.clone();
                        state.downloaded = downloaded;
                    }
                }
                Err(error) => {
                    set_failed(&job, error.to_string());
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            }
        }

        // Every piece is there: the job is done.
        if have.iter().all(|has| *has) {
            if job.control.seeding() {
                if let Ok(mut state) = job.state.lock() {
                    // The minutes the reader set are counted from the moment
                    // the job first had something to give.
                    if state.seeding_since.is_none() {
                        state.seeding_since = Some(Instant::now());
                    }
                }
                if shared_enough(&job, downloaded, uploaded) {
                    // The ratio or the minutes are spent: the job stops
                    // sharing, and every byte it wrote stays where it is.
                    stop_seeding(&job);
                    set_status(&job, JobStatus::Completed);
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                }
                // A seeder still wants a peer to talk to; without one there is
                // nobody to give the pieces to.
                if session.is_none() {
                    let _ = connect(
                        &mut session,
                        &addresses,
                        &mut tried,
                        &job.id,
                        &peer_id,
                        piece_map.count,
                    )
                    .await;
                    attach_source(&mut session, &storage);
                }
                set_status(&job, JobStatus::Seeding);
                seed(
                    &mut session,
                    &job,
                    &mut uploaded,
                    &known,
                    &have,
                    &mut announced,
                )
                .await;
            } else {
                set_status(&job, JobStatus::Completed);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }

        // The pieces that are still missing, in the order a film plays in.
        let missing: Vec<u64> = (0..piece_map.count)
            .filter(|index| !have[*index as usize])
            .collect();

        // A peer has to be connected before we can ask what it holds.
        if session.is_none() {
            match connect(
                &mut session,
                &addresses,
                &mut tried,
                &job.id,
                &peer_id,
                piece_map.count,
            )
            .await
            {
                Ok(()) => {
                    attach_source(&mut session, &storage);
                    // The peer is told which pieces we already hold, so it
                    // neither offers them again nor thinks we are empty.
                    announce_have(&mut session, &have, &mut announced).await;
                }
                Err(_) => {
                    set_status(&job, JobStatus::NoPeers);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            }
        }
        // The borrow of the peer ends here, so a miss can drop the session.
        let choice = {
            let peer = session.as_mut().expect("a peer");
            choose_piece(&missing, |index| peer.has_piece(index), job.sequential)
        };
        let wanted = match choice {
            Some(index) => index,
            None => {
                // No piece this peer can give: ask the next one instead.
                session = None;
                continue;
            }
        };

        let length = piece_map.length_of(wanted);
        // The piece that is on its way is said out loud, so the window can
        // draw it apart from the pieces that are there.
        if let Ok(mut state) = job.state.lock() {
            state.inflight = Some(wanted);
        }
        let peer = session.as_mut().expect("a peer");
        match peer.download_piece(wanted as u32, length).await {
            Ok(data) => {
                let hash = hex_encode(&sha1_bytes(&data));
                if hash != metainfo.pieces[wanted as usize] {
                    // The peer sent something else than the piece it promised.
                    if let Ok(mut state) = job.state.lock() {
                        state.inflight = None;
                    }
                    session = None;
                    continue;
                }
                let storage = storage.as_ref().expect("the files");
                if storage.write_piece(wanted, &data).await.is_err() {
                    set_failed(&job, "the file could not be written".into());
                    if let Ok(mut state) = job.state.lock() {
                        state.inflight = None;
                    }
                    session = None;
                    continue;
                }
                have[wanted as usize] = true;
                downloaded += length;
                let peer = session.as_mut().expect("a peer");
                let _ = peer.send(&Message::Have(wanted as u32)).await;
                for index in storage.finished_files(&have) {
                    let _ = storage.finish_file(index).await;
                }
                if let Ok(mut state) = job.state.lock() {
                    state.downloaded = downloaded;
                    state.have = have.clone();
                    state.inflight = None;
                    set_peers(&mut state, &known, &session);
                }
                set_status(&job, JobStatus::Downloading);
            }
            Err(_) => {
                if let Ok(mut state) = job.state.lock() {
                    state.inflight = None;
                }
                session = None;
            }
        }
    }
    // Nothing counts the speeds of a job that is not there any more, and the
    // local network is told the torrent is no longer held here.
    discovery::lpd_forget(&job);
    speeds.abort();
}

/// Connect to the next peers that were not tried yet, a handful at once.
/// The first peer that completes the handshake is kept and the rest are
/// dropped: most peers a tracker or the table names sit behind a router
/// that takes no connection in, and asking them one at a time would spend
/// minutes on the few that never answer.
async fn connect(
    session: &mut Option<PeerSession>,
    addresses: &[SocketAddr],
    tried: &mut Vec<SocketAddr>,
    info_hash: &str,
    peer_id: &[u8; 20],
    piece_count: u64,
) -> Result<(), TorrentError> {
    let batch: Vec<SocketAddr> = addresses
        .iter()
        .filter(|address| !tried.contains(address))
        .copied()
        .take(8)
        .collect();
    if batch.is_empty() {
        // Every peer that was named has been tried. Starting the list again
        // is what turns a swarm of dead addresses into a download: the
        // trackers are only asked again every half hour, and a peer that was
        // quiet once may answer on the next pass.
        tried.clear();
        return Err(TorrentError::Network("there is no peer to talk to".into()));
    }
    tried.extend(batch.iter().copied());
    let hash = crate::util::hex_decode(info_hash)
        .ok_or_else(|| TorrentError::Malformed("the info hash is not hex".into()))?;
    let mut bytes = [0u8; 20];
    bytes.copy_from_slice(&hash);
    let id = *peer_id;
    let mut asking = tokio::task::JoinSet::new();
    for address in batch {
        asking.spawn(async move {
            tokio::time::timeout(
                Duration::from_secs(10),
                PeerSession::connect(address, bytes, id, piece_count),
            )
            .await
            .ok()?
            .ok()
        });
    }
    while let Some(answer) = asking.join_next().await {
        if let Ok(Some(peer)) = answer {
            asking.abort_all();
            *session = Some(peer);
            return Ok(());
        }
    }
    Err(TorrentError::Network("there is no peer to talk to".into()))
}

/// Hand the peer the files to serve, so a request is answered from disk
/// instead of from a session that is not allowed to read it.
fn attach_source(session: &mut Option<PeerSession>, storage: &Option<Arc<Storage>>) {
    if let (Some(peer), Some(storage)) = (session.as_mut(), storage) {
        peer.set_source(storage.clone());
    }
}

/// Tell a peer what we hold, so it neither offers the pieces we have nor asks
/// for what we cannot give. A peer hears it once per connection, right after
/// the handshake; a bitfield at any other moment is not the protocol.
async fn announce_have(session: &mut Option<PeerSession>, have: &[bool], announced: &mut bool) {
    if *announced || have.is_empty() {
        return;
    }
    if let Some(peer) = session.as_mut() {
        if peer.send_bitfield(have).await.is_ok() {
            *announced = true;
        }
    }
}

/// Stay with the peer and give it what it asks for. The peer learns what we
/// have before it can ask: a bitfield, and the unchoke that answers for it.
async fn seed(
    session: &mut Option<PeerSession>,
    job: &Arc<Job>,
    uploaded: &mut u64,
    known: &[PeerInfo],
    have: &[bool],
    announced: &mut bool,
) {
    let Some(peer) = session.as_mut() else {
        *announced = false;
        return;
    };
    if !*announced {
        let _ = peer.send_bitfield(have).await;
        *announced = true;
    }
    // A peer only asks for a block it is allowed to ask for, so the unchoke
    // is said again for a peer that connected while the job was downloading.
    let _ = peer.send(&Message::Unchoke).await;
    let hung_up = match tokio::time::timeout(Duration::from_secs(120), peer.read_message()).await {
        Ok(Ok(Message::Request {
            index,
            begin,
            length,
        })) => {
            let _ = peer.serve_request(index, begin, length).await;
            false
        }
        Ok(Ok(_)) => false,
        Ok(Err(_)) => true,
        Err(_) => {
            let _ = peer.send(&Message::KeepAlive).await;
            false
        }
    };
    *uploaded = peer.uploaded;
    if let Ok(mut state) = job.state.lock() {
        state.uploaded = *uploaded;
        set_peers(&mut state, known, session);
    }
    if hung_up {
        *session = None;
        *announced = false;
    }
}

/// Whether a job has shared as much as the reader allowed: the ratio it stops
/// at, or the minutes it keeps sharing. A zero is no limit.
fn shared_enough(job: &Arc<Job>, downloaded: u64, uploaded: u64) -> bool {
    let limits = job.control.limits();
    if limits.seed_ratio > 0.0
        && downloaded > 0
        && uploaded as f64 / downloaded as f64 >= limits.seed_ratio
    {
        return true;
    }
    if limits.seed_minutes > 0 {
        let minutes = Duration::from_secs(limits.seed_minutes.saturating_mul(60));
        if let Ok(state) = job.state.lock() {
            if state
                .seeding_since
                .is_some_and(|since| since.elapsed() >= minutes)
            {
                return true;
            }
        }
    }
    false
}

/// Stop sharing a job that has given what the reader allowed. The bytes are
/// not touched: what came in stays.
fn stop_seeding(job: &Arc<Job>) {
    job.control.set_seeding(false);
    if let Ok(mut state) = job.state.lock() {
        state.seeding = false;
        state.seeding_since = None;
    }
}

/// The peers of a job, as they are shown: the ones the trackers named and the
/// one we are talking to, each with the program it runs and the speed it was
/// last measured at.
fn peers_of(
    known: &[PeerInfo],
    session: &Option<PeerSession>,
    previous: &[PeerInfo],
) -> Vec<PeerInfo> {
    let mut peers = known.to_vec();
    if let Some(peer) = session {
        let live = PeerInfo {
            address: peer.address.to_string(),
            peer_id: hex_encode(&peer.peer_id),
            client: client_name(&peer.peer_id),
            connected: true,
            downloaded: peer.downloaded,
            uploaded: peer.uploaded,
            progress: peer.progress(),
            down_speed: 0,
            up_speed: 0,
        };
        match peers.iter_mut().find(|known| known.address == live.address) {
            Some(known) => *known = live,
            None => peers.push(live),
        }
    }
    // A peer keeps the speed it was measured at: the counters carry on from
    // the last second, not from zero.
    for peer in peers.iter_mut() {
        if let Some(before) = previous
            .iter()
            .find(|before| before.address == peer.address)
        {
            peer.down_speed = before.down_speed;
            peer.up_speed = before.up_speed;
            if peer.client.is_empty() {
                peer.client = before.client.clone();
            }
        }
    }
    peers
}

/// Put the peers of a job in its state, keeping what was measured of them.
fn set_peers(state: &mut JobState, known: &[PeerInfo], session: &Option<PeerSession>) {
    let previous = std::mem::take(&mut state.peers);
    state.peers = peers_of(known, session, &previous);
}

/// Say what the job is doing.
fn set_status(job: &Arc<Job>, status: JobStatus) {
    if let Ok(mut state) = job.state.lock() {
        if state.status != status {
            state.status = status;
        }
    }
}

/// Say that the job stopped with an error.
fn set_failed(job: &Arc<Job>, error: String) {
    if let Ok(mut state) = job.state.lock() {
        state.status = JobStatus::Failed(error.clone());
        state.error = Some(error);
    }
}

mod discovery;
#[cfg(test)]
mod tests;
mod trackers;
