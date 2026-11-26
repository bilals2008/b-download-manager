//! The pieces a torrent is cut into, and which files the reader chose.

use serde::Serialize;

use crate::metainfo::Metainfo;

/// The next piece to ask a peer for. In order, only the first missing one: a
/// peer that lacks it yields nothing, so the caller drops the session and
/// tries another peer instead of skipping ahead. Out of order, the first
/// missing piece the peer actually has, which is today's fastest fill.
pub(crate) fn choose_piece(
    missing: &[u64],
    peer_has: impl Fn(u32) -> bool,
    sequential: bool,
) -> Option<u64> {
    let first = *missing.first()?;
    if sequential {
        return peer_has(first as u32).then_some(first);
    }
    missing
        .iter()
        .copied()
        .find(|index| peer_has(*index as u32))
}

/// Where the pieces of a torrent are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PieceMap {
    /// How many bytes a piece holds.
    pub piece_length: u64,
    /// How many pieces there are.
    pub count: u64,
    /// How many bytes the whole torrent holds.
    pub total_length: u64,
    /// How many of the pieces are on disk.
    pub have: u64,
}

impl PieceMap {
    /// The pieces of a torrent of `total_length` bytes, cut into pieces of
    /// `piece_length` bytes.
    pub fn new(piece_length: u64, total_length: u64) -> PieceMap {
        PieceMap::with_have(piece_length, total_length, 0)
    }

    /// The pieces of a torrent, of which `have` are on disk.
    pub fn with_have(piece_length: u64, total_length: u64, have: u64) -> PieceMap {
        let count = if piece_length == 0 {
            0
        } else {
            total_length.div_ceil(piece_length)
        };
        PieceMap {
            piece_length,
            count,
            total_length,
            have,
        }
    }

    /// The piece a byte belongs to.
    pub fn index_at(&self, offset: u64) -> u64 {
        if self.piece_length == 0 {
            return 0;
        }
        offset / self.piece_length
    }

    /// Where a piece starts.
    pub fn offset_of(&self, index: u64) -> u64 {
        index * self.piece_length
    }

    /// How many bytes a piece holds; the last one is shorter.
    pub fn length_of(&self, index: u64) -> u64 {
        let start = self.offset_of(index);
        if start >= self.total_length {
            return 0;
        }
        (self.total_length - start).min(self.piece_length)
    }
}

/// The files the reader chose: every file when nothing was chosen, and the
/// chosen ones otherwise.
pub fn file_selection(metainfo: &Metainfo, wanted: &[usize]) -> Vec<usize> {
    if wanted.is_empty() {
        return (0..metainfo.files.len()).collect();
    }
    let mut chosen: Vec<usize> = Vec::new();
    for index in wanted {
        if *index < metainfo.files.len() && !chosen.contains(index) {
            chosen.push(*index);
        }
    }
    chosen
}

/// How much of every file of the torrent is on disk: the bytes that lie in
/// pieces that are there, over the length of the file, and whether the whole
/// file is there. A piece can lie over two files, so the bytes of a file are
/// counted only when the piece they belong to is whole.
pub fn file_progress(metainfo: &Metainfo, have: &[bool]) -> Vec<(f32, bool)> {
    let map = PieceMap::new(metainfo.piece_length, metainfo.total_length);
    metainfo
        .files
        .iter()
        .map(|file| {
            if file.length == 0 {
                return (1.0, true);
            }
            let first = map.index_at(file.offset);
            let last = map.index_at(file.offset + file.length - 1);
            let file_end = file.offset + file.length;
            let mut present = 0u64;
            let mut whole = true;
            for piece in first..=last {
                let start = map.offset_of(piece);
                let end = start + map.length_of(piece);
                let from = file.offset.max(start);
                let to = file_end.min(end);
                if to <= from {
                    continue;
                }
                if have.get(piece as usize).copied().unwrap_or(false) {
                    present += to - from;
                } else {
                    whole = false;
                }
            }
            (present as f32 / file.length as f32, whole)
        })
        .collect()
}

/// How many bytes the pieces that are there hold.
pub fn present_bytes(piece_map: &PieceMap, have: &[bool]) -> u64 {
    have.iter()
        .enumerate()
        .filter(|(_, has)| **has)
        .map(|(index, _)| piece_map.length_of(index as u64))
        .sum()
}

/// The pieces that are on disk, as a bitmap: one bit per piece, the first
/// piece in the highest bit of the first byte.
///
/// The order is the order the BitTorrent bitfield uses, so the same bytes can
/// go on the wire. A torrent of twenty thousand pieces is a bitmap of two and
/// a half kilobytes, which a torrent of twenty thousand booleans is not.
pub fn piece_bitmap(have: &[bool]) -> Vec<u8> {
    let mut bytes = vec![0u8; have.len().div_ceil(8)];
    for (index, has) in have.iter().enumerate() {
        if *has {
            bytes[index / 8] |= 0x80 >> (index % 8);
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metainfo::TorrentFile;

    fn a_metainfo() -> Metainfo {
        Metainfo {
            info_hash: "0".repeat(40),
            name: "Example".into(),
            // Two bytes a piece, so the second file lies over two of them.
            piece_length: 2,
            pieces: vec!["a".into(), "b".into()],
            files: vec![
                TorrentFile {
                    path: "a".into(),
                    length: 1,
                    offset: 0,
                },
                TorrentFile {
                    path: "b".into(),
                    length: 2,
                    offset: 1,
                },
            ],
            trackers: vec![],
            total_length: 3,
        }
    }

    #[test]
    fn a_byte_belongs_to_a_piece() {
        let map = PieceMap::new(4, 11);
        assert_eq!(map.count, 3);
        assert_eq!(map.index_at(0), 0);
        assert_eq!(map.index_at(3), 0);
        assert_eq!(map.index_at(4), 1);
        assert_eq!(map.index_at(10), 2);
        assert_eq!(map.offset_of(2), 8);
        assert_eq!(map.length_of(0), 4);
        assert_eq!(map.length_of(2), 3);
        assert_eq!(map.length_of(3), 0);
    }

    #[test]
    fn every_file_is_taken_when_nothing_is_chosen() {
        let metainfo = a_metainfo();
        assert_eq!(file_selection(&metainfo, &[]), vec![0, 1]);
        assert_eq!(file_selection(&metainfo, &[1]), vec![1]);
        assert_eq!(file_selection(&metainfo, &[1, 0, 1]), vec![1, 0]);
        assert_eq!(file_selection(&metainfo, &[7]), Vec::<usize>::new());
    }

    #[test]
    fn the_map_says_how_many_pieces_are_there() {
        let map = PieceMap::with_have(4, 12, 2);
        assert_eq!(map.count, 3);
        assert_eq!(map.have, 2);
        assert_eq!(PieceMap::new(4, 12).have, 0);
    }

    #[test]
    fn a_file_that_is_half_there_is_half_way() {
        let metainfo = a_metainfo();
        // The first piece is there, the second and third are not.
        let progress = file_progress(&metainfo, &[true, false, false]);
        // The first file lies inside the first piece: it is whole.
        assert_eq!(progress[0].0, 1.0);
        assert!(progress[0].1);
        // The second file lies over the first and the second piece: one byte
        // of the two is there.
        assert_eq!(progress[1].0, 0.5);
        assert!(!progress[1].1);
    }

    #[test]
    fn the_bytes_of_the_pieces_are_counted() {
        let map = PieceMap::new(4, 11);
        assert_eq!(present_bytes(&map, &[true, false, true]), 7);
        assert_eq!(present_bytes(&map, &[false, false, false]), 0);
    }

    #[test]
    fn the_bitmap_says_which_pieces_are_there() {
        let bits = piece_bitmap(&[true, false, true, false, false, false, false, false, true]);
        assert_eq!(bits, vec![0xa0, 0x80]);
        // The first piece is the highest bit of the first byte, and the ninth
        // piece is the highest bit of the second one.
        assert!(bits[0] & 0x80 != 0);
        assert!(bits[0] & 0x40 == 0);
        assert!(bits[0] & 0x20 != 0);
        assert!(bits[1] & 0x80 != 0);
    }

    #[test]
    fn a_torrent_of_many_pieces_is_a_small_bitmap() {
        let have = vec![true; 21_000];
        let bits = piece_bitmap(&have);
        assert_eq!(bits.len(), 2_625);
        assert!(bits.iter().all(|byte| *byte == 0xff));
        // The empty map of the same torrent is the same size.
        assert_eq!(piece_bitmap(&vec![false; 21_000]).len(), 2_625);
    }
}
