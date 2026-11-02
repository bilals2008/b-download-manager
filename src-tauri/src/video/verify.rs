//! Reading what a finished file actually is.
//!
//! Nothing here trusts a file name, an exit code or a manifest. The bytes are
//! read back and the container is parsed: an MP4's `moov` index gives its
//! tracks, codecs and duration; a Matroska/WebM header gives the same; a byte
//! stream that is neither is not a deliverable. When `ffmpeg` is present, the
//! first seconds are decoded as well, so a file whose index lies about its
//! samples is caught too.
//!
//! This is what keeps the download list honest: a media download reports
//! **Saved** only after [`check_finished`] returned, so a failed join, a
//! truncated stream or an HTML error page saved under an `.mp4` name is
//! reported as a failure instead.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};

use idm_core::log;

use super::tools::{self, Tool};

/// How much of a file is read when looking for its index. A `moov` or a
/// Matroska `Tracks` element is a few hundred kilobytes even for a long film;
/// this is a ceiling, never a target.
const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;

/// How long the playback check may take. It decodes a few seconds, not the
/// film, so a tool that hangs is stopped.
pub const PLAYBACK_TIMEOUT: Duration = Duration::from_secs(120);

/// How many seconds of the finished file are decoded.
const PLAYBACK_SECONDS: &str = "3";

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
}

impl MediaInfo {
    fn empty(container: MediaContainer, bytes: u64) -> Self {
        Self {
            container,
            duration: None,
            video_codec: None,
            audio_codec: None,
            bytes,
        }
    }
}

/// What the caller expects the finished file to be.
#[derive(Debug, Clone, Copy)]
pub struct Expect<'a> {
    /// The extension the file will be given.
    pub extension: &'a str,
    pub video: bool,
    pub audio: bool,
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
/// not a media container is refused even when `ffmpeg` said it wrote one.
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
        if expect.audio && info.audio_codec.is_none() {
            return Err("the finished file has no sound".to_string());
        }
    }
    if info.duration.is_some_and(|seconds| seconds <= 0.05) {
        return Err("the finished file is no time long".to_string());
    }

    // The index can lie about its samples. Decoding the first seconds is the
    // only way to know the file plays, and it is cheap beside the download.
    let plays = plays(path);
    if info.duration.is_none() && plays.is_ok() {
        // A container with no duration is accepted when the decoder read it:
        // some streams simply do not state one.
        return Ok(info);
    }
    plays?;
    Ok(info)
}

/// Decode the first seconds of a file with `ffmpeg`. `Ok` means a decoder read
/// it without an error; it does not re-encode or write anything.
pub fn plays(path: &Path) -> Result<(), String> {
    let Some(tool) = Tool::Ffmpeg.locate() else {
        // Without the tool there is no playback check. The container check
        // above already passed, so this is not a reason to fail a download.
        return Ok(());
    };
    let mut child = tools::command_for(&tool)
        .args(["-hide_banner", "-nostdin", "-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0", "-t", PLAYBACK_SECONDS, "-f", "null", "-"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("the playback check could not run: {error}"))?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let mut message = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut message);
                }
                let message = last_line(&message);
                log::write(
                    "warn",
                    &format!(
                        "the playback check failed for {}: {message}",
                        path.display()
                    ),
                );
                return Err(match message.is_empty() {
                    true => "the finished file could not be played back".to_string(),
                    false => format!("the finished file could not be played back: {message}"),
                });
            }
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("the playback check failed: {error}"));
            }
        }
        if started.elapsed() > PLAYBACK_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            log::write("warn", "the playback check timed out");
            return Err("the playback check took too long and was stopped".to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn last_line(text: &str) -> String {
    text.lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(200).collect())
        .unwrap_or_default()
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
            read_moov(&index, &mut info);
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

fn read_moov(index: &[u8], info: &mut MediaInfo) {
    if let Some(mvhd) = find(index, &[b"mvhd"]) {
        let body = &index[mvhd.start..mvhd.end];
        info.duration = movie_duration(body);
    }

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
        let kind = &body[8..12];
        let codec = sample_format(media);
        match kind {
            b"vide" => video = codec.or(Some("unknown".to_string())),
            b"soun" => audio = codec.or(Some("unknown".to_string())),
            _ => {}
        }
    }
    info.video_codec = video;
    info.audio_codec = audio;
}

/// The first sample entry's format, e.g. `avc1` or `mp4a`.
fn sample_format(media: &[u8]) -> Option<String> {
    let minf = find(media, &[b"minf"])?;
    let stbl = find(&media[minf.start..minf.end], &[b"stbl"])?;
    let stbl = &media[minf.start..minf.end][stbl.start..stbl.end];
    let stsd = find(stbl, &[b"stsd"])?;
    let body = &stbl[stsd.start..stsd.end];
    if body.len() < 16 {
        return None;
    }
    // version/flags, entry count, then the first entry: size, format, ...
    let format = &body[12..16];
    Some(String::from_utf8_lossy(format).trim().to_string())
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

    if let Some(section) = find_element(&body, &segment, 0x1654AE6B) {
        for track in elements(&body, section.data, section.end) {
            if track.id != 0xAE {
                continue;
            }
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
    Ok(info)
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
            },
        )
        .expect_err("no picture");
        assert!(error.contains("picture"), "{error}");
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
            },
        )
        .expect_err("not a video");
        assert!(error.contains("not a playable video file"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_zero_duration_file_is_refused() {
        // A join test may have pointed `BDM_FFMPEG` at a stand-in.
        let _guard = super::super::tools::test_guard();
        let dir = temp_dir("noduration");
        let path = dir.join("film.mp4");
        std::fs::write(&path, sample_mp4(0)).expect("the file");
        let error = check_finished(
            &path,
            Expect {
                extension: "mp4",
                video: true,
                audio: false,
            },
        )
        .expect_err("no duration");
        assert!(!error.is_empty());
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
}
