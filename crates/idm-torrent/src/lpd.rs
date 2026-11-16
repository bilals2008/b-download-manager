//! Local peer discovery (BEP 14): the torrents this client holds, announced
//! on the local network, and the peers that answer.
//!
//! The announce is an HTTP-like message on the multicast group the
//! specification names — `239.192.152.143:6771` — carrying the port the
//! peers are reached on and the info hashes this client has. A client that
//! hears a search for a hash it holds answers the same message, unicast to
//! the asker. Announces go out when a job starts and every five minutes
//! while it runs; a peer that is not heard from again is forgotten.
//!
//! One socket, one task. IPv4 only: the group of the specification is an
//! IPv4 one, and the IPv6 form is left for a network that asks for it.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};

use crate::error::TorrentError;
use crate::util::{hex_encode, peer_id};

/// The multicast group of the specification.
pub const GROUP: Ipv4Addr = Ipv4Addr::new(239, 192, 152, 143);
/// The port the group is spoken on.
pub const PORT: u16 = 6771;

/// How often an announce goes out again while a job runs.
const ANNOUNCE_EVERY: Duration = Duration::from_secs(300);
/// How long a peer is kept after it was last heard from.
const PEER_LIFETIME: Duration = Duration::from_secs(600);
/// How often the task looks at its announcements and its peers.
const TICK: Duration = Duration::from_secs(1);
/// The most bytes one message may be.
const MESSAGE: usize = 2048;

/// A search or an answer, as the specification writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Search {
    /// The port the sender's peers are reached on.
    pub port: u16,
    /// The info hashes the sender holds or looks for.
    pub hashes: Vec<[u8; 20]>,
    /// The sender's own mark, so it can tell its announcements from others'.
    pub cookie: String,
}

/// The message as it goes on the wire.
pub fn encode_search(hashes: &[[u8; 20]], port: u16, cookie: &str) -> Vec<u8> {
    let mut text = format!(
        "BT-SEARCH * HTTP/1.1\r\nHost: {GROUP}:{PORT}\r\nPort: {port}\r\n"
    );
    for hash in hashes {
        text.push_str(&format!("Infohash: {}\r\n", hex_encode(hash)));
    }
    text.push_str(&format!("cookie: {cookie}\r\n\r\n"));
    text.into_bytes()
}

/// Read a message that came in. Nothing that is not a search is one.
pub fn parse_search(bytes: &[u8]) -> Option<Search> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.split("\r\n");
    let first = lines.next()?;
    if !first.starts_with("BT-SEARCH") {
        return None;
    }
    let mut port = None;
    let mut hashes = Vec::new();
    let mut cookie = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "port" => port = value.parse().ok(),
            "infohash" => {
                let bytes = crate::util::hex_decode(value)?;
                let hash: [u8; 20] = bytes.try_into().ok()?;
                hashes.push(hash);
            }
            "cookie" => cookie = Some(value.to_string()),
            _ => {}
        }
    }
    Some(Search {
        port: port?,
        hashes,
        cookie: cookie.unwrap_or_default(),
    })
}

/// What the reader of this module asks of the discovery.
enum Command {
    /// Hold this torrent, and say so on the network.
    Announce { hash: [u8; 20] },
    /// The job is over; stop saying so, and forget its peers.
    Forget { hash: [u8; 20] },
    /// The peers heard for a torrent.
    Peers {
        hash: [u8; 20],
        reply: oneshot::Sender<Vec<SocketAddr>>,
    },
    /// Leave the group.
    Stop,
}

/// The local discovery of one client.
pub struct Lpd {
    commands: mpsc::Sender<Command>,
}

impl Lpd {
    /// Join the group and start listening. The port is the one the peers
    /// of this client are reached on, which every announce carries.
    pub fn start(port: u16) -> Result<Arc<Lpd>, TorrentError> {
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(TorrentError::Failed(
                "the discovery needs a runtime to run in".into(),
            ));
        }
        let socket = std::net::UdpSocket::bind(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            PORT,
        ))?;
        socket.set_nonblocking(true)?;
        let cookie = format!("idm-{}", hex_encode(&peer_id()[..8]));
        let (commands, receiver) = mpsc::channel(16);
        let handle = Arc::new(Lpd { commands });
        tokio::spawn(async move {
            let socket = match UdpSocket::from_std(socket) {
                Ok(socket) => socket,
                Err(_) => return,
            };
            if socket
                .join_multicast_v4(GROUP, Ipv4Addr::UNSPECIFIED)
                .is_err()
            {
                return;
            }
            run(socket, receiver, port, cookie).await;
        });
        Ok(handle)
    }

    /// Say this client holds a torrent. The announce goes out at once and
    /// again every few minutes while the job runs.
    pub fn announce(&self, hash: [u8; 20]) {
        let _ = self.commands.try_send(Command::Announce { hash });
    }

    /// The job is over.
    pub fn forget(&self, hash: [u8; 20]) {
        let _ = self.commands.try_send(Command::Forget { hash });
    }

    /// The peers heard for a torrent, deduplicated, freshest first.
    pub async fn peers(&self, hash: [u8; 20]) -> Vec<SocketAddr> {
        let (reply, answer) = oneshot::channel();
        if self
            .commands
            .send(Command::Peers { hash, reply })
            .await
            .is_err()
        {
            return Vec::new();
        }
        answer.await.unwrap_or_default()
    }

    /// Leave the group.
    pub fn stop(&self) {
        let _ = self.commands.try_send(Command::Stop);
    }
}

/// Everything the task holds.
struct Table {
    /// The torrents this client announces, and when each went out last.
    announced: HashMap<[u8; 20], Instant>,
    /// The peers heard, by torrent, and when each was heard last.
    peers: HashMap<[u8; 20], HashMap<IpAddr, (u16, Instant)>>,
}

/// Run the discovery until the reader goes.
async fn run(socket: UdpSocket, mut commands: mpsc::Receiver<Command>, port: u16, cookie: String) {
    let mut table = Table {
        announced: HashMap::new(),
        peers: HashMap::new(),
    };
    let group = SocketAddr::new(IpAddr::V4(GROUP), PORT);
    let mut buffer = vec![0u8; MESSAGE];
    loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(Command::Announce { hash }) => {
                        let message = encode_search(&[hash], port, &cookie);
                        let _ = socket.send_to(&message, group).await;
                        table.announced.insert(hash, Instant::now());
                    }
                    Some(Command::Forget { hash }) => {
                        table.announced.remove(&hash);
                        table.peers.remove(&hash);
                    }
                    Some(Command::Peers { hash, reply }) => {
                        let now = Instant::now();
                        let peers = table.peers.get(&hash).map(|heard| {
                            heard.iter()
                                .filter(|(_, (_, seen))| now.duration_since(*seen) < PEER_LIFETIME)
                                .map(|(ip, (peer_port, _))| SocketAddr::new(*ip, *peer_port))
                                .collect()
                        }).unwrap_or_default();
                        let _ = reply.send(peers);
                    }
                    Some(Command::Stop) | None => break,
                }
            }
            received = socket.recv_from(&mut buffer) => {
                let Ok((length, from)) = received else {
                    continue;
                };
                let Some(search) = parse_search(&buffer[..length]) else {
                    continue;
                };
                // Our own announcements come back from the group; they are
                // not news.
                if search.cookie == cookie {
                    continue;
                }
                for hash in &search.hashes {
                    table.peers.entry(*hash).or_default().insert(
                        from.ip(),
                        (search.port, Instant::now()),
                    );
                    // A search for a torrent we hold is answered directly.
                    if table.announced.contains_key(hash) {
                        let answer = encode_search(&[*hash], port, &cookie);
                        let _ = socket.send_to(&answer, from).await;
                    }
                }
            }
            _ = tokio::time::sleep(TICK) => {
                let now = Instant::now();
                for (hash, last) in table.announced.iter_mut() {
                    if now.duration_since(*last) >= ANNOUNCE_EVERY {
                        let message = encode_search(&[*hash], port, &cookie);
                        let _ = socket.send_to(&message, group).await;
                        *last = now;
                    }
                }
                for heard in table.peers.values_mut() {
                    heard.retain(|_, (_, seen)| now.duration_since(*seen) < PEER_LIFETIME);
                }
            }
        }
    }
    let _ = socket.leave_multicast_v4(GROUP, Ipv4Addr::UNSPECIFIED);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_goes_and_comes_back() {
        let hash = [0x42u8; 20];
        let bytes = encode_search(&[hash], 6881, "idm-test");
        let search = parse_search(&bytes).expect("the search");
        assert_eq!(search.port, 6881);
        assert_eq!(search.hashes, vec![hash]);
        assert_eq!(search.cookie, "idm-test");
    }

    #[test]
    fn several_hashes_travel_in_one_message() {
        let hashes = [[1u8; 20], [2u8; 20], [3u8; 20]];
        let bytes = encode_search(&hashes, 51413, "c");
        let search = parse_search(&bytes).expect("the search");
        assert_eq!(search.hashes, hashes.to_vec());
        assert_eq!(search.port, 51413);
    }

    #[test]
    fn the_headers_are_read_whatever_their_case() {
        let text = b"BT-SEARCH * HTTP/1.1\r\nHOST: 239.192.152.143:6771\r\nPORT: 6881\r\nINFOHASH: 0123456789abcdef0123456789abcdef01234567\r\nCOOKIE: x\r\n\r\n";
        let search = parse_search(text).expect("the search");
        assert_eq!(search.port, 6881);
        assert_eq!(search.hashes.len(), 1);
        assert_eq!(search.cookie, "x");
    }

    #[test]
    fn what_is_not_a_search_is_not_read() {
        assert!(parse_search(b"GET / HTTP/1.1\r\n\r\n").is_none());
        assert!(parse_search(b"BT-SEARCH * HTTP/1.1\r\n\r\n").is_none());
        assert!(parse_search(b"BT-SEARCH * HTTP/1.1\r\nPort: not a port\r\n\r\n").is_none());
        assert!(parse_search(b"BT-SEARCH * HTTP/1.1\r\nPort: 6881\r\nInfohash: zz\r\n\r\n").is_none());
    }
}