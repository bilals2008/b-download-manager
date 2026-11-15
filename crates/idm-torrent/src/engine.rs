//! The engine of a job: the trackers, the peers, the pieces and the files.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::TorrentError;
use crate::magnet::MagnetInfo;
use crate::metainfo::{metainfo_from_metadata, Metainfo, TorrentFile};
use crate::peer::{Message, PeerInfo};
use crate::peerconn::PeerSession;
use crate::pieces::{file_selection, PieceMap};
use crate::session::JobStatus;
use crate::storage::Storage;
use crate::tracker;
use crate::util::{hex_encode, sha1_bytes};

/// The port the tracker is told about. Nothing listens on it: this crate
/// never takes a connection in.
const PORT: u16 = 6881;

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
}

impl Control {
    pub fn new() -> Control {
        Control {
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            seeding: AtomicBool::new(false),
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
}

/// What a job knows about itself right now.
pub(crate) struct JobState {
    pub status: JobStatus,
    pub downloaded: u64,
    pub uploaded: u64,
    pub peers: Vec<PeerInfo>,
    pub have: Vec<bool>,
    pub metainfo: Option<Metainfo>,
    pub magnet: Option<MagnetInfo>,
    pub files: Vec<TorrentFile>,
    pub seeding: bool,
    pub error: Option<String>,
}

impl JobState {
    pub fn new() -> JobState {
        JobState {
            status: JobStatus::Idle,
            downloaded: 0,
            uploaded: 0,
            peers: Vec::new(),
            have: Vec::new(),
            metainfo: None,
            magnet: None,
            files: Vec::new(),
            seeding: false,
            error: None,
        }
    }
}

/// One job.
pub(crate) struct Job {
    pub id: String,
    pub name: String,
    pub directory: std::path::PathBuf,
    pub source: Source,
    pub wanted: Vec<usize>,
    pub control: Arc<Control>,
    pub state: Arc<Mutex<JobState>>,
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

        // The trackers are asked now and then who has the torrent.
        if Instant::now() >= next_announce {
            next_announce = Instant::now() + interval;
            tried.clear();
            let left = metainfo.as_ref().map(|m| m.total_length).unwrap_or(0);
            for tracker in job.trackers() {
                match tracker::announce(
                    &tracker,
                    &job.id,
                    &peer_id_text,
                    PORT,
                    "started",
                    left,
                )
                .await
                {
                    Ok(result) => {
                        for address in result.peers {
                            if !addresses.contains(&address) {
                                addresses.push(address);
                                known.push(PeerInfo {
                                    address: address.to_string(),
                                    ..PeerInfo::default()
                                });
                            }
                        }
                        if result.interval > 0 {
                            interval = Duration::from_secs(result.interval.clamp(30, 3600));
                        }
                        if let Ok(mut state) = job.state.lock() {
                            state.peers = peers_of(&known, &session);
                        }
                    }
                    Err(error) => {
                        if let Ok(mut state) = job.state.lock() {
                            state.error = Some(error.to_string());
                        }
                    }
                }
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
            match peer.fetch_metadata().await {
                Ok(metadata) => {
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
                Err(_) => {
                    session = None;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
        }

        let metainfo = metainfo.as_ref().expect("the metadata");
        let piece_map = PieceMap::new(metainfo.piece_length, metainfo.total_length);

        // The files are opened once the metadata is there.
        if storage.is_none() {
            let chosen = file_selection(metainfo, &job.wanted);
            match Storage::new(&job.directory, metainfo, &chosen).await {
                Ok(files) => {
                    have = vec![false; piece_map.count as usize];
                    storage = Some(Arc::new(files));
                    if let Ok(mut state) = job.state.lock() {
                        state.have = have.clone();
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
                seed(&mut session, &job, &mut uploaded, &known, &have, &mut announced).await;
            } else {
                set_status(&job, JobStatus::Completed);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }

        // The piece to get next: the first one that is missing, so a film
        // can be played while it is still coming in.
        let missing: Vec<u64> = (0..piece_map.count)
            .filter(|index| !have[*index as usize])
            .collect();
        let wanted = missing[0];

        // The peer has to have it.
        if session.is_none() {
            match connect(&mut session, &addresses, &mut tried, &job.id, &peer_id, piece_map.count)
                .await
            {
                Ok(()) => {
                    attach_source(&mut session, &storage);
                }
                Err(_) => {
                    set_status(&job, JobStatus::NoPeers);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            }
        }
        let peer = session.as_mut().expect("a peer");
        if !peer.has_piece(wanted as u32) {
            // Another peer may have the piece we want; this one does not.
            let mut keep = false;
            for index in &missing {
                if peer.has_piece(*index as u32) {
                    keep = true;
                    break;
                }
            }
            if !keep {
                session = None;
                continue;
            }
        }
        let wanted = missing
            .iter()
            .find(|index| peer.has_piece(**index as u32))
            .copied()
            .unwrap_or(wanted);

        let length = piece_map.length_of(wanted);
        let peer = session.as_mut().expect("a peer");
        match peer.download_piece(wanted as u32, length).await {
            Ok(data) => {
                let hash = hex_encode(&sha1_bytes(&data));
                if hash != metainfo.pieces[wanted as usize] {
                    // The peer sent something else than the piece it promised.
                    session = None;
                    continue;
                }
                let storage = storage.as_ref().expect("the files");
                if storage.write_piece(wanted, &data).await.is_err() {
                    set_failed(&job, "the file could not be written".into());
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
                    state.peers = peers_of(&known, &session);
                }
                set_status(&job, JobStatus::Downloading);
            }
            Err(_) => {
                session = None;
            }
        }
    }
}

/// Connect to the next peer that was not tried yet.
async fn connect(
    session: &mut Option<PeerSession>,
    addresses: &[SocketAddr],
    tried: &mut Vec<SocketAddr>,
    info_hash: &str,
    peer_id: &[u8; 20],
    piece_count: u64,
) -> Result<(), TorrentError> {
    let Some(address) = addresses
        .iter()
        .find(|address| !tried.contains(address))
        .copied()
    else {
        return Err(TorrentError::Network("there is no peer to talk to".into()));
    };
    tried.push(address);
    let hash = crate::util::hex_decode(info_hash)
        .ok_or_else(|| TorrentError::Malformed("the info hash is not hex".into()))?;
    let mut bytes = [0u8; 20];
    bytes.copy_from_slice(&hash);
    *session = Some(PeerSession::connect(address, bytes, *peer_id, piece_count).await?);
    Ok(())
}

/// Hand the peer the files to serve, so a request is answered from disk
/// instead of from a session that is not allowed to read it.
fn attach_source(session: &mut Option<PeerSession>, storage: &Option<Arc<Storage>>) {
    if let (Some(peer), Some(storage)) = (session.as_mut(), storage) {
        peer.set_source(storage.clone());
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
        let _ = peer.send(&Message::Unchoke).await;
        *announced = true;
    }
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
        state.peers = peers_of(&known, session);
    }
    if hung_up {
        *session = None;
        *announced = false;
    }
}

/// The peers of a job, as they are shown: the ones the trackers named and the
/// one we are talking to.
fn peers_of(known: &[PeerInfo], session: &Option<PeerSession>) -> Vec<PeerInfo> {
    let mut peers = known.to_vec();
    if let Some(peer) = session {
        let live = PeerInfo {
            address: peer.address.to_string(),
            peer_id: hex_encode(&peer.peer_id),
            connected: true,
            downloaded: peer.downloaded,
            uploaded: peer.uploaded,
            progress: peer.progress(),
        };
        match peers.iter_mut().find(|known| known.address == live.address) {
            Some(known) => *known = live,
            None => peers.push(live),
        }
    }
    peers
}

/// Say what the job is doing.
fn set_status(job: &Arc<Job>, status: JobStatus) {
    if let Ok(mut state) = job.state.lock() {
        if state.status != status {
            state.status = status;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bencode::{bencode_encode, BencodeValue};
    use crate::session::{JobRequest, JobSource, TorrentClient};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn content() -> Vec<u8> {
        (0u8..=255).cycle().take(2048).collect()
    }

    fn a_torrent(announce: &str, bytes: &[u8]) -> Vec<u8> {
        let pieces = [&bytes[..1024], &bytes[1024..]]
            .iter()
            .flat_map(|chunk| sha1_bytes(chunk))
            .collect();
        let info = BencodeValue::Dict(vec![
            ("length".into(), BencodeValue::Integer(2048)),
            ("name".into(), BencodeValue::Bytes(b"hello.txt".to_vec())),
            ("piece length".into(), BencodeValue::Integer(1024)),
            ("pieces".into(), BencodeValue::Bytes(pieces)),
        ]);
        bencode_encode(&BencodeValue::Dict(vec![
            (
                "announce".into(),
                BencodeValue::Bytes(announce.as_bytes().to_vec()),
            ),
            ("info".into(), info),
        ]))
    }

    async fn a_tracker(peer: std::net::SocketAddr) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let bytes = peer.ip().to_string().parse::<std::net::Ipv4Addr>().unwrap();
        let port = peer.port();
        tokio::spawn(async move {
            for _ in 0..8 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![0u8; 2048];
                let read = socket.read(&mut request).await.unwrap_or(0);
                if read == 0 {
                    continue;
                }
                // The announce names the torrent; the answer names the one peer.
                let body = bencode_encode(&BencodeValue::Dict(vec![
                    ("interval".into(), BencodeValue::Integer(1800)),
                    (
                        "peers".into(),
                        BencodeValue::Bytes(
                            [bytes.octets().as_slice(), port.to_be_bytes().as_slice()].concat(),
                        ),
                    ),
                ]));
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&body).await;
            }
        });
        format!("http://{address}/announce")
    }

    /// One peer that has both pieces and answers what is asked of it.
    async fn a_peer(bytes: Vec<u8>) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut hello = [0u8; 68];
            socket.read_exact(&mut hello).await.unwrap();
            let info_hash: [u8; 20] = hello[28..48].try_into().unwrap();
            let mut response = vec![19u8];
            response.extend_from_slice(b"BitTorrent protocol");
            response.extend_from_slice(&hello[20..28]);
            response.extend_from_slice(&info_hash);
            response.extend_from_slice(&[9u8; 20]);
            socket.write_all(&response).await.unwrap();
            // Both pieces are there.
            let bitfield = Message::to_wire(&Message::Bitfield(vec![0b1100_0000]));
            socket.write_all(&bitfield).await.unwrap();
            loop {
                let mut length = [0u8; 4];
                if socket.read_exact(&mut length).await.is_err() {
                    return;
                }
                let length = u32::from_be_bytes(length) as usize;
                if length == 0 {
                    continue; // a keep-alive
                }
                let mut body = vec![0u8; length];
                if socket.read_exact(&mut body).await.is_err() {
                    return;
                }
                if body[0] != 6 {
                    continue; // not a request; nothing owed back
                }
                let index = u32::from_be_bytes(body[1..5].try_into().unwrap()) as u64;
                let begin = u32::from_be_bytes(body[5..9].try_into().unwrap()) as u64;
                let amount = u32::from_be_bytes(body[9..13].try_into().unwrap()) as u64;
                let start = (index * 1024 + begin) as usize;
                let data = bytes[start..start + amount as usize].to_vec();
                let piece = Message::to_wire(&Message::Piece {
                    index: index as u32,
                    begin: begin as u32,
                    data,
                });
                if socket.write_all(&piece).await.is_err() {
                    return;
                }
            }
        });
        address
    }

    #[tokio::test]
    async fn a_job_downloads_its_pieces_from_a_peer() {
        let bytes = content();
        let peer = a_peer(bytes.clone()).await;
        let tracker = a_tracker(peer).await;
        let torrent = a_torrent(&tracker, &bytes);
        let directory = std::env::temp_dir().join(format!("idm-torrent-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);

        let client = TorrentClient::new();
        let id = client
            .add_job(JobRequest {
                source: JobSource::Bytes(torrent),
                directory: directory.clone(),
                wanted: Vec::new(),
            })
            .await
            .expect("the job was added");

        let mut finished = None;
        for _ in 0..100 {
            if let Some(job) = client.job(&id) {
                match job.status {
                    JobStatus::Completed => {
                        finished = Some(job);
                        break;
                    }
                    JobStatus::Failed(why) => panic!("the job failed: {why}"),
                    _ => {}
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let job = finished.expect("the job did not finish in time");
        assert_eq!(job.downloaded, 2048);
        let written = std::fs::read(directory.join("hello.txt")).expect("the file is there");
        assert_eq!(written, bytes);
        let _ = client.remove(&id, true).await;
        let _ = std::fs::remove_dir_all(&directory);
    }
}

/// Say that the job stopped with an error.
fn set_failed(job: &Arc<Job>, error: String) {
    if let Ok(mut state) = job.state.lock() {
        state.status = JobStatus::Failed(error.clone());
        state.error = Some(error);
    }
}
