//! Reading what a finished file actually is.
//!
//! Nothing here trusts a file name or a manifest. The bytes are
//! read back and the container is parsed: an MP4's `moov` index gives its
//! tracks, codecs and duration; a Matroska/WebM header gives the same; a byte
//! stream that is neither is not a deliverable.
//!
//! This is what keeps the download list honest: a media download reports
//! **Saved** only after [`check_finished`] returned, so a failed join, a
//! truncated stream or an HTML error page saved under an `.mp4` name is
//! reported as a failure instead.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use idm_core::log;

/// How much of a file is read when looking for its index. A `moov` or a
/// Matroska `Tracks` element is a few hundred kilobytes even for a long film;
/// this is a ceiling, never a target.
const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;

/// The container the file itself declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaContainer {
    Mp4,
    Matroska,
    Webm,
    MpegTs,
    /// A bare AAC byte stream (ADTS), which HLS audio renditions often are.
    Adts,
    Mp3,
    Flac,
    /// An Ogg page stream, which is what an `ogg`/`opus` file is.
    Ogg,
    Unknown,
}

impl MediaContainer {
    pub fn name(self) -> &'static str {
        match self {
            MediaContainer::Mp4 => "mp4",
            MediaContainer::Matroska => "matroska",
            MediaContainer::Webm => "webm",
            MediaContainer::MpegTs => "mpeg-ts",
            MediaContainer::Adts => "aac",
            MediaContainer::Mp3 => "mp3",
            MediaContainer::Flac => "flac",
            MediaContainer::Ogg => "ogg",
            MediaContainer::Unknown => "unknown",
        }
    }

    /// Whether the container has an index that states its streams. A byte
    /// stream (ADTS, MP3, FLAC, Ogg) does not; its signature is all there is to
    /// check, plus the playback test.
    pub fn lists_streams(self) -> bool {
        matches!(
            self,
            MediaContainer::Mp4 | MediaContainer::Matroska | MediaContainer::Webm
        )
    }

    /// Whether a file with this name could hold these bytes.
    pub fn fits_extension(self, extension: &str) -> bool {
        match extension.to_ascii_lowercase().as_str() {
            "mp4" | "m4v" | "mov" | "m4a" => self == MediaContainer::Mp4,
            "mkv" => matches!(self, MediaContainer::Matroska | MediaContainer::Webm),
            "webm" => matches!(self, MediaContainer::Webm | MediaContainer::Matroska),
            "ts" => self == MediaContainer::MpegTs,
            "aac" => self == MediaContainer::Adts,
            "mp3" => self == MediaContainer::Mp3,
            "flac" => self == MediaContainer::Flac,
            "ogg" | "opus" => self == MediaContainer::Ogg,
            _ => false,
        }
    }
}

/// What the bytes hold.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    pub container: MediaContainer,
    /// Seconds, when the index states a duration. `None` means the file does
    /// not say; it is never a guess.
    pub duration: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub bytes: u64,
    /// How many samples the index accounts for. Zero for a byte stream, which
    /// has no index to account for anything.
    pub sample_count: u64,
    /// What the index was found to say about the bytes themselves.
    pub playback: Playback,
}

/// What was proved about the bytes, beyond what the index claims about itself.
///
/// An index can be read and still be wrong: a download cut short leaves a
/// `moov` that names samples the file does not hold, and a join can patch its
/// tables onto the wrong kind of stream. Those are contradictions, not
/// opinions, and a file that has one is not a file anybody should be given.
/// What cannot be checked is said to be unchecked rather than called a success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Playback {
    /// Every sample the index names lies inside the bytes, the sample and
    /// timing tables agree, and each track states how to decode it.
    Verified,
    /// Nothing contradicted the file, and nothing proved it either. The reason
    /// is kept so the log can say what was not checked.
    Unverified(&'static str),
    /// The index contradicts the file. Publishing it would hand the user a
    /// file that plays badly or not at all.
    Broken(String),
}

impl MediaInfo {
    fn empty(container: MediaContainer, bytes: u64) -> Self {
        Self {
            container,
            duration: None,
            video_codec: None,
            audio_codec: None,
            bytes,
            sample_count: 0,
            // A byte stream has no index, so there is nothing to contradict
            // and nothing to prove.
            playback: Playback::Unverified("a byte stream has no index to read"),
        }
    }
}

/// What the caller expects the finished file to be.
#[derive(Debug, Clone, Copy)]
pub struct Expect<'a> {
    /// The extension the file will be given.
    pub extension: &'a str,
    pub video: bool,
    /// Sound was named as one of the tracks that were fetched, so its absence
    /// is a broken download rather than a silent source.
    pub audio: bool,
    /// The caller asked for an audio track on top of the picture. A source that
    /// carries no sound at all — a silent stock clip, a video-only HLS
    /// rendition — is still the file that was asked for, so it is not refused.
    pub audio_selected: bool,
}

/// Read a file's container, tracks and duration from its own bytes.
pub fn inspect(path: &Path) -> Result<MediaInfo, String> {
    let mut file =
        File::open(path).map_err(|error| format!("the file could not be read: {error}"))?;
    let bytes = file
        .metadata()
        .map(|meta| meta.len())
        .map_err(|error| format!("the file could not be read: {error}"))?;
    if bytes == 0 {
        return Err("the file is empty".to_string());
    }

    let mut head = [0u8; 512];
    let read = file
        .read(&mut head)
        .map_err(|error| format!("the file could not be read: {error}"))?;
    let head = &head[..read];

    if looks_like_mp4(head) {
        return parse_mp4(&mut file, bytes);
    }
    if looks_like_matroska(head) {
        return parse_matroska(&mut file, bytes);
    }
    if looks_like_ts(head, bytes) {
        return Ok(MediaInfo::empty(MediaContainer::MpegTs, bytes));
    }
    if looks_like_adts(head) {
        return Ok(MediaInfo::empty(MediaContainer::Adts, bytes));
    }
    if looks_like_mp3(head) {
        return Ok(MediaInfo::empty(MediaContainer::Mp3, bytes));
    }
    if looks_like_flac(head) {
        return Ok(MediaInfo::empty(MediaContainer::Flac, bytes));
    }
    if looks_like_ogg(head) {
        return Ok(MediaInfo::empty(MediaContainer::Ogg, bytes));
    }
    Ok(MediaInfo::empty(MediaContainer::Unknown, bytes))
}

/// Whether the finished file is what the caller promised, and plays.
///
/// This is the only gate between a download and **Saved**. It fails with one
/// sentence a person can act on, and it never guesses: a file whose bytes are
/// not a media container is refused even when its name claims it is one.
pub fn check_finished(path: &Path, expect: Expect<'_>) -> Result<MediaInfo, String> {
    let info = inspect(path)?;
    if !info.container.fits_extension(expect.extension) {
        log::write(
            "warn",
            &format!(
                "{} is {} but was named .{}",
                path.display(),
                info.container.name(),
                expect.extension
            ),
        );
        return Err(format!(
            "the finished file is not {} (its bytes are {})",
            extension_word(expect.extension),
            info.container.name()
        ));
    }
    if info.container.lists_streams() {
        if expect.video && info.video_codec.is_none() {
            return Err("the finished file has no picture".to_string());
        }
        if expect.audio && info.audio_codec.is_none() && expect.audio_selected {
            return Err("the finished file has no sound".to_string());
        }
        if expect.audio && info.audio_codec.is_none() {
            // A silent source. Stock footage sites carry clips with no audio
            // track at all, and the picture the user asked for is complete, so
            // the file is kept and only the log says what it is.
            log::write(
                "info",
                &format!(
                    "{} has no sound track; the source is silent",
                    path.display()
                ),
            );
        }
    }
    if info.duration.is_some_and(|seconds| seconds <= 0.05) {
        return Err("the finished file is no time long".to_string());
    }

    // The deep check runs last: the cheap, specific refusals above say more to
    // a person than "a sample is missing", and they are the ones that name a
    // broken track rather than a broken file.
    match &info.playback {
        Playback::Broken(reason) => return Err(reason.clone()),
        Playback::Verified => log::write(
            "info",
            &format!(
                "{} was checked against its own index: {} bytes, {}s, {} sample(s)",
                path.display(),
                info.bytes,
                info.duration.unwrap_or(0.0),
                info.sample_count
            ),
        ),
        Playback::Unverified(reason) => log::write(
            "info",
            &format!(
                "{} was read but not proved: {reason}",
                path.display()
            ),
        ),
    }

    Ok(info)
}

fn extension_word(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "mkv" | "webm" => "a video in its container",
        "m4a" | "mp3" | "flac" | "ogg" | "opus" | "aac" => "a playable audio file",
        _ => "a playable video file",
    }
}

/* ---------------------------------------------------------------- signatures */

fn looks_like_mp4(head: &[u8]) -> bool {
    head.len() >= 8 && matches!(&head[4..8], b"ftyp" | b"moov" | b"mdat" | b"free" | b"wide")
}

fn looks_like_matroska(head: &[u8]) -> bool {
    head.len() >= 4 && head[0..4] == [0x1A, 0x45, 0xDF, 0xA3]
}

fn looks_like_ts(head: &[u8], bytes: u64) -> bool {
    bytes >= 3 * 188
        && head.len() >= 3 * 188
        && head[0] == 0x47
        && head[188] == 0x47
        && head[376] == 0x47
}

fn looks_like_adts(head: &[u8]) -> bool {
    head.len() >= 7 && head[0] == 0xFF && (head[1] & 0xF6) == 0xF0
}

fn looks_like_mp3(head: &[u8]) -> bool {
    head.len() >= 4
        && (head.starts_with(b"ID3")
            || (head[0] == 0xFF && (head[1] & 0xE0) == 0xE0 && (head[1] & 0x06) != 0))
}

fn looks_like_flac(head: &[u8]) -> bool {
    head.starts_with(b"fLaC")
}

fn looks_like_ogg(head: &[u8]) -> bool {
    head.starts_with(b"OggS")
}

/* ---------------------------------------------------------------------- mp4 */

/// Read the top-level boxes until `moov`, then its track list.
fn parse_mp4(file: &mut File, bytes: u64) -> Result<MediaInfo, String> {
    let mut at = 0u64;
    while at + 8 <= bytes {
        file.seek(SeekFrom::Start(at))
            .map_err(|error| format!("the file could not be read: {error}"))?;
        let mut header = [0u8; 16];
        let read = file
            .read(&mut header)
            .map_err(|error| format!("the file could not be read: {error}"))?;
        if read < 8 {
            break;
        }
        let mut size = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let tag = [header[4], header[5], header[6], header[7]];
        let mut header_len = 8u64;
        if size == 1 {
            if read < 16 {
                break;
            }
            size = u64::from_be_bytes([
                header[8], header[9], header[10], header[11], header[12], header[13], header[14],
                header[15],
            ]);
            header_len = 16;
        } else if size == 0 {
            size = bytes - at;
        }
        if size < header_len || at + size > bytes {
            break;
        }

        if &tag == b"moov" {
            let body = size - header_len;
            if body > MAX_INDEX_BYTES {
                return Err("the file's index is too large to check".to_string());
            }
            let mut index = vec![0u8; body as usize];
            // The 16-byte header read above may have taken bytes belonging to
            // the box itself; the body starts after the real header.
            file.seek(SeekFrom::Start(at + header_len))
                .map_err(|error| format!("the file could not be read: {error}"))?;
            file.read_exact(&mut index)
                .map_err(|error| format!("the file could not be read: {error}"))?;
            let mut info = MediaInfo::empty(MediaContainer::Mp4, bytes);
            info.playback = read_moov(&index, bytes, &mut info);
            return Ok(info);
        }
        at += size;
    }
    Err("the file has no MP4 index".to_string())
}

/// One box: its four-character type and the byte range of its content.
struct Atom {
    tag: [u8; 4],
    start: usize,
    end: usize,
}

fn atoms(bytes: &[u8]) -> Vec<Atom> {
    let mut atoms = Vec::new();
    let mut at = 0usize;
    while at + 8 <= bytes.len() {
        let size32 =
            u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as u64;
        let mut header = 8usize;
        let mut size = size32;
        if size32 == 1 {
            if at + 16 > bytes.len() {
                break;
            }
            size = u64::from_be_bytes([
                bytes[at + 8],
                bytes[at + 9],
                bytes[at + 10],
                bytes[at + 11],
                bytes[at + 12],
                bytes[at + 13],
                bytes[at + 14],
                bytes[at + 15],
            ]);
            header = 16;
        } else if size32 == 0 {
            size = (bytes.len() - at) as u64;
        }
        if size < header as u64 || at as u64 + size > bytes.len() as u64 {
            break;
        }
        atoms.push(Atom {
            tag: [bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]],
            start: at + header,
            end: at + size as usize,
        });
        at += size as usize;
    }
    atoms
}

fn find(bytes: &[u8], wanted: &[&[u8; 4]]) -> Option<Atom> {
    atoms(bytes).into_iter().find(|atom| {
        wanted
            .iter()
            .any(|tag| atom.tag.as_slice() == tag.as_slice())
    })
}

/// A track's index, as much of it as the file states.
struct TrackIndex {
    handler: [u8; 4],
    /// Whether the sample entry carries the box that configures the decoder.
    configured: bool,
    timescale: u32,
    /// The track's own duration, in its own timescale.
    duration: u64,
    table: Option<SampleTable>,
}

/// The sample tables of one track.
///
/// They are read together because they only mean anything together: sizes
/// without the chunk offsets say nothing about where the bytes are, and timing
/// without the sizes cannot be checked against them.
struct SampleTable {
    count: u64,
    /// A non-zero size every sample shares; zero means `sizes` is the table.
    uniform: u32,
    sizes: Vec<u32>,
    chunk_offsets: Vec<u64>,
    /// `(first chunk, samples per chunk)`, one entry per run, chunks 1-based.
    runs: Vec<(u32, u32)>,
    /// Sync sample numbers, 1-based. `None` means the file states none, which
    /// in an MP4 means every sample can be started from.
    sync: Option<Vec<u32>>,
    /// `(samples, decode units)` the timing table names.
    timeline: Option<(u64, u64)>,
}

impl SampleTable {
    fn size_of(&self, index: u64) -> Result<u32, String> {
        if self.uniform > 0 {
            return Ok(self.uniform);
        }
        self.sizes.get(index as usize).copied().ok_or_else(|| {
            "the sample size table is shorter than the samples it counts".to_string()
        })
    }
}

fn read_moov(index: &[u8], bytes: u64, info: &mut MediaInfo) -> Playback {
    if let Some(mvhd) = find(index, &[b"mvhd"]) {
        let body = &index[mvhd.start..mvhd.end];
        info.duration = movie_duration(body);
    }

    let mut tracks: Vec<TrackIndex> = Vec::new();
    let mut video = None;
    let mut audio = None;
    for atom in atoms(index) {
        if &atom.tag != b"trak" {
            continue;
        }
        let track = &index[atom.start..atom.end];
        let Some(media) = find(track, &[b"mdia"]) else {
            continue;
        };
        let media = &track[media.start..media.end];
        let Some(handler) = find(media, &[b"hdlr"]) else {
            continue;
        };
        let body = &media[handler.start..handler.end];
        if body.len() < 12 {
            continue;
        }
        let handler = [body[8], body[9], body[10], body[11]];
        let (codec, configured) = sample_format(media).unwrap_or((None, false));
        let (timescale, duration) = find(media, &[b"mdhd"])
            .and_then(|atom| media_duration(&media[atom.start..atom.end]))
            .unwrap_or((0, 0));
        let table = match track_table(media) {
            Ok(table) => table,
            Err(reason) => return Playback::Broken(reason),
        };
        match handler {
            [b'v', b'i', b'd', b'e'] => video = codec.clone().or(Some("unknown".to_string())),
            [b's', b'o', b'u', b'n'] => audio = codec.clone().or(Some("unknown".to_string())),
            _ => {}
        }
        tracks.push(TrackIndex {
            handler,
            configured,
            timescale,
            duration,
            table,
        });
    }
    info.video_codec = video;
    info.audio_codec = audio;
    judge_tracks(&tracks, info, bytes)
}

/// Decide what the index proves about the bytes, or where it contradicts them.
fn judge_tracks(tracks: &[TrackIndex], info: &mut MediaInfo, bytes: u64) -> Playback {
    let mut placed = 0u64;
    let mut unplaced = false;
    for track in tracks {
        let sample_track = track.handler == *b"vide" || track.handler == *b"soun";
        let Some(table) = &track.table else {
            unplaced |= sample_track;
            continue;
        };
        if table.count == 0 {
            continue;
        }
        if let Err(reason) = placed_samples(table, bytes) {
            return Playback::Broken(reason);
        }
        placed = placed.saturating_add(table.count);
        if let Some(sync) = &table.sync {
            if sync.iter().any(|number| *number == 0 || *number as u64 > table.count) {
                return Playback::Broken(
                    "the keyframe table names a sample that is not in the track".to_string(),
                );
            }
        }
        if let Some((samples, _)) = table.timeline {
            if samples != table.count {
                return Playback::Broken(format!(
                    "the timing table names {samples} samples and the sample table names {}",
                    table.count
                ));
            }
        }
    }
    info.sample_count = placed;
    if placed == 0 {
        return Playback::Unverified("the index does not say where its samples are");
    }
    if unplaced {
        return Playback::Unverified("a track has no sample table");
    }
    if tracks
        .iter()
        .any(|track| (track.handler == *b"vide" || track.handler == *b"soun") && !track.configured)
    {
        return Playback::Unverified("a track carries no codec configuration");
    }
    // A movie shorter than one of its own tracks means its length was written
    // from something other than the tracks themselves.
    for track in tracks {
        if track.timescale == 0 || track.duration == 0 {
            continue;
        }
        let seconds = track.duration as f64 / track.timescale as f64;
        if info
            .duration
            .is_some_and(|movie| seconds > movie * 1.05 + 0.5)
        {
            return Playback::Unverified("the movie is shorter than one of its tracks");
        }
    }
    Playback::Verified
}

/// The sample tables of one track, when it has any.
fn track_table(media: &[u8]) -> Result<Option<SampleTable>, String> {
    let Some(minf) = find(media, &[b"minf"]) else {
        return Ok(None);
    };
    let minf = &media[minf.start..minf.end];
    let Some(stbl) = find(minf, &[b"stbl"]) else {
        return Ok(None);
    };
    sample_table(&minf[stbl.start..stbl.end])
}

fn sample_table(stbl: &[u8]) -> Result<Option<SampleTable>, String> {
    let Some(stsz) = find(stbl, &[b"stsz"]) else {
        return Ok(None);
    };
    let body = &stbl[stsz.start..stsz.end];
    if body.len() < 12 {
        return Ok(None);
    }
    let uniform = be32(body, 4)?;
    let count = be32(body, 8)? as u64;
    let mut sizes = Vec::new();
    if uniform == 0 {
        let room = (body.len() - 12) / 4;
        if count as usize > room {
            return Err("the sample size table is shorter than the samples it counts".to_string());
        }
        sizes.reserve(count as usize);
        for index in 0..count as usize {
            sizes.push(be32(body, 12 + index * 4)?);
        }
    }
    // Without chunk offsets the sizes say nothing about the file; that is not
    // a contradiction, only something that cannot be checked.
    let Some(chunk_offsets) = chunk_offsets(stbl)? else {
        return Ok(None);
    };
    Ok(Some(SampleTable {
        count,
        uniform,
        sizes,
        chunk_offsets,
        runs: chunk_runs(stbl)?,
        sync: sync_samples(stbl)?,
        timeline: timeline(stbl)?,
    }))
}

/// Walk the sample tables and return the byte the last sample ends at.
///
/// The walk is what turns an index into a claim about the file: a chunk offset
/// plus the sizes of the samples in it is a byte position, and a position past
/// the end of the file is a download that stopped early.
fn placed_samples(table: &SampleTable, file_bytes: u64) -> Result<u64, String> {
    if table.count == 0 {
        return Ok(0);
    }
    if table.runs.is_empty() {
        return Err("the index names samples but not which chunk holds them".to_string());
    }
    let mut placed = 0u64;
    let mut end = 0u64;
    let mut next_chunk = 1u32;
    for (index, run) in table.runs.iter().enumerate() {
        let (first, per_chunk) = *run;
        let until = table
            .runs
            .get(index + 1)
            .map(|next| next.0)
            .unwrap_or(u32::MAX);
        if first != next_chunk {
            return Err("the sample-to-chunk table skips a chunk".to_string());
        }
        if until <= first {
            return Err("the sample-to-chunk table does not move forward".to_string());
        }
        if per_chunk == 0 {
            return Err("a chunk in the sample-to-chunk table holds no samples".to_string());
        }
        if placed < table.count && (first as usize - 1) >= table.chunk_offsets.len() {
            return Err(
                "the sample-to-chunk table names a chunk the file does not have".to_string(),
            );
        }
        let mut chunk = (first - 1) as usize;
        while (chunk as u32) < until && chunk < table.chunk_offsets.len() {
            let mut at = table.chunk_offsets[chunk];
            for _ in 0..per_chunk {
                if placed >= table.count {
                    break;
                }
                at = at.saturating_add(table.size_of(placed)? as u64);
                placed += 1;
            }
            end = end.max(at);
            chunk += 1;
        }
        next_chunk = until;
    }
    if placed != table.count {
        return Err(format!(
            "the sample tables disagree: {placed} of {} samples are placed",
            table.count
        ));
    }
    if end > file_bytes {
        return Err("the index names a sample that reaches past the end of the file".to_string());
    }
    Ok(end)
}

fn chunk_offsets(stbl: &[u8]) -> Result<Option<Vec<u64>>, String> {
    if let Some(atom) = find(stbl, &[b"co64"]) {
        let body = &stbl[atom.start..atom.end];
        let count = be32(body, 4)? as usize;
        if 8 + count.saturating_mul(8) > body.len() {
            return Err("the chunk offset table is shorter than the chunks it counts".to_string());
        }
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            out.push(be64(body, 8 + index * 8)?);
        }
        return Ok(Some(out));
    }
    if let Some(atom) = find(stbl, &[b"stco"]) {
        let body = &stbl[atom.start..atom.end];
        let count = be32(body, 4)? as usize;
        if 8 + count.saturating_mul(4) > body.len() {
            return Err("the chunk offset table is shorter than the chunks it counts".to_string());
        }
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            out.push(be32(body, 8 + index * 4)? as u64);
        }
        return Ok(Some(out));
    }
    Ok(None)
}

fn chunk_runs(stbl: &[u8]) -> Result<Vec<(u32, u32)>, String> {
    let Some(atom) = find(stbl, &[b"stsc"]) else {
        return Ok(Vec::new());
    };
    let body = &stbl[atom.start..atom.end];
    let count = be32(body, 4)? as usize;
    if 8 + count.saturating_mul(12) > body.len() {
        return Err("the sample-to-chunk table is shorter than the runs it counts".to_string());
    }
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let at = 8 + index * 12;
        out.push((be32(body, at)?, be32(body, at + 4)?));
    }
    Ok(out)
}

fn sync_samples(stbl: &[u8]) -> Result<Option<Vec<u32>>, String> {
    let Some(atom) = find(stbl, &[b"stss"]) else {
        return Ok(None);
    };
    let body = &stbl[atom.start..atom.end];
    let count = be32(body, 4)? as usize;
    if 8 + count.saturating_mul(4) > body.len() {
        return Err("the keyframe table is shorter than the keyframes it counts".to_string());
    }
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        out.push(be32(body, 8 + index * 4)?);
    }
    Ok(Some(out))
}

fn timeline(stbl: &[u8]) -> Result<Option<(u64, u64)>, String> {
    let Some(atom) = find(stbl, &[b"stts"]) else {
        return Ok(None);
    };
    let body = &stbl[atom.start..atom.end];
    let count = be32(body, 4)? as usize;
    if 8 + count.saturating_mul(8) > body.len() {
        return Err("the timing table is shorter than the runs it counts".to_string());
    }
    let mut samples = 0u64;
    let mut units = 0u64;
    for index in 0..count {
        let at = 8 + index * 8;
        let run = be32(body, at)? as u64;
        let delta = be32(body, at + 4)? as u64;
        samples = samples.saturating_add(run);
        units = units.saturating_add(run.saturating_mul(delta));
    }
    Ok(Some((samples, units)))
}

/// The first sample entry's format, e.g. `avc1` or `mp4a`, and whether it
/// states how its decoder is configured.
fn sample_format(media: &[u8]) -> Option<(Option<String>, bool)> {
    let minf = find(media, &[b"minf"])?;
    let minf = &media[minf.start..minf.end];
    let stbl = find(minf, &[b"stbl"])?;
    let stbl = &minf[stbl.start..stbl.end];
    let stsd = find(stbl, &[b"stsd"])?;
    let body = &stbl[stsd.start..stsd.end];
    if body.len() < 16 {
        return None;
    }
    // version/flags, entry count, then the first entry: size, format, ...
    let format = &body[12..16];
    let size = u32::from_be_bytes([body[8], body[9], body[10], body[11]]) as usize;
    // A sample entry states its own length; its children start after the
    // eight-byte box header and the sixteen fixed bytes of the entry itself.
    let end = (8 + size).clamp(16, body.len());
    let configured = find(
        &body[24.min(end)..end],
        &[b"avcC", b"hvcC", b"av1C", b"vpcC", b"esds", b"dOps", b"dac3", b"dec3"],
    )
    .is_some();
    Some((
        Some(String::from_utf8_lossy(format).trim().to_string()),
        configured,
    ))
}

fn be32(bytes: &[u8], at: usize) -> Result<u32, String> {
    let end = at + 4;
    if end > bytes.len() {
        return Err("an index table is truncated".to_string());
    }
    Ok(u32::from_be_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
    ]))
}

fn be64(bytes: &[u8], at: usize) -> Result<u64, String> {
    let end = at + 8;
    if end > bytes.len() {
        return Err("an index table is truncated".to_string());
    }
    Ok(u64::from_be_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
        bytes[at + 4],
        bytes[at + 5],
        bytes[at + 6],
        bytes[at + 7],
    ]))
}

/// A track's own timescale and duration, from its media header.
fn media_duration(body: &[u8]) -> Option<(u32, u64)> {
    let version = *body.first()?;
    if version == 1 {
        Some((
            u32::from_be_bytes([
                *body.get(20)?,
                *body.get(21)?,
                *body.get(22)?,
                *body.get(23)?,
            ]),
            u64::from_be_bytes([
                *body.get(24)?,
                *body.get(25)?,
                *body.get(26)?,
                *body.get(27)?,
                *body.get(28)?,
                *body.get(29)?,
                *body.get(30)?,
                *body.get(31)?,
            ]),
        ))
    } else {
        Some((
            u32::from_be_bytes([
                *body.get(12)?,
                *body.get(13)?,
                *body.get(14)?,
                *body.get(15)?,
            ]),
            u32::from_be_bytes([
                *body.get(16)?,
                *body.get(17)?,
                *body.get(18)?,
                *body.get(19)?,
            ]) as u64,
        ))
    }
}

fn movie_duration(body: &[u8]) -> Option<f64> {
    let version = *body.first()?;
    let (timescale, duration) = match version {
        1 => (
            u32::from_be_bytes([
                *body.get(20)?,
                *body.get(21)?,
                *body.get(22)?,
                *body.get(23)?,
            ]),
            u64::from_be_bytes([
                *body.get(24)?,
                *body.get(25)?,
                *body.get(26)?,
                *body.get(27)?,
                *body.get(28)?,
                *body.get(29)?,
                *body.get(30)?,
                *body.get(31)?,
            ]),
        ),
        _ => (
            u32::from_be_bytes([
                *body.get(12)?,
                *body.get(13)?,
                *body.get(14)?,
                *body.get(15)?,
            ]),
            u32::from_be_bytes([
                *body.get(16)?,
                *body.get(17)?,
                *body.get(18)?,
                *body.get(19)?,
            ]) as u64,
        ),
    };
    if timescale == 0 || duration == 0 {
        return None;
    }
    Some(duration as f64 / timescale as f64)
}

/* ----------------------------------------------------------------- matroska */

/// Read the EBML header, then `Info` and `Tracks` from the segment.
fn parse_matroska(file: &mut File, bytes: u64) -> Result<MediaInfo, String> {
    let window = bytes.min(MAX_INDEX_BYTES);
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("the file could not be read: {error}"))?;
    let mut body = vec![0u8; window as usize];
    let mut read = 0usize;
    while read < body.len() {
        let got = file
            .read(&mut body[read..])
            .map_err(|error| format!("the file could not be read: {error}"))?;
        if got == 0 {
            break;
        }
        read += got;
    }
    body.truncate(read);

    let mut info = MediaInfo::empty(MediaContainer::Matroska, bytes);
    let header = elements(&body, 0, body.len())
        .into_iter()
        .find(|element| element.id == EBML_HEADER)
        .ok_or_else(|| "the file has no Matroska header".to_string())?;
    if let Some(doc) = find_element(&body, &header, 0x4282) {
        let kind = String::from_utf8_lossy(&body[doc.data..doc.end]);
        if kind.trim_end_matches('\0') == "webm" {
            info.container = MediaContainer::Webm;
        }
    }

    let Some(segment) = elements(&body, header.end, body.len())
        .into_iter()
        .find(|element| element.id == SEGMENT)
    else {
        // The header is there but the Segment it introduces is not: the file
        // stopped before it, or it promises more bytes than the file holds.
        info.playback =
            Playback::Broken("the file has a header but no complete Segment".to_string());
        return Ok(info);
    };

    let mut timestamp_scale = 1_000_000u64;
    let mut duration = None;
    if let Some(section) = find_element(&body, &segment, 0x1549A966) {
        if let Some(scale) = find_element(&body, &section, 0x2AD7B1) {
            timestamp_scale = read_uint(&body[scale.data..scale.end]).unwrap_or(timestamp_scale);
        }
        if let Some(length) = find_element(&body, &section, 0x4489) {
            duration = read_float(&body[length.data..length.end]);
        }
    }
    if let Some(seconds) = duration {
        if timestamp_scale > 0 {
            info.duration = Some(seconds * timestamp_scale as f64 / 1_000_000_000.0);
        }
    }

    let mut tracks = 0usize;
    if let Some(section) = find_element(&body, &segment, 0x1654AE6B) {
        for track in elements(&body, section.data, section.end) {
            if track.id != 0xAE {
                continue;
            }
            tracks += 1;
            let kind = find_element(&body, &track, 0x83)
                .and_then(|atom| read_uint(&body[atom.data..atom.end]))
                .unwrap_or(0);
            let codec = find_element(&body, &track, 0x86).map(|atom| {
                String::from_utf8_lossy(&body[atom.data..atom.end])
                    .trim_end_matches('\0')
                    .to_string()
            });
            match kind {
                1 if info.video_codec.is_none() => info.video_codec = codec,
                2 if info.audio_codec.is_none() => info.audio_codec = codec,
                _ => {}
            }
        }
    }

    // The segment's own children only end cleanly at the end of the file. A
    // last cluster that declares more bytes than remain is a download that
    // stopped in the middle of a frame, and no player recovers from that.
    if body.len() as u64 == bytes && !walk_complete(&body, segment.data, segment.end) {
        info.playback =
            Playback::Broken("the file ends inside an element it declared".to_string());
        return Ok(info);
    }
    info.playback = if tracks == 0 {
        Playback::Unverified("the file declares no tracks")
    } else if duration.is_none() {
        Playback::Unverified("the file does not state its duration")
    } else if body.len() as u64 != bytes {
        // Only the head of a large file was read; the tail could not be walked.
        Playback::Unverified("only the first part of the file was read")
    } else {
        Playback::Verified
    };
    Ok(info)
}

/// Whether a range's children can be walked to `to` without one of them
/// declaring more bytes than the range holds.
fn walk_complete(bytes: &[u8], from: usize, to: usize) -> bool {
    let mut at = from;
    while at < to {
        let Some((_, id_len)) = read_id(&bytes[at..to]) else {
            return false;
        };
        let Some((size, size_len)) = read_size(&bytes[at + id_len..to]) else {
            return false;
        };
        let data = at + id_len + size_len;
        match size {
            // An unknown size runs to the end of its parent, which is exactly
            // what a streamed file looks like.
            None => return true,
            Some(size) => {
                let end = data.saturating_add(size);
                if end > to {
                    return false;
                }
                at = end;
            }
        }
    }
    true
}

const EBML_HEADER: u64 = 0x1A45_DFA3;
const SEGMENT: u64 = 0x1853_8067;

struct Element {
    id: u64,
    data: usize,
    end: usize,
}

/// The children of one EBML range.
fn elements(bytes: &[u8], from: usize, to: usize) -> Vec<Element> {
    let mut out = Vec::new();
    let mut at = from;
    while at < to {
        let Some((id, id_len)) = read_id(&bytes[at..to]) else {
            break;
        };
        let Some((size, size_len)) = read_size(&bytes[at + id_len..to]) else {
            break;
        };
        let data = at + id_len + size_len;
        let end = match size {
            None => to,
            Some(size) => {
                let end = data.saturating_add(size);
                if end > to {
                    // The element continues past the window that was read.
                    break;
                }
                end
            }
        };
        out.push(Element { id, data, end });
        at = end;
    }
    out
}

fn find_element(bytes: &[u8], parent: &Element, id: u64) -> Option<Element> {
    elements(bytes, parent.data, parent.end)
        .into_iter()
        .find(|element| element.id == id)
}

fn read_id(bytes: &[u8]) -> Option<(u64, usize)> {
    let first = *bytes.first()?;
    let length = first.leading_zeros() as usize + 1;
    if length > 4 || bytes.len() < length {
        return None;
    }
    let mut id = 0u64;
    for byte in &bytes[..length] {
        id = (id << 8) | *byte as u64;
    }
    Some((id, length))
}

/// The size value, with the marker bits removed. `None` is EBML's unknown size.
fn read_size(bytes: &[u8]) -> Option<(Option<usize>, usize)> {
    let first = *bytes.first()?;
    let length = first.leading_zeros() as usize + 1;
    if length > 8 || bytes.len() < length {
        return None;
    }
    let mut value = (first as u64) & (0xFF >> length);
    let mut all_ones = value == (0xFF >> length) as u64;
    for byte in &bytes[1..length] {
        value = (value << 8) | *byte as u64;
        all_ones &= *byte == 0xFF;
    }
    if all_ones {
        return Some((None, length));
    }
    Some((Some(value as usize), length))
}

fn read_uint(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || bytes.len() > 8 {
        return None;
    }
    Some(
        bytes
            .iter()
            .fold(0u64, |value, byte| (value << 8) | *byte as u64),
    )
}

fn read_float(bytes: &[u8]) -> Option<f64> {
    match bytes.len() {
        4 => Some(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64),
        8 => Some(f64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bdm-verify-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        dir
    }

    /// A minimal MP4: `ftyp`, a `moov` with one video track and one sound
    /// track, and an empty `mdat`. The bytes are built here so the parser is
    /// tested without a real file.
    fn sample_mp4(duration: u32) -> Vec<u8> {
        fn atom(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = Vec::with_capacity(body.len() + 8);
            out.extend(((body.len() + 8) as u32).to_be_bytes());
            out.extend(tag);
            out.extend(body);
            out
        }
        fn mvhd(duration: u32) -> Vec<u8> {
            let mut body = vec![0u8; 100];
            body[12..16].copy_from_slice(&1000u32.to_be_bytes());
            body[16..20].copy_from_slice(&duration.to_be_bytes());
            atom(b"mvhd", &body)
        }
        fn hdlr(kind: &[u8; 4]) -> Vec<u8> {
            let mut body = vec![0u8; 25];
            body[8..12].copy_from_slice(kind);
            atom(b"hdlr", &body)
        }
        fn stsd(format: &[u8; 4]) -> Vec<u8> {
            let mut entry = vec![0u8; 16];
            entry[4..8].copy_from_slice(format);
            let mut body = vec![0u8; 8];
            body[4..8].copy_from_slice(&1u32.to_be_bytes());
            body.extend(entry);
            atom(b"stsd", &body)
        }
        fn trak(kind: &[u8; 4], format: &[u8; 4]) -> Vec<u8> {
            let stbl = atom(b"stbl", &stsd(format));
            let minf = atom(b"minf", &stbl);
            let media = atom(b"mdia", &[hdlr(kind), minf].concat());
            atom(b"trak", &media)
        }
        let moov = atom(
            b"moov",
            &[
                mvhd(duration),
                trak(b"vide", b"avc1"),
                trak(b"soun", b"mp4a"),
            ]
            .concat(),
        );
        let mut ftyp = Vec::new();
        ftyp.extend(b"isom");
        ftyp.extend(0u32.to_be_bytes());
        ftyp.extend(b"isomiso2avc1mp41");
        let mut out = atom(b"ftyp", &ftyp);
        out.extend(moov);
        out.extend(atom(b"mdat", &[0u8; 32]));
        out
    }

    #[test]
    fn an_mp4_index_names_its_tracks_and_duration() {
        let dir = temp_dir("mp4");
        let path = dir.join("film.mp4");
        std::fs::write(&path, sample_mp4(10_000)).expect("the file");

        let info = inspect(&path).expect("an inspection");
        assert_eq!(info.container, MediaContainer::Mp4);
        assert_eq!(info.duration, Some(10.0));
        assert_eq!(info.video_codec.as_deref(), Some("avc1"));
        assert_eq!(info.audio_codec.as_deref(), Some("mp4a"));

        // The playback check is exercised against real media in
        // `tests/media_pipeline.rs`; this fixture has an index but no samples.
        let info = inspect(&path).expect("the same file read twice");
        assert_eq!(info.video_codec.as_deref(), Some("avc1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_picture_is_refused() {
        // A join test may have pointed `BDM_FFMPEG` at a stand-in.
        let _guard = super::super::tools::test_guard();
        let dir = temp_dir("nopicture");
        let path = dir.join("film.mp4");
        let mut bytes = sample_mp4(5_000);
        // Break the video track's handler into a text track.
        let at = bytes
            .windows(4)
            .position(|window| window == b"vide")
            .expect("the handler");
        bytes[at..at + 4].copy_from_slice(b"text");
        std::fs::write(&path, bytes).expect("the file");

        let error = check_finished(
            &path,
            Expect {
                extension: "mp4",
                video: true,
                audio: false,
                audio_selected: false,
            },
        )
        .expect_err("no picture");
        assert!(error.contains("picture"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_selected_audio_track_that_is_lost_is_refused_but_a_silent_source_is_kept() {
        // A join test may have pointed `BDM_FFMPEG` at a stand-in.
        let _guard = super::super::tools::test_guard();
        let dir = temp_dir("silent");
        let path = dir.join("film.mp4");
        let mut bytes = sample_mp4(5_000);
        // Break the sound track's handler into a text track: the source itself
        // carries no audio, which is a silent clip and not a broken download.
        let at = bytes
            .windows(4)
            .position(|window| window == b"soun")
            .expect("the audio handler");
        bytes[at..at + 4].copy_from_slice(b"text");
        std::fs::write(&path, bytes).expect("the file");

        let kept = check_finished(
            &path,
            Expect {
                extension: "mp4",
                video: true,
                audio: true,
                audio_selected: false,
            },
        )
        .err()
        .unwrap_or_default();
        // The fixture has no samples, so the playback check refuses it; what
        // matters is that no sound is not what refused it.
        assert!(!kept.contains("no sound"), "{kept}");

        // The same file is a broken download when a separate audio track was
        // fetched to be joined onto it.
        let error = check_finished(
            &path,
            Expect {
                extension: "mp4",
                video: true,
                audio: true,
                audio_selected: true,
            },
        )
        .expect_err("a lost audio track");
        assert!(error.contains("no sound"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_html_page_named_mp4_is_not_a_video() {
        // A join test may have pointed `BDM_FFMPEG` at a stand-in.
        let _guard = super::super::tools::test_guard();
        let dir = temp_dir("html");
        let path = dir.join("film.mp4");
        std::fs::write(&path, b"<!doctype html><title>Not found</title>").expect("the file");

        let info = inspect(&path).expect("an inspection");
        assert_eq!(info.container, MediaContainer::Unknown);
        let error = check_finished(
            &path,
            Expect {
                extension: "mp4",
                video: true,
                audio: false,
                audio_selected: false,
            },
        )
        .expect_err("not a video");
        assert!(error.contains("not a playable video file"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_with_almost_no_time_is_refused() {
        // A join test may have pointed `BDM_FFMPEG` at a stand-in. Ten
        // milliseconds is stated by the index, so the refusal does not depend
        // on whether a decoder happens to read the sample.
        let _guard = super::super::tools::test_guard();
        let dir = temp_dir("noduration");
        let path = dir.join("film.mp4");
        std::fs::write(&path, sample_mp4(10)).expect("the file");
        let error = check_finished(
            &path,
            Expect {
                extension: "mp4",
                video: true,
                audio: false,
                audio_selected: false,
            },
        )
        .expect_err("almost no time");
        assert!(error.contains("no time long"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_truncated_mp4_has_no_index() {
        let dir = temp_dir("truncated");
        let path = dir.join("film.mp4");
        let mut bytes = sample_mp4(5_000);
        bytes.truncate(40);
        std::fs::write(&path, bytes).expect("the file");
        let error = inspect(&path).expect_err("no index");
        assert!(error.contains("index"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_adts_stream_is_recognised_and_is_not_an_mp4() {
        let dir = temp_dir("adts");
        let path = dir.join("sound.aac");
        // Two ADTS frames with a silent payload.
        let mut bytes = Vec::new();
        for _ in 0..2 {
            bytes.extend([0xFF, 0xF1, 0x50, 0x80, 0x01, 0x1F, 0xFC]);
            bytes.extend([0u8; 100]);
        }
        std::fs::write(&path, bytes).expect("the file");
        let info = inspect(&path).expect("an inspection");
        assert_eq!(info.container, MediaContainer::Adts);
        assert!(!MediaContainer::Adts.fits_extension("m4a"));
        assert!(MediaContainer::Adts.fits_extension("aac"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_truncated_matroska_header_is_refused() {
        let dir = temp_dir("matroska");
        let path = dir.join("film.mkv");
        std::fs::write(&path, [0x1A, 0x45, 0xDF, 0xA3, 0x00]).expect("the file");
        let error = inspect(&path).expect_err("no header");
        assert!(!error.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_extension_only_accepts_what_it_can_hold() {
        assert!(MediaContainer::Mp4.fits_extension("m4a"));
        assert!(MediaContainer::Mp4.fits_extension("MP4"));
        assert!(!MediaContainer::Mp4.fits_extension("webm"));
        assert!(MediaContainer::Webm.fits_extension("mkv"));
        assert!(MediaContainer::Matroska.fits_extension("webm"));
        assert!(!MediaContainer::MpegTs.fits_extension("mp4"));
    }

    /* ------------------------------------------- deep index checks */

    fn boxed(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(body.len() + 8);
        out.extend(((body.len() + 8) as u32).to_be_bytes());
        out.extend(tag);
        out.extend(body);
        out
    }

    /// A complete, self-consistent MP4: two fifty-byte samples in one chunk,
    /// one of them a keyframe, two seconds long, with a decoder configuration.
    ///
    /// The chunk offset is a parameter because it can only be known once the
    /// boxes in front of the `mdat` have been measured.
    fn indexed_mp4(offset: u64) -> Vec<u8> {
        let mut stts = 0u32.to_be_bytes().to_vec();
        stts.extend(1u32.to_be_bytes());
        stts.extend(2u32.to_be_bytes());
        stts.extend(1000u32.to_be_bytes());
        let mut stsc = 0u32.to_be_bytes().to_vec();
        stsc.extend(1u32.to_be_bytes());
        stsc.extend(1u32.to_be_bytes());
        stsc.extend(2u32.to_be_bytes());
        stsc.extend(1u32.to_be_bytes());
        let mut stsz = 0u32.to_be_bytes().to_vec();
        stsz.extend(0u32.to_be_bytes());
        stsz.extend(2u32.to_be_bytes());
        stsz.extend(50u32.to_be_bytes());
        stsz.extend(50u32.to_be_bytes());
        let mut co64 = 0u32.to_be_bytes().to_vec();
        co64.extend(1u32.to_be_bytes());
        co64.extend(offset.to_be_bytes());
        let mut stss = 0u32.to_be_bytes().to_vec();
        stss.extend(1u32.to_be_bytes());
        stss.extend(1u32.to_be_bytes());

        let mut entry = vec![0u8; 16];
        let configuration = boxed(b"avcC", &[1, 2, 3, 4]);
        entry[0..4].copy_from_slice(&((16 + configuration.len()) as u32).to_be_bytes());
        entry[4..8].copy_from_slice(b"avc1");
        entry.extend(configuration);
        let mut stsd = 0u32.to_be_bytes().to_vec();
        stsd.extend(1u32.to_be_bytes());
        stsd.extend(entry);

        let stbl = boxed(
            b"stbl",
            &[
                boxed(b"stsd", &stsd),
                boxed(b"stts", &stts),
                boxed(b"stsc", &stsc),
                boxed(b"stsz", &stsz),
                boxed(b"co64", &co64),
                boxed(b"stss", &stss),
            ]
            .concat(),
        );
        let mut mdhd = vec![0u8; 24];
        mdhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
        mdhd[16..20].copy_from_slice(&2000u32.to_be_bytes());
        let mut hdlr = vec![0u8; 25];
        hdlr[8..12].copy_from_slice(b"vide");
        let mdia = boxed(
            b"mdia",
            &[boxed(b"mdhd", &mdhd), boxed(b"hdlr", &hdlr), boxed(b"minf", &stbl)].concat(),
        );

        let mut mvhd = vec![0u8; 100];
        mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
        mvhd[16..20].copy_from_slice(&2000u32.to_be_bytes());
        let moov = boxed(b"moov", &[boxed(b"mvhd", &mvhd), boxed(b"trak", &mdia)].concat());

        let mut ftyp = Vec::new();
        ftyp.extend(b"isom");
        ftyp.extend(0u32.to_be_bytes());
        ftyp.extend(b"isomiso2avc1mp41");
        let mut out = boxed(b"ftyp", &ftyp);
        out.extend(moov);
        out.extend(boxed(b"mdat", &[0u8; 100]));
        out
    }

    /// The same file, with its chunk offset pointing at the real payload.
    fn complete_mp4() -> Vec<u8> {
        let probe = indexed_mp4(0);
        let offset = probe.len() as u64 - 100;
        indexed_mp4(offset)
    }

    fn place(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("the file");
        path
    }

    fn picture() -> Expect<'static> {
        Expect {
            extension: "mp4",
            video: true,
            audio: false,
            audio_selected: false,
        }
    }

    #[test]
    fn a_complete_index_is_verified_against_the_file() {
        let dir = temp_dir("indexed");
        let path = place(&dir, "film.mp4", &complete_mp4());

        let info = inspect(&path).expect("an inspection");
        assert_eq!(info.playback, Playback::Verified);
        assert_eq!(info.sample_count, 2, "both samples are accounted for");

        let checked = check_finished(&path, picture()).expect("a good file");
        assert_eq!(checked.duration, Some(2.0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_sample_that_reaches_past_the_end_of_the_file_is_refused() {
        let _guard = super::super::tools::test_guard();
        let dir = temp_dir("short-mdat");
        // The index still names a hundred bytes of samples; only sixty remain.
        let mut bytes = complete_mp4();
        bytes.truncate(bytes.len() - 40);
        let path = place(&dir, "film.mp4", &bytes);

        let info = inspect(&path).expect("an inspection");
        assert!(
            matches!(&info.playback, Playback::Broken(reason) if reason.contains("past the end")),
            "{:?}",
            info.playback
        );
        let error = check_finished(&path, picture()).expect_err("a truncated download");
        assert!(error.contains("past the end"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_keyframe_that_is_not_a_sample_is_refused() {
        let dir = temp_dir("keyframe");
        let mut bytes = complete_mp4();
        let at = bytes
            .windows(4)
            .position(|window| window == b"stss")
            .expect("the keyframe table");
        // The table names sample 99 of a track that has two. The first entry
        // follows version/flags and the count.
        bytes[at + 12..at + 16].copy_from_slice(&99u32.to_be_bytes());
        let path = place(&dir, "film.mp4", &bytes);

        let info = inspect(&path).expect("an inspection");
        assert!(
            matches!(&info.playback, Playback::Broken(reason) if reason.contains("keyframe")),
            "{:?}",
            info.playback
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_timing_table_that_names_another_count_is_refused() {
        let dir = temp_dir("timing");
        let mut bytes = complete_mp4();
        let at = bytes
            .windows(4)
            .position(|window| window == b"stts")
            .expect("the timing table");
        // Three samples of timing for a track whose sample table has two. The
        // run count is the first field of the body, after version/flags.
        bytes[at + 8..at + 12].copy_from_slice(&3u32.to_be_bytes());
        let path = place(&dir, "film.mp4", &bytes);

        let info = inspect(&path).expect("an inspection");
        assert!(
            matches!(&info.playback, Playback::Broken(reason) if reason.contains("timing table")),
            "{:?}",
            info.playback
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_size_table_shorter_than_its_own_count_is_refused() {
        let dir = temp_dir("sizes");
        let mut bytes = complete_mp4();
        let at = bytes
            .windows(4)
            .position(|window| window == b"stsz")
            .expect("the size table");
        // Four samples counted, two sizes given. The count lives at body byte
        // eight: version/flags, the uniform size, then the count.
        bytes[at + 12..at + 16].copy_from_slice(&4u32.to_be_bytes());
        let path = place(&dir, "film.mp4", &bytes);

        let info = inspect(&path).expect("an inspection");
        assert!(
            matches!(&info.playback, Playback::Broken(reason) if reason.contains("size table")),
            "{:?}",
            info.playback
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /* --------------------------------------- matroska tail checks */

    /// One EBML element with a one- or two-byte size.
    fn ebml(id: &[u8], body: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        let size = body.len();
        if size < 0x7F {
            out.push(0x80 | size as u8);
        } else {
            out.push(0x40 | (size >> 8) as u8);
            out.push(size as u8);
        }
        out.extend_from_slice(body);
        out
    }

    /// The same element, declaring a size its body does not have — which is
    /// exactly what a download cut short leaves behind.
    fn overrunning(id: &[u8], body: &[u8], declared: usize) -> Vec<u8> {
        let mut out = id.to_vec();
        out.push(0x40 | (declared >> 8) as u8);
        out.push(declared as u8);
        out.extend_from_slice(body);
        out
    }

    fn webm_clip(declared_cluster: Option<usize>) -> Vec<u8> {
        let header = ebml(&[0x1A, 0x45, 0xDF, 0xA3], &ebml(&[0x42, 0x82], b"webm"));
        let mut info = ebml(&[0x2A, 0xD7, 0xB1], &[0x0F, 0x42, 0x40]);
        info.extend(ebml(&[0x44, 0x89], &1000.0f64.to_be_bytes()));
        let mut entry = ebml(&[0xD7], &[1]);
        entry.extend(ebml(&[0x83], &[1]));
        entry.extend(ebml(&[0x86], b"V_VP9"));
        let mut payload = ebml(&[0xE7], &[0]);
        payload.extend(ebml(&[0xA3], &[0x81, 0, 0, 0x80, 1, 2, 3]));

        let mut out = header;
        out.extend([0x18, 0x53, 0x80, 0x67]);
        out.extend([0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        out.extend(ebml(&[0x15, 0x49, 0xA9, 0x66], &info));
        out.extend(ebml(&[0x16, 0x54, 0xAE, 0x6B], &ebml(&[0xAE], &entry)));
        out.extend(match declared_cluster {
            Some(size) => overrunning(&[0x1F, 0x43, 0xB6, 0x75], &payload, size),
            None => ebml(&[0x1F, 0x43, 0xB6, 0x75], &payload),
        });
        out
    }

    #[test]
    fn a_webm_whose_last_cluster_overruns_the_file_is_refused() {
        let _guard = super::super::tools::test_guard();
        let dir = temp_dir("webm-short");
        let body = webm_clip(None);
        // A cluster that says it holds twenty bytes more than it does.
        let path = place(&dir, "film.webm", &webm_clip(Some(body.len() - 60 + 20)));

        let info = inspect(&path).expect("an inspection");
        assert!(
            matches!(&info.playback, Playback::Broken(reason) if reason.contains("ends inside")),
            "{:?}",
            info.playback
        );
        let error = check_finished(
            &path,
            Expect {
                extension: "webm",
                video: true,
                audio: false,
                audio_selected: false,
            },
        )
        .expect_err("a truncated download");
        assert!(error.contains("ends inside"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_webm_that_walks_cleanly_is_verified() {
        let _guard = super::super::tools::test_guard();
        let dir = temp_dir("webm-whole");
        let path = place(&dir, "film.webm", &webm_clip(None));

        let info = check_finished(
            &path,
            Expect {
                extension: "webm",
                video: true,
                audio: false,
                audio_selected: false,
            },
        )
        .expect("a whole file");
        assert_eq!(info.playback, Playback::Verified);
        assert_eq!(info.duration, Some(1.0));
        assert_eq!(info.video_codec.as_deref(), Some("V_VP9"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
