//! The pieces a torrent is cut into, and which files the reader chose.

use serde::Serialize;

use crate::metainfo::Metainfo;

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
}

impl PieceMap {
    /// The pieces of a torrent of `total_length` bytes, cut into pieces of
    /// `piece_length` bytes.
    pub fn new(piece_length: u64, total_length: u64) -> PieceMap {
        let count = if piece_length == 0 {
            0
        } else {
            total_length.div_ceil(piece_length)
        };
        PieceMap {
            piece_length,
            count,
            total_length,
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let metainfo = Metainfo {
            info_hash: "0".repeat(40),
            name: "Example".into(),
            piece_length: 4,
            pieces: vec![],
            files: vec![
                crate::metainfo::TorrentFile {
                    path: "a".into(),
                    length: 1,
                    offset: 0,
                },
                crate::metainfo::TorrentFile {
                    path: "b".into(),
                    length: 2,
                    offset: 1,
                },
            ],
            trackers: vec![],
            total_length: 3,
        };
        assert_eq!(file_selection(&metainfo, &[]), vec![0, 1]);
        assert_eq!(file_selection(&metainfo, &[1]), vec![1]);
        assert_eq!(file_selection(&metainfo, &[1, 0, 1]), vec![1, 0]);
        assert_eq!(file_selection(&metainfo, &[7]), Vec::<usize>::new());
    }
}
