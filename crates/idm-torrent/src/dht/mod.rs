//! The distributed hash table: how a torrent no tracker knows about finds
//! its peers anyway.
//!
//! The table is the one the specification describes: a kademlia tree of
//! twenty-byte ids, spoken over UDP with the KRPC messages of bencoded
//! dictionaries. Four questions are spoken — ping, find_node, get_peers and
//! announce_peer — and every answer is a list of ids, of addresses, or of
//! both.
//!
//! One socket, one task, one table of questions. Every question is written
//! down with the time it may be asked again, and a question that is not
//! answered in time is asked again and then dropped, so nothing waits forever
//! and nothing grows without a bound.

pub mod krpc;
pub mod lookup;
pub mod routing;
pub mod store;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};

use crate::bencode::BencodeValue;
use crate::dht::krpc::{encode_error, encode_query, encode_response, Message, NodeId, Query};
use crate::dht::lookup::Lookup;
use crate::dht::routing::RoutingTable;
use crate::dht::store::Store;
use crate::error::TorrentError;
use crate::util::sha1_bytes;

/// The port the table listens on.
///
/// It is the port the trackers are told about, so the mapping the router
/// makes for this client is the mapping the table is reached on.
pub const PORT: u16 = 6881;

/// The nodes the table starts from.
pub const BOOTSTRAP: [(&str, u16); 3] = [
    ("router.bittorrent.com", 6881),
    ("dht.transmissionbt.com", 6881),
    ("router.utorrent.com", 6881),
];

/// How long a question waits for its answer before it is asked again.
const RETRY: Duration = Duration::from_secs(3);
/// How many times a question is asked.
const TRIES: u8 = 3;
/// How often the table looks at its own questions.
const TICK: Duration = Duration::from_millis(250);
/// How many questions of one walk are on the wire at the same time.
const PARALLEL: usize = 4;
/// The most bytes a message may be.
const MESSAGE: usize = 4096;

/// What the reader of this module asks of the table.
enum Command {
    /// The peers of an info hash.
    Peers {
        hash: [u8; 20],
        reply: oneshot::Sender<Vec<SocketAddr>>,
    },
    /// This client has the torrent of an info hash.
    Announce { hash: [u8; 20], port: u16 },
    /// Say hello to more nodes, as the reader's settings name them.
    AddNodes { hosts: Vec<String> },
}

/// The table of this process.
pub struct Dht {
    /// Where the reader asks for something.
    commands: mpsc::Sender<Command>,
    /// The address the table listens on.
    address: SocketAddr,
    /// The id of this node.
    id: NodeId,
}

impl Dht {
    /// Start the table, listening on an address, saying hello to the standard
    /// bootstrap nodes and to the ones the reader's settings name.
    pub fn start(bind: SocketAddr) -> Result<Arc<Dht>, TorrentError> {
        let mut hosts: Vec<String> = BOOTSTRAP
            .iter()
            .map(|(host, port)| format!("{host}:{port}"))
            .collect();
        hosts.extend(extra_nodes());
        Self::start_with_nodes(bind, hosts)
    }

    /// Start the table, saying hello to exactly the nodes that are named.
    ///
    /// The socket is bound straight away, so the caller knows the address
    /// even when it asked for port zero. A node is a `host:port` text; an
    /// empty list starts a table that waits to be spoken to.
    pub fn start_with_nodes(bind: SocketAddr, hosts: Vec<String>) -> Result<Arc<Dht>, TorrentError> {
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(TorrentError::Failed(
                "the table needs a runtime to run in".into(),
            ));
        }
        let socket = std::net::UdpSocket::bind(bind)?;
        socket.set_nonblocking(true)?;
        let address = socket.local_addr()?;
        let id = node_id();
        let (commands, receiver) = mpsc::channel(16);
        let handle = Arc::new(Dht {
            commands,
            address,
            id,
        });
        let task = handle.clone();
        tokio::spawn(async move {
            let socket = match UdpSocket::from_std(socket) {
                Ok(socket) => socket,
                Err(_) => return,
            };
            run(socket, receiver, task.id, hosts).await;
        });
        Ok(handle)
    }

    /// The address the table listens on.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The id of this node.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// The peers of an info hash, as far as the table knows them.
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

    /// Say that this client has the torrent of an info hash.
    pub async fn announce(&self, hash: [u8; 20], port: u16) {
        let _ = self.commands.send(Command::Announce { hash, port }).await;
    }
}

/// The one table of this process, started when it is first asked for.
pub fn shared() -> Option<Arc<Dht>> {
    let cell = SHARED.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().ok()?;
    if let Some(handle) = guard.as_ref() {
        return Some(handle.clone());
    }
    let handle = Dht::start(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), PORT)).ok()?;
    *guard = Some(handle.clone());
    Some(handle)
}

/// Where the one table of this process is kept.
static SHARED: OnceLock<Mutex<Option<Arc<Dht>>>> = OnceLock::new();

/// The nodes the reader's settings name, on top of the standard ones.
///
/// They are kept for the life of the process only: nothing is written to the
/// disk, so every run starts from the standard nodes and these again.
static EXTRA: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

/// Take the nodes the reader's settings name. A table that is already
/// running says hello to them at once; one that is not yet started takes
/// them with it when it starts.
pub fn set_nodes(hosts: Vec<String>) {
    let cell = EXTRA.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut held) = cell.lock() {
        *held = hosts.clone();
    }
    if let Some(cell) = SHARED.get() {
        if let Ok(guard) = cell.lock() {
            if let Some(handle) = guard.as_ref() {
                let _ = handle.commands.try_send(Command::AddNodes { hosts });
            }
        }
    }
}

/// The nodes the settings named, as they are now.
fn extra_nodes() -> Vec<String> {
    EXTRA
        .get()
        .and_then(|cell| cell.lock().ok().map(|held| held.clone()))
        .unwrap_or_default()
}

/// The id of this node: twenty bytes that are different every run.
fn node_id() -> NodeId {
    let mut seed = Vec::new();
    seed.extend_from_slice(b"idm-torrent-dht");
    seed.extend_from_slice(&std::process::id().to_le_bytes());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    seed.extend_from_slice(&nanos.to_le_bytes());
    sha1_bytes(&seed)
}

/// A question that is on the wire.
struct Pending {
    /// The node the question was sent to.
    node: SocketAddr,
    /// The question itself, so it can be sent again.
    bytes: Vec<u8>,
    /// How many times it has been sent.
    tries: u8,
    /// When it may be sent again.
    due: Instant,
    /// The walk it belongs to, when it belongs to one.
    walk: Option<u64>,
}

/// Everything the task of the table holds.
struct Table {
    /// The id of this node.
    id: NodeId,
    /// The routing table.
    routing: RoutingTable,
    /// The values this node holds.
    store: Store,
    /// The questions that are on the wire.
    pending: HashMap<Vec<u8>, Pending>,
    /// The walks that are running.
    walks: Vec<Walk>,
    /// The number of the next transaction.
    transactions: AtomicU16,
    /// The number of the next walk.
    walks_number: AtomicU64,
}

/// A walk, with the number that names it.
struct Walk {
    /// The number of the walk.
    number: u64,
    /// The walk itself.
    lookup: Lookup,
}

impl Table {
    /// The number of the next transaction.
    fn transaction(&self) -> Vec<u8> {
        let number = self.transactions.fetch_add(1, Ordering::Relaxed);
        number.to_be_bytes().to_vec()
    }
}

/// Run the table until the reader goes.
async fn run(
    socket: UdpSocket,
    mut commands: mpsc::Receiver<Command>,
    id: NodeId,
    hosts: Vec<String>,
) {
    let secret = sha1_bytes(&[id.as_slice(), b"secret".as_slice()].concat());
    let mut table = Table {
        id,
        routing: RoutingTable::new(id),
        store: Store::new(secret),
        pending: HashMap::new(),
        walks: Vec::new(),
        transactions: AtomicU16::new(0),
        walks_number: AtomicU64::new(0),
    };
    bootstrap(&socket, &mut table, &hosts).await;

    let mut buffer = vec![0u8; MESSAGE];
    loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(Command::Peers { hash, reply }) => {
                        // A value this node holds itself is an answer already;
                        // the walk is only for what it does not have.
                        let held = table.store.peers(&hash);
                        if !held.is_empty() {
                            let _ = reply.send(held);
                        } else {
                            let seeds = table.routing.closest(&hash, 16);
                            let number = table.walks_number.fetch_add(1, Ordering::Relaxed);
                            table.walks.push(Walk {
                                number,
                                lookup: Lookup::new(
                                    table.id,
                                    hash,
                                    None,
                                    seeds.into_iter().map(|node| (node.id, node.address)).collect(),
                                    reply,
                                ),
                            });
                        }
                    }
                    Some(Command::Announce { hash, port }) => {
                        let seeds = table.routing.closest(&hash, 16);
                        let number = table.walks_number.fetch_add(1, Ordering::Relaxed);
                        let (reply, _kept) = oneshot::channel();
                        table.walks.push(Walk {
                            number,
                            lookup: Lookup::new(
                                table.id,
                                hash,
                                Some(port),
                                seeds.into_iter().map(|node| (node.id, node.address)).collect(),
                                reply,
                            ),
                        });
                    }
                    Some(Command::AddNodes { hosts }) => {
                        bootstrap(&socket, &mut table, &hosts).await;
                    }
                    None => break,
                }
            }
            received = socket.recv_from(&mut buffer) => {
                let Ok((length, from)) = received else {
                    continue;
                };
                let message = buffer[..length].to_vec();
                answer_from(&socket, &mut table, &message, from).await;
            }
            _ = tokio::time::sleep(TICK) => {
                expire(&socket, &mut table).await;
            }
        }
        drive(&socket, &mut table).await;
    }
}

/// Say hello to the nodes the table starts from. A name is resolved here,
/// once, so the rest of the table only ever speaks to addresses.
async fn bootstrap(socket: &UdpSocket, table: &mut Table, hosts: &[String]) {
    let mut addresses: Vec<SocketAddr> = Vec::new();
    for host in hosts.iter().take(32) {
        let Ok(found) = tokio::net::lookup_host(host.as_str()).await else {
            continue;
        };
        for address in found {
            if !addresses.contains(&address) {
                addresses.push(address);
            }
        }
    }
    for address in addresses {
        let transaction = table.transaction();
        let bytes = encode_query(
            &transaction,
            Query::Ping,
            vec![("id".into(), BencodeValue::Bytes(table.id.to_vec()))],
        );
        if socket.send_to(&bytes, address).await.is_err() {
            continue;
        }
        table.pending.insert(
            transaction,
            Pending {
                node: address,
                bytes,
                tries: 1,
                due: Instant::now() + RETRY,
                walk: None,
            },
        );
    }
}

/// Read what came in, and answer it.
async fn answer_from(socket: &UdpSocket, table: &mut Table, bytes: &[u8], from: SocketAddr) {
    let message = match krpc::decode(bytes) {
        Ok(message) => message,
        Err(_) => return,
    };
    match message {
        Message::Response { transaction, body } => {
            // An answer carries the id of the node that sent it, so the node
            // is put into the table.
            if let Some(BencodeValue::Bytes(id)) = body
                .iter()
                .find(|(name, _)| name == "id")
                .map(|(_, value)| value)
            {
                if id.len() == 20 {
                    let mut node_id = [0u8; 20];
                    node_id.copy_from_slice(id);
                    table.routing.add(node_id, from);
                }
            }
            let Some(pending) = table.pending.remove(&transaction) else {
                return;
            };
            if let Some(number) = pending.walk {
                if let Some(walk) = table.walks.iter_mut().find(|walk| walk.number == number) {
                    walk.lookup.answer(from, &body);
                }
            }
        }
        Message::Error { transaction, .. } => {
            table.pending.remove(&transaction);
        }
        Message::Query {
            transaction,
            query,
            arguments,
        } => {
            serve(socket, table, transaction, query, arguments, from).await;
        }
    }
}

/// Answer a question of another node.
async fn serve(
    socket: &UdpSocket,
    table: &mut Table,
    transaction: Vec<u8>,
    query: Query,
    arguments: Vec<(String, BencodeValue)>,
    from: SocketAddr,
) {
    let argument = |name: &str| {
        arguments
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    };
    // A question carries the id of the node that asked, so that node is put
    // into the table.
    if let Some(BencodeValue::Bytes(id)) = argument("id") {
        if id.len() == 20 {
            let mut node_id = [0u8; 20];
            node_id.copy_from_slice(id);
            table.routing.add(node_id, from);
        }
    }

    let body = match query {
        Query::Ping => Some(vec![("id".into(), BencodeValue::Bytes(table.id.to_vec()))]),
        Query::FindNode => {
            let target = match argument("target") {
                Some(BencodeValue::Bytes(bytes)) if bytes.len() == 20 => {
                    let mut target = [0u8; 20];
                    target.copy_from_slice(bytes);
                    target
                }
                _ => {
                    let _ = socket
                        .send_to(&encode_error(&transaction, 203, "the target is not twenty bytes"), from)
                        .await;
                    return;
                }
            };
            let nodes = table.routing.closest(&target, 8);
            Some(vec![
                ("id".into(), BencodeValue::Bytes(table.id.to_vec())),
                (
                    "nodes".into(),
                    BencodeValue::Bytes(krpc::encode_nodes(
                        &nodes
                            .into_iter()
                            .map(|node| (node.id, node.address))
                            .collect::<Vec<_>>(),
                    )),
                ),
            ])
        }
        Query::GetPeers => {
            let hash = match argument("info_hash") {
                Some(BencodeValue::Bytes(bytes)) if bytes.len() == 20 => {
                    let mut hash = [0u8; 20];
                    hash.copy_from_slice(bytes);
                    hash
                }
                _ => {
                    let _ = socket
                        .send_to(
                            &encode_error(&transaction, 203, "the info hash is not twenty bytes"),
                            from,
                        )
                        .await;
                    return;
                }
            };
            let peers = table.store.peers(&hash);
            let nodes = table.routing.closest(&hash, 8);
            let mut body = vec![
                ("id".into(), BencodeValue::Bytes(table.id.to_vec())),
                (
                    "token".into(),
                    BencodeValue::Bytes(table.store.token(from.ip())),
                ),
            ];
            if !peers.is_empty() {
                body.push(("values".into(), krpc::encode_peer_values(&peers)));
            }
            body.push((
                "nodes".into(),
                BencodeValue::Bytes(krpc::encode_nodes(
                    &nodes
                        .into_iter()
                        .map(|node| (node.id, node.address))
                        .collect::<Vec<_>>(),
                )),
            ));
            Some(body)
        }
        Query::AnnouncePeer => {
            let hash = match argument("info_hash") {
                Some(BencodeValue::Bytes(bytes)) if bytes.len() == 20 => {
                    let mut hash = [0u8; 20];
                    hash.copy_from_slice(bytes);
                    hash
                }
                _ => {
                    let _ = socket
                        .send_to(
                            &encode_error(&transaction, 203, "the info hash is not twenty bytes"),
                            from,
                        )
                        .await;
                    return;
                }
            };
            let port = match argument("port") {
                Some(BencodeValue::Integer(port)) if (0..=65535).contains(port) => *port as u16,
                _ => {
                    let _ = socket
                        .send_to(&encode_error(&transaction, 203, "the port is not a port"), from)
                        .await;
                    return;
                }
            };
            let token = match argument("token") {
                Some(BencodeValue::Bytes(bytes)) => bytes.clone(),
                _ => Vec::new(),
            };
            if !table.store.accepts(from.ip(), &token) {
                let _ = socket
                    .send_to(
                        &encode_error(&transaction, 203, "the token is not one of ours"),
                        from,
                    )
                    .await;
                return;
            }
            table
                .store
                .announce(hash, SocketAddr::new(from.ip(), port));
            Some(vec![
                ("id".into(), BencodeValue::Bytes(table.id.to_vec())),
                (
                    "token".into(),
                    BencodeValue::Bytes(table.store.token(from.ip())),
                ),
            ])
        }
    };

    if let Some(body) = body {
        let _ = socket
            .send_to(&encode_response(&transaction, body), from)
            .await;
    }
}

/// Ask the nodes of the walks, and finish the walks that are over.
async fn drive(socket: &UdpSocket, table: &mut Table) {
    for index in 0..table.walks.len() {
        let number = table.walks[index].number;
        // A walk that started before the table knew any node takes the ones
        // it has learned since, so an early question is not answered with
        // nothing while the bootstrap answers are still on their way.
        if table.walks[index].lookup.needs_seeds() {
            let hash = table.walks[index].lookup.hash();
            let seeds = table.routing.closest(&hash, 16);
            table.walks[index]
                .lookup
                .seed(seeds.into_iter().map(|node| (node.id, node.address)).collect());
        }
        while !table.walks[index].lookup.finished()
            && table
                .pending
                .values()
                .filter(|pending| pending.walk == Some(number))
                .count()
                < PARALLEL
        {
            let Some((address, arguments)) = table.walks[index].lookup.next() else {
                break;
            };
            let transaction = table.transaction();
            let bytes = encode_query(&transaction, Query::GetPeers, arguments);
            if socket.send_to(&bytes, address).await.is_err() {
                continue;
            }
            table.pending.insert(
                transaction,
                Pending {
                    node: address,
                    bytes,
                    tries: 1,
                    due: Instant::now() + RETRY,
                    walk: Some(number),
                },
            );
        }
    }

    // The walks that are over are answered, and the ones that announce this
    // client say so to the nodes that gave a token. A walk is only over once
    // every question it asked has been answered or given up on: an answer
    // still on its way may carry the token the announcement needs, or name
    // the nodes the walk goes on to ask.
    let mut finished: Vec<u64> = Vec::new();
    for walk in &table.walks {
        let outstanding = table
            .pending
            .values()
            .any(|pending| pending.walk == Some(walk.number));
        if walk.lookup.finished() && !outstanding {
            finished.push(walk.number);
        }
    }
    for number in finished {
        let Some(index) = table.walks.iter().position(|walk| walk.number == number) else {
            continue;
        };
        let mut walk = table.walks.remove(index);
        let hash = walk.lookup.hash();
        if let Some(reply) = walk.lookup.take_reply() {
            let _ = reply.send(walk.lookup.peers());
        }
        if let Some(port) = walk.lookup.announce_port() {
            for (node, token) in walk.lookup.announcements() {
                let transaction = table.transaction();
                let bytes = encode_query(
                    &transaction,
                    Query::AnnouncePeer,
                    vec![
                        ("id".into(), BencodeValue::Bytes(table.id.to_vec())),
                        ("info_hash".into(), BencodeValue::Bytes(hash.to_vec())),
                        ("port".into(), BencodeValue::Integer(port as i64)),
                        ("token".into(), BencodeValue::Bytes(token)),
                    ],
                );
                if socket.send_to(&bytes, node).await.is_err() {
                    continue;
                }
                table.pending.insert(
                    transaction,
                    Pending {
                        node,
                        bytes,
                        tries: 1,
                        due: Instant::now() + RETRY,
                        walk: None,
                    },
                );
            }
        }
        table.pending.retain(|_, pending| pending.walk != Some(number));
    }
}

/// Ask again the questions that were not answered, and drop the ones that
/// were not answered at all.
async fn expire(socket: &UdpSocket, table: &mut Table) {
    let now = Instant::now();
    let mut again = Vec::new();
    let mut done = Vec::new();
    for (transaction, pending) in &table.pending {
        if pending.due > now {
            continue;
        }
        if pending.tries < TRIES {
            again.push(transaction.clone());
        } else {
            done.push(transaction.clone());
        }
    }
    for transaction in again {
        let Some((bytes, node)) = table.pending.get_mut(&transaction).map(|pending| {
            pending.tries += 1;
            pending.due = now + RETRY;
            (pending.bytes.clone(), pending.node)
        }) else {
            continue;
        };
        let _ = socket.send_to(&bytes, node).await;
    }
    for transaction in done {
        table.pending.remove(&transaction);
    }
    table.store.expire();
}
