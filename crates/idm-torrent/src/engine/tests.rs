//! The engine driven end to end: a torrent, a tracker, a peer, and the
//! bytes that land in the folder the job writes into.

use super::*;
use crate::bencode::{bencode_decode, bencode_encode, BencodeValue};
use crate::pieces::choose_piece;
use crate::session::{JobRequest, JobSource, TorrentClient};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn choose_piece_respects_the_order_setting() {
    let missing = vec![2u64, 5, 9];
    // The first missing piece is available: both modes take it.
    assert_eq!(choose_piece(&missing, |i| i == 2 || i == 9, false), Some(2));
    assert_eq!(choose_piece(&missing, |i| i == 2 || i == 9, true), Some(2));
    // A later piece is there but the first is not: only free order takes it.
    assert_eq!(choose_piece(&missing, |i| i == 5, false), Some(5));
    assert_eq!(choose_piece(&missing, |i| i == 5, true), None);
    // Nothing the peer has: nothing in either mode.
    assert_eq!(choose_piece(&missing, |_| false, false), None);
    assert_eq!(choose_piece(&missing, |_| false, true), None);
}

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

/// One peer that hands out the file list of a magnet (BEP 9) and then the
/// pieces it promised, over the one connection the client makes.
async fn a_magnet_peer(metadata: Vec<u8>, bytes: Vec<u8>) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        // The client comes back for the pieces on a second connection,
        // once it knows how many there are, so the peer stays listening.
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            let metadata = metadata.clone();
            let bytes = bytes.clone();
            tokio::spawn(async move { serve_magnet_peer(socket, metadata, bytes).await });
        }
    });
    address
}

async fn serve_magnet_peer(mut socket: tokio::net::TcpStream, metadata: Vec<u8>, bytes: Vec<u8>) {
    let mut hello = [0u8; 68];
    socket.read_exact(&mut hello).await.unwrap();
    let info_hash: [u8; 20] = hello[28..48].try_into().unwrap();
    let mut response = vec![19u8];
    response.extend_from_slice(b"BitTorrent protocol");
    response.extend_from_slice(&hello[20..28]);
    response.extend_from_slice(&info_hash);
    response.extend_from_slice(&[9u8; 20]);
    socket.write_all(&response).await.unwrap();
    // Both pieces are there, and the client reads the map first.
    socket
        .write_all(&Message::to_wire(&Message::Bitfield(vec![0b1100_0000])))
        .await
        .unwrap();
    let handshake = BencodeValue::Dict(vec![
        (
            "m".into(),
            BencodeValue::Dict(vec![("ut_metadata".into(), BencodeValue::Integer(1))]),
        ),
        (
            "metadata_size".into(),
            BencodeValue::Integer(metadata.len() as i64),
        ),
        ("v".into(), BencodeValue::Bytes(b"probe".to_vec())),
    ]);
    socket
        .write_all(&Message::to_wire(&Message::Extended {
            id: 0,
            payload: bencode_encode(&handshake),
        }))
        .await
        .unwrap();
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
        match Message::decode(body[0], &body[1..]) {
            Ok(Message::Extended { payload, .. }) => {
                let Ok(value) = bencode_decode(&payload) else {
                    continue;
                };
                // Only a request for the list is answered: the whole
                // list fits in one piece for a torrent this small.
                if value.get("msg_type").and_then(|kind| kind.as_int()) != Some(0) {
                    continue;
                }
                let reply = BencodeValue::Dict(vec![
                    ("msg_type".into(), BencodeValue::Integer(1)),
                    ("piece".into(), BencodeValue::Integer(0)),
                    (
                        "total_size".into(),
                        BencodeValue::Integer(metadata.len() as i64),
                    ),
                    ("data".into(), BencodeValue::Bytes(metadata.clone())),
                ]);
                let message = Message::Extended {
                    id: 1,
                    payload: bencode_encode(&reply),
                };
                if socket.write_all(&Message::to_wire(&message)).await.is_err() {
                    return;
                }
            }
            Ok(Message::Request {
                index,
                begin,
                length,
            }) => {
                let start = index as usize * 1024 + begin as usize;
                let data = bytes[start..start + length as usize].to_vec();
                let piece = Message::Piece { index, begin, data };
                if socket.write_all(&Message::to_wire(&piece)).await.is_err() {
                    return;
                }
            }
            _ => {}
        }
    }
}

#[tokio::test]
async fn a_magnet_job_reads_its_files_from_a_peer_and_downloads_them() {
    let bytes = content();
    let pieces: Vec<u8> = [&bytes[..1024], &bytes[1024..]]
        .iter()
        .flat_map(|chunk| sha1_bytes(chunk))
        .collect();
    let info = BencodeValue::Dict(vec![
        ("length".into(), BencodeValue::Integer(2048)),
        ("name".into(), BencodeValue::Bytes(b"hello.txt".to_vec())),
        ("piece length".into(), BencodeValue::Integer(1024)),
        ("pieces".into(), BencodeValue::Bytes(pieces)),
    ]);
    let metadata = bencode_encode(&info);
    // The magnet names the hash of the file list the peer will send, which
    // is what makes a magnet a torrent rather than an address.
    let hash = crate::util::hex_encode(&sha1_bytes(&metadata));
    let peer = a_magnet_peer(metadata, bytes.clone()).await;
    let tracker = a_tracker(peer).await;
    let link = format!("magnet:?xt=urn:btih:{hash}&dn=hello&tr={tracker}");
    let directory =
        std::env::temp_dir().join(format!("idm-torrent-magnet-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);

    let client = TorrentClient::new();
    let id = client
        .add_job(JobRequest {
            source: JobSource::Magnet(link),
            directory: directory.clone(),
            wanted: Vec::new(),
            start_paused: false,
            sequential: false,
        })
        .await
        .expect("the magnet was added");

    let mut finished = None;
    for _ in 0..120 {
        if let Some(job) = client.job(&id) {
            match job.status {
                JobStatus::Completed => {
                    finished = Some(job);
                    break;
                }
                JobStatus::Failed(why) => panic!("the magnet failed: {why}"),
                _ => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let job = finished.unwrap_or_else(|| {
        let seen = client
            .job(&id)
            .map(|job| format!("{:?} with {} bytes", job.status, job.downloaded))
            .unwrap_or_else(|| "gone".into());
        panic!("the magnet did not finish in time; the job was {seen}");
    });
    assert_eq!(job.downloaded, 2048);
    let written = std::fs::read(directory.join("hello.txt")).expect("the file is there");
    assert_eq!(written, bytes);
    let _ = client.remove(&id, true).await;
    let _ = std::fs::remove_dir_all(&directory);
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
            start_paused: false,
            sequential: false,
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

#[tokio::test]
async fn a_start_paused_job_is_queued_with_nothing_downloaded() {
    let directory = std::env::temp_dir().join(format!("idm-torrent-paused-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let client = TorrentClient::new();
    let id = client
        .add_job(JobRequest {
            source: JobSource::Magnet(format!(
                "magnet:?xt=urn:btih:{}",
                "0123456789abcdef0123456789abcdef01234567"
            )),
            directory: directory.clone(),
            wanted: Vec::new(),
            start_paused: true,
            sequential: false,
        })
        .await
        .expect("the magnet was added");
    let job = client.job(&id).expect("the job is there");
    assert_eq!(job.status, JobStatus::Paused);
    assert_eq!(job.downloaded, 0);
    let _ = client.remove(&id, true).await;
    let _ = std::fs::remove_dir_all(&directory);
}
