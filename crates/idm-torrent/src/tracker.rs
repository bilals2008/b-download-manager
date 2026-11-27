//! The trackers: the addresses a torrent is told about, and the peers they
//! answer with.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use crate::bencode::bencode_decode;
use crate::error::TorrentError;
use crate::util::hex_decode;

/// What a tracker answered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnnounceResult {
    /// The peers the tracker knows.
    pub peers: Vec<SocketAddr>,
    /// How many seconds to wait before asking again.
    pub interval: u64,
    /// How many peers have the whole torrent.
    pub complete: u64,
    /// How many peers have part of it.
    pub incomplete: u64,
}

/// How long one UDP tracker may take to answer a question, both for the
/// connection id and for the peer list.
const UDP_ANSWER: Duration = Duration::from_secs(8);

/// How long a UDP tracker may take from the first datagram to the last. A
/// tracker that never answers must not hold the job: the other trackers and
/// the peer table are waiting behind it.
const UDP_TOTAL: Duration = Duration::from_secs(18);

/// Tell a tracker who we are and ask it for peers. The proxy, when there is
/// one, carries the HTTP announce; a UDP tracker is reached directly, the
/// way the protocol is spoken.
pub async fn announce(
    tracker: &str,
    info_hash: &str,
    peer_id: &str,
    port: u16,
    event: &str,
    left: u64,
    proxy: Option<&str>,
) -> Result<AnnounceResult, TorrentError> {
    let hash = hex_decode(info_hash)
        .ok_or_else(|| TorrentError::Malformed("the info hash is not hex".into()))?;
    let id = hex_decode(peer_id)
        .ok_or_else(|| TorrentError::Malformed("the peer id is not hex".into()))?;

    if tracker.starts_with("udp://") {
        return tokio::time::timeout(
            UDP_TOTAL,
            udp_announce(tracker, &hash, &id, port, event, left),
        )
        .await
        .unwrap_or_else(|_| {
            Err(TorrentError::Network(format!(
                "the tracker {tracker} did not answer in time"
            )))
        });
    }
    if !tracker.starts_with("http://") && !tracker.starts_with("https://") {
        return Err(TorrentError::Unsupported(format!(
            "the tracker {tracker} is not one this crate can reach"
        )));
    }
    http_announce(tracker, &hash, &id, port, event, left, proxy).await
}

/// Ask a tracker over HTTP, through the proxy when one is set.
async fn http_announce(
    tracker: &str,
    info_hash: &[u8],
    peer_id: &[u8],
    port: u16,
    event: &str,
    left: u64,
    proxy: Option<&str>,
) -> Result<AnnounceResult, TorrentError> {
    let url = announce_url(tracker, info_hash, peer_id, port, event, left);
    let client = http_client(proxy)?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    if !response.status().is_success() {
        return Err(TorrentError::Network(format!(
            "the tracker answered {}",
            response.status()
        )));
    }
    let body = response
        .bytes()
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    parse_answer(&body)
}

/// The HTTP client an announce leaves through: the reader's proxy when there
/// is one, a direct connection when there is not.
pub(crate) fn http_client(proxy: Option<&str>) -> Result<reqwest::Client, TorrentError> {
    let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(20));
    if let Some(proxy) = proxy.filter(|proxy| !proxy.is_empty()) {
        let proxy = reqwest::Proxy::all(proxy)
            .map_err(|e| TorrentError::Network(format!("the proxy is not usable: {e}")))?;
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|e| TorrentError::Network(e.to_string()))
}

/// Ask a tracker over UDP (BEP 15).
async fn udp_announce(
    tracker: &str,
    info_hash: &[u8],
    peer_id: &[u8],
    port: u16,
    event: &str,
    left: u64,
) -> Result<AnnounceResult, TorrentError> {
    let rest = &tracker["udp://".len()..];
    let (host, path) = match rest.split_once('/') {
        Some((host, path)) => (host, path),
        None => (rest, ""),
    };
    let (host, port_text) = match host.rsplit_once(':') {
        Some((host, port)) => (host, port),
        None => (host, "80"),
    };
    let tracker_port: u16 = port_text.parse().map_err(|_| {
        TorrentError::Malformed(format!("the tracker port {port_text} is not a number"))
    })?;

    let mut addresses = tokio::net::lookup_host((host, tracker_port))
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    // The socket is IPv4, so an IPv6 address for the same host is not the one
    // to use: a host with both answers was failing its announce before.
    let address = addresses
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| TorrentError::Network(format!("{host} has no address")))?;
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    socket
        .connect(address)
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;

    let transaction = u32::from_le_bytes([peer_id[0], peer_id[1], peer_id[2], peer_id[3]]);
    let mut connect = Vec::with_capacity(16);
    connect.extend_from_slice(&0x41727101980u64.to_be_bytes());
    connect.extend_from_slice(&0u32.to_be_bytes());
    connect.extend_from_slice(&transaction.to_be_bytes());
    socket
        .send(&connect)
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;

    let mut buffer = [0u8; 2048];
    let read = tokio::time::timeout(UDP_ANSWER, socket.recv(&mut buffer))
        .await
        .map_err(|_| TorrentError::Network(format!("{host} did not answer the connect")))?
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    if read < 16 {
        return Err(TorrentError::Network(
            "the tracker answered with too little".into(),
        ));
    }
    // A connect answer is action 0 with the transaction id it was asked with;
    // anything else is another message and its bytes are not a connection id.
    let action = u32::from_be_bytes(buffer[0..4].try_into().unwrap_or([0; 4]));
    let answered = u32::from_be_bytes(buffer[4..8].try_into().unwrap_or([0; 4]));
    if action != 0 || answered != transaction {
        return Err(TorrentError::Network(
            "the tracker did not answer the connect".into(),
        ));
    }
    let connection_id = u64::from_be_bytes(buffer[8..16].try_into().unwrap_or([0; 8]));

    let mut request = Vec::with_capacity(98);
    request.extend_from_slice(&connection_id.to_be_bytes());
    request.extend_from_slice(&1u32.to_be_bytes());
    request.extend_from_slice(&transaction.to_be_bytes());
    request.extend_from_slice(info_hash);
    request.extend_from_slice(peer_id);
    request.extend_from_slice(&0u64.to_be_bytes());
    request.extend_from_slice(&left.to_be_bytes());
    request.extend_from_slice(&0u64.to_be_bytes());
    request.extend_from_slice(&event_code(event).to_be_bytes());
    request.extend_from_slice(&0u32.to_be_bytes());
    request.extend_from_slice(&0u32.to_be_bytes());
    request.extend_from_slice(&50u32.to_be_bytes());
    request.extend_from_slice(&port.to_be_bytes());
    socket
        .send(&request)
        .await
        .map_err(|e| TorrentError::Network(e.to_string()))?;

    let read = tokio::time::timeout(UDP_ANSWER, socket.recv(&mut buffer))
        .await
        .map_err(|_| TorrentError::Network(format!("{host} did not answer the announce")))?
        .map_err(|e| TorrentError::Network(e.to_string()))?;
    if read < 20 {
        return Err(TorrentError::Network(
            "the tracker answered with too little".into(),
        ));
    }
    // An announce answer is action 1, its transaction id, the interval, the
    // leechers, the seeders and then six bytes a peer. Reading the seeders
    // out of the leechers' bytes is how a tracker of ten seeders was shown as
    // ten leechers, and the interval out of the transaction id made the job
    // wait an hour before asking anyone again.
    let action = u32::from_be_bytes(buffer[0..4].try_into().unwrap_or([0; 4]));
    let answered = u32::from_be_bytes(buffer[4..8].try_into().unwrap_or([0; 4]));
    if action != 1 || answered != transaction {
        let refusal = String::from_utf8_lossy(&buffer[8..read]);
        return Err(TorrentError::Network(format!(
            "the tracker refused the announce: {}",
            refusal.trim()
        )));
    }
    let interval = u32::from_be_bytes(buffer[8..12].try_into().unwrap_or([0; 4])) as u64;
    let incomplete = u32::from_be_bytes(buffer[12..16].try_into().unwrap_or([0; 4])) as u64;
    let complete = u32::from_be_bytes(buffer[16..20].try_into().unwrap_or([0; 4])) as u64;
    let mut peers = Vec::new();
    let mut at = 20;
    while at + 6 <= read {
        peers.push(peer_from_bytes(&buffer[at..at + 6]));
        at += 6;
    }
    let _ = path;
    Ok(AnnounceResult {
        peers,
        interval,
        complete,
        incomplete,
    })
}

/// The event a tracker is told about, as the number it uses.
fn event_code(event: &str) -> u32 {
    match event {
        "started" => 1,
        "stopped" => 2,
        "completed" => 3,
        _ => 0,
    }
}

/// The address an announce asks for.
fn announce_url(
    tracker: &str,
    info_hash: &[u8],
    peer_id: &[u8],
    port: u16,
    event: &str,
    left: u64,
) -> String {
    let separator = if tracker.contains('?') { "&" } else { "?" };
    format!(
        "{tracker}{separator}info_hash={}&peer_id={}&port={port}&uploaded=0&downloaded=0&left={left}&compact=1&event={event}",
        percent_encode(info_hash),
        percent_encode(peer_id)
    )
}

/// The bytes as they go into a query: everything that is not a letter or a
/// digit is written as its escape.
fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for byte in bytes {
        if byte.is_ascii_alphanumeric() {
            out.push(*byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out
}

/// What a tracker answered, read out of its bencode.
fn parse_answer(body: &[u8]) -> Result<AnnounceResult, TorrentError> {
    let value = bencode_decode(body)
        .map_err(|e| TorrentError::Malformed(format!("the tracker answered with junk: {e}")))?;
    if let Some(reason) = value.get("failure reason").and_then(|v| v.as_text()) {
        return Err(TorrentError::Network(format!("the tracker said {reason}")));
    }
    let interval = value.get("interval").and_then(|v| v.as_int()).unwrap_or(0) as u64;
    let complete = value.get("complete").and_then(|v| v.as_int()).unwrap_or(0) as u64;
    let incomplete = value
        .get("incomplete")
        .and_then(|v| v.as_int())
        .unwrap_or(0) as u64;

    let mut peers = Vec::new();
    match value.get("peers") {
        Some(crate::bencode::BencodeValue::Bytes(bytes)) => {
            let mut at = 0;
            while at + 6 <= bytes.len() {
                peers.push(peer_from_bytes(&bytes[at..at + 6]));
                at += 6;
            }
        }
        Some(crate::bencode::BencodeValue::List(list)) => {
            for entry in list {
                let ip = entry
                    .get("ip")
                    .and_then(|v| v.as_text())
                    .unwrap_or_default();
                let port = entry.get("port").and_then(|v| v.as_int()).unwrap_or(0);
                if let Ok(address) = ip.parse::<Ipv4Addr>() {
                    peers.push(SocketAddr::from((address, port as u16)));
                }
            }
        }
        _ => {}
    }

    Ok(AnnounceResult {
        peers,
        interval,
        complete,
        incomplete,
    })
}

/// An address out of the six bytes a tracker writes an address in.
fn peer_from_bytes(bytes: &[u8]) -> SocketAddr {
    let address = Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]);
    let port = u16::from_be_bytes([bytes[4], bytes[5]]);
    SocketAddr::from((address, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, UdpSocket};

    /// A tracker that answers with one peer.
    async fn a_tracker() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0u8; 2048];
            let read = socket.read(&mut request).await.unwrap();
            let text = String::from_utf8_lossy(&request[..read]).to_string();
            assert!(text.contains("info_hash="));
            assert!(text.contains("event=started"));
            let body = b"d8:intervali1800e5:peers6:\x7f\x00\x00\x01\x1a\xe1e";
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
            socket.flush().await.unwrap();
        });
        format!("http://{address}/announce")
    }

    #[tokio::test]
    async fn a_tracker_answers_with_a_peer() {
        let tracker = a_tracker().await;
        let result = announce(
            &tracker,
            &"0".repeat(40),
            &"1".repeat(40),
            6881,
            "started",
            0,
            None,
        )
        .await
        .unwrap();
        assert_eq!(result.peers.len(), 1);
        assert_eq!(result.peers[0].port(), 6881);
        assert_eq!(result.interval, 1800);
    }

    #[tokio::test]
    async fn a_tracker_that_is_not_there_is_an_error() {
        let error = announce(
            "http://127.0.0.1:1/announce",
            &"0".repeat(40),
            &"1".repeat(40),
            6881,
            "started",
            0,
            None,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("tracker"));
    }

    /// A tracker that speaks BEP 15: a connection id, then the peer list.
    async fn a_udp_tracker() -> String {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        tokio::spawn(async move {
            let mut request = [0u8; 2048];
            let (read, from) = socket.recv_from(&mut request).await.unwrap();
            assert_eq!(read, 16, "a connect request is sixteen bytes");
            let transaction = u32::from_be_bytes(request[12..16].try_into().unwrap());
            let mut reply = Vec::new();
            reply.extend_from_slice(&0u32.to_be_bytes());
            reply.extend_from_slice(&transaction.to_be_bytes());
            reply.extend_from_slice(&0x0123_4567_89ab_cdefu64.to_be_bytes());
            socket.send_to(&reply, from).await.unwrap();

            let (read, from) = socket.recv_from(&mut request).await.unwrap();
            assert_eq!(read, 98, "an announce request is ninety-eight bytes");
            let transaction = u32::from_be_bytes(request[12..16].try_into().unwrap());
            let mut reply = Vec::new();
            reply.extend_from_slice(&1u32.to_be_bytes());
            reply.extend_from_slice(&transaction.to_be_bytes());
            reply.extend_from_slice(&1800u32.to_be_bytes());
            reply.extend_from_slice(&7u32.to_be_bytes());
            reply.extend_from_slice(&3u32.to_be_bytes());
            reply.extend_from_slice(&[127, 0, 0, 1, 0x1a, 0xe1]);
            socket.send_to(&reply, from).await.unwrap();
        });
        format!("udp://{address}")
    }

    #[tokio::test]
    async fn a_udp_tracker_answers_with_its_counts_and_peers() {
        let tracker = a_udp_tracker().await;
        let result = announce(
            &tracker,
            &"0".repeat(40),
            &"1".repeat(40),
            6881,
            "started",
            0,
            None,
        )
        .await
        .expect("the tracker answered");
        assert_eq!(result.interval, 1800, "the interval is at bytes 8..12");
        assert_eq!(result.complete, 3, "the seeders are at bytes 16..20");
        assert_eq!(result.incomplete, 7, "the leechers are at bytes 12..16");
        assert_eq!(result.peers.len(), 1);
        assert_eq!(result.peers[0].port(), 6881);
    }

    #[tokio::test]
    async fn a_udp_tracker_that_never_answers_is_an_error() {
        // A socket that takes datagrams and says nothing. Before the deadline
        // this call waited forever, and every tracker behind it, the peer
        // table and the whole job waited with it.
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let silence = tokio::spawn(async move {
            let mut buffer = [0u8; 2048];
            let _ = socket.recv_from(&mut buffer).await;
            // Keep the socket open, and never answer.
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let tracker = format!("udp://{address}");
        let hash = "0".repeat(40);
        let peer = "1".repeat(40);
        let failure = announce(&tracker, &hash, &peer, 6881, "started", 0, None);
        let error = tokio::time::timeout(Duration::from_secs(25), failure)
            .await
            .expect("the announce gave up")
            .expect_err("no answer is an error");
        assert!(error.to_string().contains("did not answer"));
        silence.abort();
    }

    #[tokio::test]
    async fn an_announce_leaves_through_the_proxy() {
        // A proxy of our own: it takes the request, checks that the whole
        // tracker address is in the request line — which is what a proxied
        // plain-HTTP request carries — and answers as the tracker would.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0u8; 4096];
            let read = socket.read(&mut request).await.unwrap();
            let text = String::from_utf8_lossy(&request[..read]).to_string();
            assert!(
                text.starts_with("GET http://"),
                "the announce did not go through the proxy: {text}"
            );
            let body = b"d8:intervali1800e5:peers6:\x7f\x00\x00\x01\x1a\xe1e";
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
            socket.flush().await.unwrap();
        });
        let result = announce(
            "http://tracker.example/announce",
            &"0".repeat(40),
            &"1".repeat(40),
            6881,
            "started",
            0,
            Some(&format!("http://{proxy}")),
        )
        .await
        .unwrap();
        assert_eq!(result.peers.len(), 1);
    }

    #[test]
    fn no_proxy_is_a_direct_client_and_a_bad_proxy_is_an_error() {
        assert!(http_client(None).is_ok());
        assert!(http_client(Some("")).is_ok());
        assert!(http_client(Some("not a proxy address")).is_err());
    }
}
