//! Magnet links: the info hash, the name the link carries and the trackers
//! it points at.

use serde::Serialize;

use crate::error::TorrentError;
use crate::util::{base32_decode, hex_decode, hex_encode, percent_decode};

/// What a magnet link says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MagnetInfo {
    /// The info hash, as 40 hex characters.
    pub info_hash: String,
    /// The name the link carries, when it carries one.
    pub name: Option<String>,
    /// The trackers the link points at.
    pub trackers: Vec<String>,
}

impl MagnetInfo {
    /// The name to show, when the link has none.
    pub fn display_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.info_hash.clone())
    }
}

/// Read a magnet link.
pub fn parse_magnet(link: &str) -> Result<MagnetInfo, TorrentError> {
    let rest = link
        .trim()
        .strip_prefix("magnet:")
        .ok_or_else(|| TorrentError::Malformed("the link is not a magnet link".into()))?;
    let query = rest
        .strip_prefix('?')
        .ok_or_else(|| TorrentError::Malformed("the link has no query".into()))?;

    let mut info_hash: Option<String> = None;
    let mut name: Option<String> = None;
    let mut trackers: Vec<String> = Vec::new();

    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        match key {
            "xt" => {
                if let Some(hash) = exact_topic(value) {
                    info_hash = Some(hash);
                }
            }
            "dn" => {
                let text = percent_decode(value);
                if !text.is_empty() {
                    name = Some(text);
                }
            }
            "tr" => {
                let text = percent_decode(value);
                if !text.is_empty() {
                    trackers.push(text);
                }
            }
            _ => {}
        }
    }

    let info_hash =
        info_hash.ok_or_else(|| TorrentError::Malformed("the link has no info hash".into()))?;
    Ok(MagnetInfo {
        info_hash,
        name,
        trackers,
    })
}

/// The info hash a `xt` topic carries, when it is one this crate reads.
fn exact_topic(topic: &str) -> Option<String> {
    let hash = topic.strip_prefix("urn:btih:")?;
    if hash.len() == 40 {
        let bytes = hex_decode(hash)?;
        if bytes.len() != 20 {
            return None;
        }
        return Some(hex_encode(&bytes));
    }
    if hash.len() == 32 {
        let bytes = base32_decode(hash)?;
        if bytes.len() != 20 {
            return None;
        }
        return Some(hex_encode(&bytes));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_magnet_is_read() {
        let magnet = parse_magnet(
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=Big+Buck&tr=http%3A%2F%2Ftracker.one%2Fannounce&tr=udp%3A%2F%2Ftracker.two%3A80",
        )
        .unwrap();
        assert_eq!(magnet.info_hash, "0123456789abcdef0123456789abcdef01234567");
        assert_eq!(magnet.name.as_deref(), Some("Big Buck"));
        assert_eq!(
            magnet.trackers,
            vec!["http://tracker.one/announce", "udp://tracker.two:80"]
        );
    }

    #[test]
    fn a_base32_info_hash_is_read() {
        let magnet = parse_magnet("magnet:?xt=urn:btih:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap();
        assert_eq!(magnet.info_hash, "0".repeat(40));
        assert_eq!(magnet.display_name(), "0".repeat(40));
    }

    #[test]
    fn a_link_without_a_hash_is_refused() {
        let error = parse_magnet("magnet:?dn=nothing").unwrap_err();
        assert!(error.to_string().contains("info hash"));
        assert!(parse_magnet("http://example.com/file.torrent").is_err());
    }
}
