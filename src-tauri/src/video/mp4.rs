//! A native MP4 remuxer.
//!
//! Joins one or two MP4-family tracks into a single progressive MP4 without
//! re-encoding a single sample. Both the plain MP4 a progressive download
//! yields and the fragmented MP4 (`moov` + `moof`/`mdat`) that YouTube and DASH
//! serve are read; the output is an `ftyp` + `moov` + `mdat` file with its
//! index first, so it seeks and streams like any downloaded file.
//!
//! The bytes of every sample are copied unchanged, and the codec configuration
//! (the `stsd` sample entry: `avcC`, `esds`, ...) is copied verbatim, so the
//! output is exactly what the player served, only re-arranged. Nothing here
//! knows what a codec is; it knows the MP4 box layout.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::mux::{Container, MuxError};

mod boxes;
mod fragment;
mod read;
mod write;

use read::read_track;
use write::write_joined;

/// One decoded sample: where its bytes are, how long they are, how long the
/// sample plays, and whether it is a sync (key) sample.
#[derive(Debug, Clone)]
struct Sample {
    offset: u64,
    size: u32,
    duration: u32,
    /// The sample's decode time inside its own track, in the track's timescale.
    /// A fragment series that begins part-way through a stream starts above
    /// zero, and that offset is what an edit list has to preserve.
    decode_time: u64,
    /// Presentation time minus decode time. A stream with B-frames reorders
    /// pictures, so the two are not the same number; dropping this field would
    /// put every frame back in decode order and break lip sync.
    composition_offset: i32,
    sync: bool,
}

impl Sample {
    fn presentation_time(&self) -> u64 {
        (self.decode_time as i64 + self.composition_offset as i64).max(0) as u64
    }
}

/// One track, reduced to what the writer needs to rebuild it.
struct Track {
    /// Where the sample bytes are read from.
    source: PathBuf,
    /// `vide` or `soun`.
    handler: [u8; 4],
    /// The track's own timescale, from `mdhd`.
    timescale: u32,
    /// The `tkhd` box verbatim; its duration is patched before writing.
    tkhd: Vec<u8>,
    /// The `mdhd` box verbatim; its duration is patched before writing.
    mdhd: Vec<u8>,
    /// The `stsd` box verbatim.
    stsd: Vec<u8>,
    samples: Vec<Sample>,
    /// Where the source's timeline begins, in the track's timescale.
    start_time: u64,
}

impl Track {
    fn duration_units(&self) -> u64 {
        self.samples
            .iter()
            .map(|sample| sample.duration as u64)
            .sum()
    }

    /// Whether any picture is presented out of decode order.
    fn reorders(&self) -> bool {
        self.samples
            .iter()
            .any(|sample| sample.composition_offset != 0)
    }

    /// The first presentation time, which is where an edit list has to start.
    fn first_presentation(&self) -> u64 {
        self.samples
            .iter()
            .map(Sample::presentation_time)
            .min()
            .unwrap_or(0)
    }

    /// How far the media has to move so its own timeline is kept.
    fn media_shift(&self) -> u64 {
        self.start_time.max(self.first_presentation())
    }

    /// The length of the presentation, including the shift an edit list adds.
    fn presentation_units(&self) -> u64 {
        self.media_shift().saturating_add(self.duration_units())
    }
}

/// The movie timescale the output `mvhd` uses.
const MOVIE_TIMESCALE: u32 = 1000;

struct Fragment {
    moof: Vec<u8>,
    base: u64,
}

struct RawTrack<'a> {
    id: u32,
    handler: [u8; 4],
    timescale: u32,
    tkhd: &'a [u8],
    mdhd: &'a [u8],
    stsd: &'a [u8],
}

struct BoxHeader {
    start: u64,
    size: u64,
    tag: [u8; 4],
}

struct Atom {
    tag: [u8; 4],
    start: usize,
    end: usize,
}

/// Join the tracks into one progressive MP4 at `output`.
pub(crate) fn join(
    inputs: &[PathBuf],
    output: &Path,
    work_dir: &Path,
    container: Container,
) -> Result<(), MuxError> {
    if !matches!(container, Container::Mp4 | Container::M4a) {
        return Err(MuxError::Io(format!(
            "the {} container cannot be written yet; only MP4 and M4A are",
            container.extension()
        )));
    }
    if inputs.is_empty() || inputs.len() > 2 {
        return Err(MuxError::Io(
            "one or two MP4 tracks are expected".to_string(),
        ));
    }

    let mut tracks = Vec::with_capacity(inputs.len());
    for input in inputs {
        tracks.push(read_track(input)?);
    }
    std::fs::create_dir_all(work_dir)
        .map_err(|error| MuxError::Io(format!("the storage folder could not be made: {error}")))?;
    write_joined(&tracks, output, work_dir, container)
}

#[cfg(test)]
mod tests;
