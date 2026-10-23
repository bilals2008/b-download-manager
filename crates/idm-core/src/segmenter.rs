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

/// What one connection is doing right now.
///
/// The window lists a row per connection, so every range carries the state
/// of the connection that owns it. The state lives beside the bytes in the
/// manifest, but it is never trusted from there: a manifest describes a
/// download that is not running, and its connections are reset to `Pending`
/// when they are read back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SegmentStatus {
    /// In the plan, no connection is working on it yet.
    #[default]
    Pending,
    /// A connection is being opened.
    Connecting,
    /// Bytes are arriving.
    Receiving,
    /// The connection failed and will be tried again.
    Retrying,
    /// Held back by the pause control.
    Paused,
    /// The user stopped the download. The bytes of the range stay on disk, so
    /// a resume can pick them up again.
    Stopped,
    /// The whole range is on disk.
    Done,
    /// The range will not be fetched again without a new run.
    Failed,
}

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
    /// What the connection of this range is doing. An older manifest has no
    /// status, and its ranges are waiting.
    #[serde(default)]
    pub status: SegmentStatus,
}

/// The end offset of a range whose length the server did not tell us.
///
/// The server answered `Content-Range: bytes 0-0/*`, so the file ends when
/// the response does.
pub const UNKNOWN_END: u64 = u64::MAX;

impl Segment {
    /// Number of bytes this segment is responsible for.
    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    /// Whether this range holds no bytes at all.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn done(&self) -> bool {
        self.downloaded >= self.len()
    }

    /// Whether the length of this range is known.
    pub fn is_unknown(&self) -> bool {
        self.end == UNKNOWN_END
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

/// How many bytes one connection should get.
///
/// IDM divides the bytes that are still to download by `0x2800` (10,240) to
/// find the number of connections a file deserves: `FUN_005bad00` computes
/// `__alldiv(bytes_left, 0x2800)` and takes the smaller of that and the
/// number of connections that are allowed. A small file therefore gets fewer
/// connections than a large one.
pub const BYTES_PER_CONNECTION: u64 = 0x2800;

/// The number of connections a file of this size deserves.
///
/// At least one, at most the number that is allowed.
pub fn connections_for(total: u64, max: u32) -> u32 {
    let wanted = total / BYTES_PER_CONNECTION;
    wanted.clamp(1, max.max(1) as u64) as u32
}

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
            status: SegmentStatus::Pending,
        });
        start = end;
    }

    out
}

/// One range that reaches to the end of a file whose size is unknown.
///
/// The server answered `Content-Range: bytes 0-0/*`, so the file is fetched
/// by a single connection, from the first byte to the end of the response.
pub fn plan_unknown(dir: &Path, stem: &str) -> Vec<Segment> {
    vec![Segment {
        id: 0,
        start: 0,
        end: UNKNOWN_END,
        downloaded: 0,
        part: part_path(dir, stem, 0),
        status: SegmentStatus::Pending,
    }]
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
        status: SegmentStatus::Pending,
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
    fn a_small_file_gets_fewer_connections() {
        // One connection per 10 KB that is left, as IDM counts them.
        assert_eq!(connections_for(1024 * 1024, 8), 8);
        assert_eq!(connections_for(100 * 1024, 8), 8);
        assert_eq!(connections_for(20 * 1024, 8), 2);
        assert_eq!(connections_for(1024, 8), 1);
        assert_eq!(connections_for(0, 8), 1);
    }

    #[test]
    fn the_number_of_connections_never_passes_the_limit() {
        assert_eq!(connections_for(1024 * 1024 * 1024, 4), 4);
        assert_eq!(connections_for(1024 * 1024 * 1024, 0), 1);
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
    fn a_range_from_an_older_manifest_is_waiting() {
        let json = r#"{"id":0,"start":0,"end":10,"downloaded":0,"part":"a.part0"}"#;
        let segment: Segment = serde_json::from_str(json).expect("a range without a status");
        assert_eq!(segment.status, SegmentStatus::Pending);
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
