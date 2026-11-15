//! A native WebM/Matroska remuxer.
//!
//! Joins one or two WebM tracks (the VP9/AV1 picture and the Opus sound YouTube
//! and DASH serve) into one WebM or Matroska file without re-encoding. Each
//! block's frames are copied byte for byte; only the track numbers, the cluster
//! layout and the timestamps are rewritten. The output is a `Segment` with a
//! real `Info` and `Tracks` and one ordered sequence of `Cluster`s.
//!
//! A `BlockGroup` is folded into a `SimpleBlock`, carrying its keyframe bit, so
//! the output stays a plain block stream a player can seek.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::mux::{Container, MuxError};

const EBML: u64 = 0x1A45_DFA3;
const SEGMENT: u64 = 0x1853_8067;
const INFO: u64 = 0x1549_A966;
const TRACKS: u64 = 0x1654_AE6B;
const CLUSTER: u64 = 0x1F43_B675;
const TRACK_ENTRY: u64 = 0xAE;
const SIMPLE_BLOCK: u64 = 0xA3;
const BLOCK_GROUP: u64 = 0xA0;
const BLOCK: u64 = 0xA1;
const REFERENCE_BLOCK: u64 = 0xFB;
const CLUSTER_TIMESTAMP: u64 = 0xE7;
const TIMESTAMP_SCALE: u64 = 0x2AD7B1;
const DURATION: u64 = 0x4489;
const DOC_TYPE: u64 = 0x4282;
const DOC_TYPE_VERSION: u64 = 0x4287;
const DOC_TYPE_READ_VERSION: u64 = 0x4285;
const TRACK_NUMBER: u64 = 0xD7;
const TRACK_UID: u64 = 0x73C5;
const TRACK_TYPE: u64 = 0x83;
const CODEC_ID: u64 = 0x86;
const CODEC_PRIVATE: u64 = 0x63A2;
const DEFAULT_DURATION: u64 = 0x23E383;
/// How long the decoder has to be delayed before its first frame lines up with
/// the rest of the file. Dropping it puts Opus audio ahead of the picture.
const CODEC_DELAY: u64 = 0x56AA;
/// How many nanoseconds of audio the encoder expects a seek to discard.
const SEEK_PRE_ROLL: u64 = 0x56BB;
const VIDEO: u64 = 0xE0;
const PIXEL_WIDTH: u64 = 0xB0;
const PIXEL_HEIGHT: u64 = 0xBA;
const AUDIO: u64 = 0xE1;
const SAMPLING_FREQUENCY: u64 = 0xB5;
const CHANNELS: u64 = 0x9F;
/// How much of a block's last frame the decoder must throw away. Only a
/// `BlockGroup` can carry it, so a group is kept whenever it is present.
const DISCARD_PADDING: u64 = 0x75A2;
const CUES: u64 = 0x1C53_BB6B;
const CUE_POINT: u64 = 0xBB;
const CUE_TIME: u64 = 0xB3;
const CUE_TRACK_POSITIONS: u64 = 0xB7;
const CUE_TRACK: u64 = 0xF7;
const CUE_CLUSTER_POSITION: u64 = 0xF1;
const MUXING_APP: u64 = 0x4D80;
const WRITING_APP: u64 = 0x5741;

/// The default scale: one timestamp unit is one millisecond.
const DEFAULT_SCALE: u64 = 1_000_000;
/// How far a block may sit from its cluster's start before a new cluster opens.
const CLUSTER_SPAN_MS: u64 = 1000;
/// A block's relative timestamp must fit a signed 16-bit count of units.
const MAX_RELATIVE: i64 = 32_000;

#[derive(Clone, Default)]
struct RawTrack {
    number: u64,
    kind: u64,
    codec_id: Vec<u8>,
    codec_private: Vec<u8>,
    default_duration: Option<u64>,
    codec_delay: Option<u64>,
    seek_pre_roll: Option<u64>,
    pixel_width: Option<u64>,
    pixel_height: Option<u64>,
    sample_rate: Option<f64>,
    channels: Option<u64>,
}

#[derive(Clone)]
struct Block {
    source_track: u64,
    flags: u8,
    keyframe: bool,
    time_ns: u64,
    data_offset: u64,
    data_len: u64,
    /// The signed nanoseconds the decoder must discard from the end of this
    /// block's last frame, when its `BlockGroup` said so.
    discard_padding: Option<i64>,
}

#[derive(Default)]
struct Input {
    path: PathBuf,
    scale: u64,
    tracks: Vec<RawTrack>,
    blocks: Vec<Block>,
}

#[derive(Clone)]
struct Header {
    id: u64,
    data: u64,
    size: Option<u64>,
    end: u64,
}

struct ElementRange {
    id: u64,
    data: usize,
    end: usize,
}

/// Join the WebM tracks into one WebM or Matroska file.
pub(crate) fn join(
    inputs: &[PathBuf],
    output: &Path,
    work_dir: &Path,
    container: Container,
) -> Result<(), MuxError> {
    if !matches!(container, Container::Mkv | Container::Webm) {
        return Err(MuxError::Io(format!(
            "the {} container is not a Matroska/WebM target",
            container.extension()
        )));
    }
    if inputs.is_empty() || inputs.len() > 2 {
        return Err(MuxError::Io(
            "one or two WebM tracks are expected".to_string(),
        ));
    }
    let mut all = Vec::with_capacity(inputs.len());
    for input in inputs {
        all.push(read_input(input)?);
    }
    if all.iter().all(|input| input.tracks.is_empty()) {
        return Err(MuxError::Io(
            "the tracks have no Matroska header".to_string(),
        ));
    }
    std::fs::create_dir_all(work_dir)
        .map_err(|error| MuxError::Io(format!("the storage folder could not be made: {error}")))?;
    write_joined(&all, output, work_dir, container)
}

mod ebml;
mod read;
mod write;

use read::read_input;
use write::write_joined;

#[cfg(test)]
mod tests;
