//! Segmentation — the Rust analogue of IDM's `ChunksList` / `ChunksListItem`.
//!
//! Reverse engineering `IDMan.exe` (v6.43.15.2) showed that IDM does **not**
//! download a file into one stream and then split it. It keeps a *list of
//! chunks*; every chunk owns a contiguous byte range of the target file and
//! writes the bytes of that range into its own temporary part file. The list
//! is persisted so an interrupted download can be resumed — in IDM that is the
//! registry key `HKCU\Software\DownloadManager\<id>\ChList`, here it is a JSON
//! manifest written next to the part files.
//!
//! Evidence: `ChunksList::CreateNewItem()`, `ChunksList::InitFirst()`,
//! `ChunksList::JoinFile()`, `ChunksList::FindMaxFreeChunk()` and
//! `ChunksList::NeedNewConnection()` (assert strings in `IDMan.exe`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One contiguous byte range of the target file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub id: u32,
    /// Absolute offset of the first byte of this range.
    pub start: u64,
    /// Absolute offset one past the last byte of this range.
    pub end: u64,
    /// How many bytes of this range are already stored in `part`.
    pub downloaded: u64,
    /// Temporary file holding the bytes of this range.
    pub part: PathBuf,
}

impl Segment {
    /// Number of bytes this segment is responsible for.
    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    pub fn done(&self) -> bool {
        self.downloaded >= self.len()
    }

    /// Offset of the next byte that still has to be requested.
    pub fn next_offset(&self) -> u64 {
        self.start + self.downloaded
    }
}

/// Target size of one segment. IDM splits a file into a handful of large
/// ranges rather than into many small ones, so the same rule is used here:
/// the segment count grows with the file size but stays bounded by the
/// connection limit.
pub const SEGMENT_TARGET: u64 = 16 * 1024 * 1024;

/// Smallest remainder worth handing to another connection.
pub const MIN_SPLIT: u64 = 1024 * 1024;

/// Split `total` bytes into at most `max_segments` contiguous ranges.
pub fn plan(total: u64, max_segments: u32, dir: &Path, stem: &str) -> Vec<Segment> {
    let max_segments = max_segments.max(1) as u64;
    let count = if total == 0 {
        1
    } else {
        total.div_ceil(SEGMENT_TARGET)
    }
    .clamp(1, max_segments);

    let base = total / count;
    let mut remainder = total % count;
    let mut start = 0u64;
    let mut out = Vec::with_capacity(count as usize);

    for i in 0..count {
        let mut size = base;
        if remainder > 0 {
            size += 1;
            remainder -= 1;
        }
        let end = start + size;
        out.push(Segment {
            id: i as u32,
            start,
            end,
            downloaded: 0,
            part: part_path(dir, stem, i as u32),
        });
        start = end;
    }

    out
}

/// Split a segment in two and hand the tail to a new connection.
///
/// This mirrors IDM's `ChunksList::AddAlternative()` / "Add alternative
/// connection (%ld) for connection %ld": the running connection keeps the
/// bytes it already wrote, and a fresh connection continues from the point it
/// reached. The original range end is preserved on the new segment.
pub fn split(seg: &mut Segment, new_id: u32, dir: &Path, stem: &str) -> Option<Segment> {
    let old_end = seg.end;
    let split_at = if seg.downloaded > 0 {
        seg.next_offset()
    } else {
        seg.start + seg.len() / 2
    };

    if split_at <= seg.start || split_at >= old_end {
        return None;
    }

    seg.end = split_at;
    seg.downloaded = seg.downloaded.min(split_at - seg.start);

    Some(Segment {
        id: new_id,
        start: split_at,
        end: old_end,
        downloaded: 0,
        part: part_path(dir, stem, new_id),
    })
}

/// Path of the temporary part file that holds one segment's bytes.
pub fn part_path(dir: &Path, stem: &str, id: u32) -> PathBuf {
    dir.join(format!("{stem}.part{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_covers_the_whole_file_without_gaps() {
        let dir = Path::new("/tmp");
        let total = 4 * SEGMENT_TARGET + 12345;
        let segments = plan(total, 4, dir, "file");

        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0].start, 0);
        assert_eq!(segments.last().unwrap().end, total);
        for pair in segments.windows(2) {
            assert_eq!(pair[0].end, pair[1].start, "segments must be contiguous");
        }
        let sum: u64 = segments.iter().map(|s| s.len()).sum();
        assert_eq!(sum, total);
    }

    #[test]
    fn a_small_file_is_not_split() {
        let segments = plan(1000, 4, Path::new("/tmp"), "file");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].len(), 1000);
    }

    #[test]
    fn plan_never_exceeds_the_connection_limit() {
        let segments = plan(64 * 1024 * 1024, 8, Path::new("/tmp"), "file");
        assert!(segments.len() <= 8);
    }

    #[test]
    fn split_hands_the_tail_to_the_new_segment() {
        let dir = Path::new("/tmp");
        let mut segments = plan(1000, 2, dir, "file");
        let original_end = segments[0].end;

        let new_segment = split(&mut segments[0], 99, dir, "file").expect("split");

        assert_eq!(segments[0].end, new_segment.start);
        assert_eq!(new_segment.end, original_end);
        assert_eq!(new_segment.downloaded, 0);
        assert_ne!(new_segment.part, segments[0].part);
    }

    #[test]
    fn split_keeps_the_bytes_already_downloaded() {
        let dir = Path::new("/tmp");
        let mut segments = plan(1000, 2, dir, "file");
        segments[0].downloaded = 100;

        let new_segment = split(&mut segments[0], 99, dir, "file").expect("split");

        assert_eq!(new_segment.start, 100);
        assert_eq!(segments[0].downloaded, 100);
        assert!(segments[0].done());
    }
}
