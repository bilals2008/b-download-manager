//! Reading a torrent before it is started: its name and its files.
//!
//! A `.torrent` file carries that already. A magnet link does not: the file
//! list has to come from a peer, in the same exchange the engine makes once a
//! job runs. Add torrent asks here first, so the reader can untick what they do
//! not want before a byte is written.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::error::TorrentError;
use crate::magnet::parse_magnet;
use crate::metainfo::{metainfo_from_metadata, parse_torrent, Metainfo};

use crate::peerconn::PeerSession;
use crate::session::JobSource;
use crate::tracker;
use crate::util::{hex_decode, hex_encode, peer_id};

/// The port a tracker is told about. Nothing listens on it: this crate never
/// takes a connection in.
const PORT: u16 = 6881;

/// How long a magnet may spend finding a peer that has the file list.
const DEADLINE: Duration = Duration::from_secs(60);

/// How long one peer gets to answer hello, and then to send the file list.
const HELLO: Duration = Duration::from_secs(15);
const LIST: Duration = Duration::from_secs(20);

/// What a torrent holds.
pub async fn lookup(source: &JobSource) -> Result<Metainfo, TorrentError> {
    match source {
        JobSource::Bytes(bytes) => parse_torrent(bytes),
        JobSource::Magnet(link) => lookup_magnet(link).await,
    }
}

/// Ask a magnet's peers for its file list.
async fn lookup_magnet(link: &str) -> Result<Metainfo, TorrentError> {
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
            PORT,
            "started",
            0,
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

    let started = Instant::now();
    for address in addresses {
        if started.elapsed() > DEADLINE {
            break;
        }
        let session =
            tokio::time::timeout(HELLO, PeerSession::connect(address, info_hash, peer_id, 0)).await;
        let Ok(Ok(mut session)) = session else {
            continue;
        };
        let metadata = tokio::time::timeout(LIST, session.fetch_metadata()).await;
        if let Ok(Ok(metadata)) = metadata {
            match metainfo_from_metadata(
                &metadata,
                magnet.trackers.clone(),
                Some(&magnet.info_hash),
            ) {
                Ok(found) => return Ok(found),
                // Metadata of another torrent is not metadata; ask the next peer.
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
        let failed = lookup(&JobSource::Magnet(link.into())).await;
        assert!(failed.is_err());
    }
}
