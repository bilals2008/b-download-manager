//! The table driven end to end inside one process: nodes on the loopback
//! socket, an announce that travels from one to another, and a question that
//! finds it again — the same exchange the real table makes, only small.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

use super::krpc::{decode, encode_query, Message, Query};
use super::{Dht, PORT};
use crate::bencode::BencodeValue;

/// A node of the test, listening on a port of its own on the loopback.
fn a_node(hosts: Vec<String>) -> std::sync::Arc<Dht> {
    Dht::start_with_nodes("127.0.0.1:0".parse().expect("the address"), hosts)
        .expect("the node started")
}

/// Ask a node for the peers of a hash until the one that announced itself is
/// among them, or until the time is up.
async fn wait_for_peers(node: &Dht, hash: [u8; 20], want: SocketAddr) -> Vec<SocketAddr> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let peers = node.peers(hash).await;
        if peers.contains(&want) {
            return peers;
        }
        if Instant::now() >= deadline {
            panic!("the peer {want} was not found; the node named {peers:?}");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::test]
async fn a_peer_announced_to_one_node_is_found_by_another() {
    let hash = [0x42u8; 20];
    let announced: SocketAddr = "127.0.0.1:4242".parse().expect("the address");

    // A knows nobody; B starts by saying hello to A, so the two know each
    // other. A announces the torrent, and the announcement lands on B, the
    // only node A knows.
    let a = a_node(Vec::new());
    let b = a_node(vec![a.address().to_string()]);
    a.announce(hash, announced.port()).await;
    let held = wait_for_peers(&b, hash, announced).await;
    assert!(held.contains(&announced));

    // C knows only B. Its question travels the wire: B answers with the
    // peers it holds, which is the announcement A made.
    let c = a_node(vec![b.address().to_string()]);
    let found = wait_for_peers(&c, hash, announced).await;
    assert!(found.contains(&announced));
}

#[tokio::test]
async fn a_token_is_handed_out_and_taken_back_over_the_wire() {
    let node = a_node(Vec::new());
    let socket = UdpSocket::bind("127.0.0.1:0").await.expect("the socket");
    let hash = [0x13u8; 20];
    let id = vec![9u8; 20];

    // The question is get_peers; the answer carries a token.
    let question = encode_query(
        b"aa",
        Query::GetPeers,
        vec![
            ("id".into(), BencodeValue::Bytes(id.clone())),
            ("info_hash".into(), BencodeValue::Bytes(hash.to_vec())),
        ],
    );
    socket.send_to(&question, node.address()).await.expect("the question");
    let answer = read(&socket).await;
    let Message::Response { transaction, body } = answer else {
        panic!("an answer was expected, got {answer:?}");
    };
    assert_eq!(transaction, b"aa");
    let token = match body.iter().find(|(name, _)| name == "token") {
        Some((_, BencodeValue::Bytes(bytes))) => bytes.clone(),
        _ => panic!("the answer carried no token"),
    };

    // The announce takes the token back, and is taken.
    let announce = encode_query(
        b"bb",
        Query::AnnouncePeer,
        vec![
            ("id".into(), BencodeValue::Bytes(id.clone())),
            ("info_hash".into(), BencodeValue::Bytes(hash.to_vec())),
            ("port".into(), BencodeValue::Integer(51413)),
            ("token".into(), BencodeValue::Bytes(token)),
        ],
    );
    socket.send_to(&announce, node.address()).await.expect("the announce");
    let answer = read(&socket).await;
    assert!(matches!(answer, Message::Response { .. }));

    // The next question finds the peer the announce named.
    socket.send_to(&question, node.address()).await.expect("the question");
    let answer = read(&socket).await;
    let Message::Response { body, .. } = answer else {
        panic!("an answer was expected, got {answer:?}");
    };
    let values = match body.iter().find(|(name, _)| name == "values") {
        Some((_, BencodeValue::List(values))) => values.clone(),
        _ => panic!("the answer carried no peers"),
    };
    let peers: Vec<SocketAddr> = values
        .iter()
        .filter_map(|value| match value {
            BencodeValue::Bytes(bytes) => Some(bytes.clone()),
            _ => None,
        })
        .flat_map(|bytes| super::krpc::decode_peers(&bytes))
        .collect();
    let want: SocketAddr = "127.0.0.1:51413".parse().expect("the address");
    assert!(peers.contains(&want));
}

#[tokio::test]
async fn an_announce_with_a_foreign_token_is_refused() {
    let node = a_node(Vec::new());
    let socket = UdpSocket::bind("127.0.0.1:0").await.expect("the socket");
    let announce = encode_query(
        b"cc",
        Query::AnnouncePeer,
        vec![
            ("id".into(), BencodeValue::Bytes(vec![9u8; 20])),
            ("info_hash".into(), BencodeValue::Bytes(vec![0x13u8; 20])),
            ("port".into(), BencodeValue::Integer(51413)),
            ("token".into(), BencodeValue::Bytes(b"not the token".to_vec())),
        ],
    );
    socket.send_to(&announce, node.address()).await.expect("the announce");
    let answer = read(&socket).await;
    let Message::Error { transaction, code, .. } = answer else {
        panic!("a refusal was expected, got {answer:?}");
    };
    assert_eq!(transaction, b"cc");
    assert_eq!(code, 203);
}

#[tokio::test]
async fn a_ping_is_answered_with_the_id_of_the_node() {
    let node = a_node(Vec::new());
    let socket = UdpSocket::bind("127.0.0.1:0").await.expect("the socket");
    let ping = encode_query(
        b"dd",
        Query::Ping,
        vec![("id".into(), BencodeValue::Bytes(vec![9u8; 20]))],
    );
    socket.send_to(&ping, node.address()).await.expect("the ping");
    let answer = read(&socket).await;
    let Message::Response { transaction, body } = answer else {
        panic!("an answer was expected, got {answer:?}");
    };
    assert_eq!(transaction, b"dd");
    let id = match body.iter().find(|(name, _)| name == "id") {
        Some((_, BencodeValue::Bytes(bytes))) => bytes.clone(),
        _ => panic!("the answer carried no id"),
    };
    assert_eq!(id, node.id().to_vec());
}

/// Read one message off the socket, with a deadline so a lost datagram
/// fails the test instead of hanging it.
async fn read(socket: &UdpSocket) -> Message {
    let mut buffer = vec![0u8; 4096];
    let (length, _) = tokio::time::timeout(Duration::from_secs(5), socket.recv_from(&mut buffer))
        .await
        .expect("an answer in time")
        .expect("the socket");
    decode(&buffer[..length]).expect("the message")
}

/// The port the table listens on is the port the trackers are told about.
#[test]
fn the_table_and_the_tracker_speak_of_one_port() {
    assert_eq!(PORT, 6881);
}