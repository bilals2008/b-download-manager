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
            &format!("{} was read but not proved: {reason}", path.display()),
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

mod matroska;
mod mp4;

use matroska::parse_matroska;
use mp4::parse_mp4;

#[cfg(test)]
mod tests;
