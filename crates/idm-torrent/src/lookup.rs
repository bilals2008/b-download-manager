//! Reading a torrent before it is started: its name and its files.
//!
//! A `.torrent` file carries that already. A magnet link does not: the file
//! list has to come from a peer, in the same exchange the engine makes once a
//! job runs. Add torrent asks here first, so the reader can untick what they do
//! not want before a byte is written.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::dht::{self, Dht};
use crate::error::TorrentError;
use crate::magnet::parse_magnet;
use crate::metainfo::{metainfo_from_metadata, parse_torrent, Metainfo};
use crate::options::TorrentOptions;

use crate::peerconn::PeerSession;
use crate::session::JobSource;
use crate::tracker;
use crate::util::{hex_decode, hex_encode, peer_id};

/// How long a magnet may spend finding a peer that has the file list.
const DEADLINE: Duration = Duration::from_secs(60);

/// How long one peer gets to answer hello, and then to send the file list.
const HELLO: Duration = Duration::from_secs(8);
const LIST: Duration = Duration::from_secs(25);

/// How many peers are asked at once. Most peers a table names sit behind a
/// router that takes no connection in; asking one at a time would spend the
/// whole deadline on the few that never answer.
const BATCH: usize = 8;

/// What a torrent holds.
pub async fn lookup(source: &JobSource) -> Result<Metainfo, TorrentError> {
    lookup_with(source, &TorrentOptions::default()).await
}

/// What a torrent holds, under the reader's choices: the proxy carries the
/// tracker announces, the way a running job's do.
pub async fn lookup_with(
    source: &JobSource,
    options: &TorrentOptions,
) -> Result<Metainfo, TorrentError> {
    match source {
        JobSource::Bytes(bytes) => parse_torrent(bytes),
        JobSource::Magnet(link) => {
            lookup_magnet(link, dht::shared(), options.proxy.as_deref()).await
        }
    }
}

/// Ask a magnet's peers for its file list. The trackers are asked first; a
/// magnet that names none, or whose trackers name no peer, is looked up in
/// the distributed table instead. The table is handed over so a test can
/// drive the exchange with nodes of its own.
async fn lookup_magnet(
    link: &str,
    table: Option<Arc<Dht>>,
    proxy: Option<&str>,
) -> Result<Metainfo, TorrentError> {
    let magnet = parse_magnet(link)?;
    let hash = hex_decode(&magnet.info_hash)
        .ok_or_else(|| TorrentError::Malformed("the info hash is not hex".into()))?;
    if hash.len() != 20 {
        return Err(TorrentError::Malformed(
            "the info hash is not 20 bytes".into(),
        ));
    }
    let mut info_hash = [0u8; 20];
    info_hash.copy_from_slice(&hash);

    let peer_id = peer_id();
    let peer_text = hex_encode(&peer_id);
    let mut addresses: Vec<SocketAddr> = Vec::new();
    for tracker_url in &magnet.trackers {
        if let Ok(result) = tracker::announce(
            tracker_url,
            &magnet.info_hash,
            &peer_text,
            dht::PORT,
            "started",
            0,
            proxy,
        )
        .await
        {
            for address in result.peers {
                if !addresses.contains(&address) {
                    addresses.push(address);
                }
            }
        }
    }

    // No tracker named a peer: the table is asked for them. The walk is
    // bounded by its own deadline and query cap, so this waits a while and
    // never forever.
    if addresses.is_empty() {
        if let Some(table) = &table {
            for address in table.peers(info_hash).await {
                if !addresses.contains(&address) {
                    addresses.push(address);
                }
            }
        }
    }

    let started = Instant::now();
    let mut index = 0;
    while index < addresses.len() && started.elapsed() < DEADLINE {
        let batch: Vec<SocketAddr> =
            addresses[index..(index + BATCH).min(addresses.len())].to_vec();
        index += BATCH;
        let mut asking = tokio::task::JoinSet::new();
        for address in batch {
            asking.spawn(async move {
                let mut session = tokio::time::timeout(
                    HELLO,
                    PeerSession::connect(address, info_hash, peer_id, 0),
                )
                .await
                .ok()?
                .ok()?;
                let metadata = tokio::time::timeout(LIST, session.fetch_metadata())
                    .await
                    .ok()?
                    .ok()?;
                Some(metadata)
            });
        }
        while let Some(answer) = asking.join_next().await {
            let Ok(Some(metadata)) = answer else {
                continue;
            };
            match metainfo_from_metadata(
                &metadata,
                magnet.trackers.clone(),
                Some(&magnet.info_hash),
            ) {
                Ok(found) => {
                    asking.abort_all();
                    return Ok(found);
                }
                // Metadata of another torrent is not metadata; ask the rest.
                Err(_) => continue,
            }
        }
    }

    Err(TorrentError::Network(
        "no peer sent the file list; the torrent may have no peers right now".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bencode::{bencode_encode, BencodeValue};

    /// One file of twelve bytes, cut into a single piece.
    fn a_torrent() -> Vec<u8> {
        let info = BencodeValue::Dict(vec![
            ("length".into(), BencodeValue::Integer(12)),
            ("name".into(), BencodeValue::Bytes(b"one.bin".to_vec())),
            ("piece length".into(), BencodeValue::Integer(16)),
            ("pieces".into(), BencodeValue::Bytes(vec![0u8; 20])),
        ]);
        bencode_encode(&BencodeValue::Dict(vec![
            ("info".into(), info),
            (
                "announce".into(),
                BencodeValue::Bytes(b"http://127.0.0.1:1/announce".to_vec()),
            ),
        ]))
    }

    #[tokio::test]
    async fn a_torrent_file_is_read_without_a_peer() {
        let metainfo = lookup(&JobSource::Bytes(a_torrent()))
            .await
            .expect("the file list");
        assert_eq!(metainfo.name, "one.bin");
        assert_eq!(metainfo.files.len(), 1);
        assert_eq!(metainfo.total_length, 12);
    }

    #[tokio::test]
    async fn a_magnet_without_trackers_says_so() {
        let link = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567";
        // Without a table there is nobody to ask beyond the trackers, and
        // the magnet names none: the lookup says so instead of waiting.
        let failed = lookup_magnet(link, None, None).await;
        assert!(failed.is_err());
    }

    #[tokio::test]
    async fn a_magnet_without_trackers_takes_the_peers_of_the_table() {
        let hash = [0x21u8; 20];
        let link = format!("magnet:?xt=urn:btih:{}", hex_encode(&hash));
        // A peer that announced itself to the table but listens on nothing:
        // the lookup must take it from the table and try it, which fails
        // here because the port is closed. The point is that the table's
        // peer reached the connection loop at all.
        let dead: SocketAddr = "127.0.0.1:1".parse().expect("the address");
        let a = Dht::start_with_nodes("127.0.0.1:0".parse().expect("the address"), Vec::new())
            .expect("the node");
        let b = Dht::start_with_nodes(
            "127.0.0.1:0".parse().expect("the address"),
            vec![a.address().to_string()],
        )
        .expect("the node");
        a.announce(hash, dead.port()).await;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if b.peers(hash).await.contains(&dead) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the table did not take the announcement"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let failed = lookup_magnet(&link, Some(b), None).await;
        assert!(failed.is_err());
    }
}
