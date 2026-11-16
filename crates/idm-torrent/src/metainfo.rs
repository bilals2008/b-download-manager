//! The `.torrent` file: what is in it, where the files are and how the
//! pieces are cut.

use serde::Serialize;

use crate::bencode::{bencode_decode, decode_with_info_span, BencodeValue};
use crate::error::TorrentError;
use crate::util::{hex_encode, sha1_bytes};

/// What a `.torrent` file says.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metainfo {
    /// The info hash, as 40 hex characters.
    pub info_hash: String,
    /// The name of the torrent.
    pub name: String,
    /// How many bytes a piece holds.
    pub piece_length: u64,
    /// The hash of every piece, as hex.
    pub pieces: Vec<String>,
    /// The files, in the order they come in the torrent.
    pub files: Vec<TorrentFile>,
    /// The trackers the torrent points at.
    pub trackers: Vec<String>,
    /// How many bytes the whole torrent holds.
    pub total_length: u64,
}

/// One file of a torrent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentFile {
    /// Where the file goes, relative to the folder the job writes into.
    pub path: String,
    /// How many bytes the file holds.
    pub length: u64,
    /// Where the file starts in the stream of all the bytes.
    pub offset: u64,
}

/// Read a `.torrent` file.
pub fn parse_torrent(bytes: &[u8]) -> Result<Metainfo, TorrentError> {
    let (value, info_span) = decode_with_info_span(bytes)?;
    let info = value
        .get("info")
        .ok_or_else(|| TorrentError::Malformed("it has no info dictionary".into()))?;
    let (start, end) = info_span
        .ok_or_else(|| TorrentError::Malformed("it has no info dictionary".into()))?;
    let info_hash = hex_encode(&sha1_bytes(&bytes[start..end]));

    let mut metainfo = metainfo_from_info(info, info_hash)?;
    metainfo.trackers = trackers_of(&value);
    Ok(metainfo)
}

/// Read the info dictionary of a torrent.
pub(crate) fn metainfo_from_info(
    info: &BencodeValue,
    info_hash: String,
) -> Result<Metainfo, TorrentError> {
    let name = info
        .get("name")
        .and_then(BencodeValue::as_text)
        .ok_or_else(|| TorrentError::Malformed("it has no name".into()))?;
    let piece_length = info
        .get("piece length")
        .and_then(BencodeValue::as_int)
        .ok_or_else(|| TorrentError::Malformed("it has no piece length".into()))?;
    if piece_length <= 0 {
        return Err(TorrentError::Malformed("its piece length is zero".into()));
    }
    let piece_hashes = info
        .get("pieces")
        .and_then(BencodeValue::as_bytes)
        .ok_or_else(|| TorrentError::Malformed("it has no piece hashes".into()))?;
    if piece_hashes.len() % 20 != 0 {
        return Err(TorrentError::Malformed(
            "its piece hashes are not whole hashes".into(),
        ));
    }
    let pieces = piece_hashes
        .chunks(20)
        .map(hex_encode)
        .collect::<Vec<String>>();

    let files = files_of(info, &name)?;
    let total_length: u64 = files.iter().map(|file| file.length).sum();
    let wanted = total_length.div_ceil(piece_length as u64);
    if pieces.len() as u64 != wanted {
        return Err(TorrentError::Malformed(
            "its piece hashes do not match its files".into(),
        ));
    }

    Ok(Metainfo {
        info_hash,
        name,
        piece_length: piece_length as u64,
        pieces,
        files,
        trackers: Vec::new(),
        total_length,
    })
}

/// The files of a torrent: one file, or a list of them under the torrent's
/// own name.
fn files_of(info: &BencodeValue, name: &str) -> Result<Vec<TorrentFile>, TorrentError> {
    if let Some(length) = info.get("length").and_then(BencodeValue::as_int) {
        if length < 0 {
            return Err(TorrentError::Malformed("a file has a negative length".into()));
        }
        return Ok(vec![TorrentFile {
            path: name.to_owned(),
            length: length as u64,
            offset: 0,
        }]);
    }

    let list = info
        .get("files")
        .and_then(BencodeValue::as_list)
        .ok_or_else(|| TorrentError::Malformed("it has no files".into()))?;
    let mut files = Vec::with_capacity(list.len());
    let mut offset = 0u64;
    for entry in list {
        let length = entry
            .get("length")
            .and_then(BencodeValue::as_int)
            .ok_or_else(|| TorrentError::Malformed("a file has no length".into()))?;
        if length < 0 {
            return Err(TorrentError::Malformed("a file has a negative length".into()));
        }
        let parts = entry
            .get("path")
            .and_then(BencodeValue::as_list)
            .ok_or_else(|| TorrentError::Malformed("a file has no path".into()))?;
        let mut path = name.to_owned();
        for part in parts {
            let text = part
                .as_text()
                .ok_or_else(|| TorrentError::Malformed("a file has a strange path".into()))?;
            path.push('/');
            path.push_str(&text);
        }
        files.push(TorrentFile {
            path,
            length: length as u64,
            offset,
        });
        offset += length as u64;
    }
    Ok(files)
}

/// The trackers a torrent points at: the one address and the list of lists.
fn trackers_of(value: &BencodeValue) -> Vec<String> {
    let mut trackers: Vec<String> = Vec::new();
    if let Some(announce) = value.get("announce").and_then(BencodeValue::as_text) {
        if !announce.is_empty() {
            trackers.push(announce);
        }
    }
    if let Some(list) = value.get("announce-list").and_then(BencodeValue::as_list) {
        for tier in list {
            match tier {
                BencodeValue::List(tier) => {
                    for tracker in tier {
                        if let Some(text) = tracker.as_text() {
                            if !text.is_empty() && !trackers.contains(&text) {
                                trackers.push(text);
                            }
                        }
                    }
                }
                other => {
                    if let Some(text) = other.as_text() {
                        if !text.is_empty() && !trackers.contains(&text) {
                            trackers.push(text);
                        }
                    }
                }
            }
        }
    }
    trackers
}

/// Read the info dictionary a peer sent, with the trackers of the magnet it
/// came from. A magnet's info hash is trusted over whatever the peer says
/// the metadata is; metadata that hashes to anything else is the metadata of
/// another torrent, and is refused.
pub(crate) fn metainfo_from_metadata(
    metadata: &[u8],
    trackers: Vec<String>,
    expected_info_hash: Option<&str>,
) -> Result<Metainfo, TorrentError> {
    let info = bencode_decode(metadata)?;
    let info_hash = hex_encode(&sha1_bytes(metadata));
    if let Some(expected) = expected_info_hash {
        if !info_hash.eq_ignore_ascii_case(expected) {
            return Err(TorrentError::Malformed(
                "the peer sent the metadata of another torrent".into(),
            ));
        }
    }
    let mut metainfo = metainfo_from_info(&info, info_hash)?;
    if metainfo.trackers.is_empty() {
        metainfo.trackers = trackers;
    }
    Ok(metainfo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bencode::bencode_encode;

    /// A torrent of two files, written the way a torrent writer writes it.
    fn a_torrent() -> Vec<u8> {
        let info = BencodeValue::Dict(vec![
            (
                "files".into(),
                BencodeValue::List(vec![
                    BencodeValue::Dict(vec![
                        ("length".into(), BencodeValue::Integer(5)),
                        (
                            "path".into(),
                            BencodeValue::List(vec![BencodeValue::Bytes(b"one.txt".to_vec())]),
                        ),
                    ]),
                    BencodeValue::Dict(vec![
                        ("length".into(), BencodeValue::Integer(6)),
                        (
                            "path".into(),
                            BencodeValue::List(vec![BencodeValue::Bytes(b"two.txt".to_vec())]),
                        ),
                    ]),
                ]),
            ),
            ("name".into(), BencodeValue::Bytes(b"Example".to_vec())),
            ("piece length".into(), BencodeValue::Integer(4)),
            (
                "pieces".into(),
                BencodeValue::Bytes(vec![1u8; 20 * 3]),
            ),
        ]);
        bencode_encode(&BencodeValue::Dict(vec![
            (
                "announce".into(),
                BencodeValue::Bytes(b"http://tracker.example/announce".to_vec()),
            ),
            ("info".into(), info),
        ]))
    }

    #[test]
    fn a_torrent_is_read() {
        let metainfo = parse_torrent(&a_torrent()).unwrap();
        assert_eq!(metainfo.name, "Example");
        assert_eq!(metainfo.piece_length, 4);
        assert_eq!(metainfo.pieces.len(), 3);
        assert_eq!(metainfo.total_length, 11);
        assert_eq!(
            metainfo.trackers,
            vec!["http://tracker.example/announce"]
        );
        assert_eq!(metainfo.files.len(), 2);
        assert_eq!(metainfo.files[0].path, "Example/one.txt");
        assert_eq!(metainfo.files[0].length, 5);
        assert_eq!(metainfo.files[0].offset, 0);
        assert_eq!(metainfo.files[1].path, "Example/two.txt");
        assert_eq!(metainfo.files[1].offset, 5);
        assert_eq!(metainfo.info_hash.len(), 40);
    }

    #[test]
    fn the_info_hash_is_the_hash_of_the_info_dictionary() {
        let bytes = a_torrent();
        let metainfo = parse_torrent(&bytes).unwrap();
        let (value, span) = decode_with_info_span(&bytes).unwrap();
        let info = value.get("info").unwrap();
        let (start, end) = span.unwrap();
        assert_eq!(metainfo.info_hash, hex_encode(&sha1_bytes(&bytes[start..end])));
        // The info dictionary stands on its own as well.
        let alone = metainfo_from_info(info, metainfo.info_hash.clone()).unwrap();
        assert_eq!(alone.total_length, metainfo.total_length);
    }

    #[test]
    fn the_metadata_of_another_torrent_is_refused() {
        // The torrent is of one file; the hash a magnet carries belongs to a
        // different one.
        let info = BencodeValue::Dict(vec![
            ("length".into(), BencodeValue::Integer(4)),
            ("name".into(), BencodeValue::Bytes(b"other.bin".to_vec())),
            ("piece length".into(), BencodeValue::Integer(16)),
            ("pieces".into(), BencodeValue::Bytes(vec![0u8; 20])),
        ]);
        let metadata = bencode_encode(&info);
        let found = metainfo_from_metadata(&metadata, Vec::new(), Some(&"0".repeat(40)));
        assert!(found.is_err());
        let accepted = metainfo_from_metadata(&metadata, Vec::new(), None);
        assert!(accepted.is_ok());
    }

    #[test]
    fn a_broken_torrent_is_refused() {
        assert!(parse_torrent(b"not a torrent").is_err());
        assert!(parse_torrent(b"d4:infod4:name5:shorte").is_err());
    }
}
