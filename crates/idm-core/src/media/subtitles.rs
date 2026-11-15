//! Subtitle tracks: which ones to take, and how to read them.
//!
//! The reference product asks a downloader for the subtitles and converts them
//! with ffmpeg. What is kept here is the part that needs no second program:
//! the two text formats that players and browsers both read (WebVTT and
//! SubRip), the choice of which track to take, and the difference between a
//! site that says "come back later" and a site that says "no".
//!
//! A site that limits how often its subtitles may be fetched (HTTP 429) has
//! not refused the file, it has asked for a pause. That difference decides
//! whether the run asks again, so it lives in the error itself instead of
//! being flattened into a message.

use std::time::Duration;

use reqwest::header::RETRY_AFTER;
use serde::{Deserialize, Serialize};

use super::{Format, Manifest, MediaKind};

/// The caption codec of a container that keeps the text as it is.
///
/// An MPEG-TS file has no caption codec of its own, and a container nobody
/// knows is not a reason to convert the text either.
pub const TEXT_AS_IS: &str = "text";

/// One caption rendition a manifest offered.
///
/// The renditions a manifest names become rows of their own in the quality
/// panel, so the language and the default flag travel with the row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleRendition {
    /// The manifest's own name for the rendition.
    pub id: String,
    /// The name the panel shows.
    pub name: String,
    /// The language the rendition is in, when the manifest says.
    pub language: Option<String>,
    /// Where the bytes are.
    pub url: String,
    /// The format the bytes are in (`vtt`, `srt`, `ttml`).
    pub container: String,
    /// Whether the manifest marks this one as the default.
    pub is_default: bool,
}

impl SubtitleRendition {
    /// The id the quality row carries.
    ///
    /// The row has no language field of its own, so the language travels in
    /// the id and in the label.
    pub fn row_id(&self) -> String {
        let language = self.language.as_deref().unwrap_or("und");
        let mut id = format!("{}-{language}", self.id);
        if self.is_default {
            id.push_str("-default");
        }
        id
    }

    /// The text the quality row shows.
    pub fn row_label(&self) -> String {
        match (&self.language, self.is_default) {
            (Some(language), true) => format!("{} ({language}, default)", self.name),
            (Some(language), false) => format!("{} ({language})", self.name),
            (None, true) => format!("{} (default)", self.name),
            (None, false) => self.name.clone(),
        }
    }

    /// The quality row this rendition becomes.
    pub fn format(&self) -> Format {
        let mut format = Format::new(self.row_id(), MediaKind::Subtitle, &self.container);
        format.label = self.row_label();
        format
    }
}

/// A manifest and the caption renditions it offered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleManifest {
    /// The rows the quality panel shows.
    pub manifest: Manifest,
    /// The caption renditions behind the caption rows.
    pub renditions: Vec<SubtitleRendition>,
}

/// Add the caption rows of these renditions to a manifest.
pub fn add_formats(manifest: &mut Manifest, renditions: &[SubtitleRendition]) {
    for rendition in renditions {
        manifest.formats.push(rendition.format());
    }
}

/// The caption codec a container wants.
///
/// An MP4 wants `mov_text`, which is SubRip's text; a Matroska file wants
/// `S_TEXT/UTF8`, which is WebVTT's text. Any other container keeps the text
/// as it is, so nothing is converted for it.
pub fn container_codec(container: &str) -> &str {
    match container.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
        "mp4" | "m4a" | "m4v" | "m4s" | "mov" => "mov_text",
        "mkv" | "webm" => "S_TEXT/UTF8",
        _ => TEXT_AS_IS,
    }
}

/// The format a caption file must be written in for a container.
///
/// An MP4 wants `mov_text`, which is SubRip's text; a Matroska file wants
/// `S_TEXT/UTF8`, which is WebVTT's text.
pub fn format_for_container(container: &str) -> SubtitleFormat {
    match container.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
        "mp4" | "m4a" | "m4v" | "m4s" | "mov" => SubtitleFormat::Srt,
        _ => SubtitleFormat::Vtt,
    }
}

/// The caption file a rendition needs for a container, written out.
///
/// A WebVTT rendition for an MP4 is written as SubRip, because that is the
/// text `mov_text` holds. A file that holds no cue is left as it is rather
/// than replaced by an empty one.
pub fn render_for_container(text: &str, from: SubtitleFormat, container: &str) -> String {
    let to = format_for_container(container);
    if to == from {
        return text.to_string();
    }
    convert(text, from, to).unwrap_or_else(|_| text.to_string())
}

/// The caption format an address names.
///
/// A file that names no format is read as WebVTT, because that is what a
/// site serves when it has nothing else to offer.
pub fn container_of(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match extension.as_str() {
        "srt" | "subrip" => "srt".to_string(),
        "ttml" | "dfxp" | "xml" => "ttml".to_string(),
        _ => "vtt".to_string(),
    }
}

/// Where a subtitle track came from.
///
/// The uploader's own subtitles were written and checked by a person; the
/// automatic captions are what a speech recogniser made of the sound. Both
/// are worth having, but only one of them is right when they disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubtitleKind {
    /// The uploader's own subtitles.
    Uploader,
    /// The site's automatic captions.
    Automatic,
}

/// The text format a subtitle track is in.
///
/// Only the two formats that need no conversion to be read are named: WebVTT
/// is what a browser serves and SubRip is what every player understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubtitleFormat {
    /// WebVTT, the `.vtt` files.
    Vtt,
    /// SubRip, the `.srt` files.
    Srt,
}

impl SubtitleFormat {
    /// The extension a file of this format is written with.
    pub fn extension(self) -> &'static str {
        match self {
            SubtitleFormat::Vtt => "vtt",
            SubtitleFormat::Srt => "srt",
        }
    }

    /// The format a file name or an address names.
    ///
    /// The query of an address is not part of its name: a signed subtitle
    /// link ends in `?expire=...` and must still be read as the `.vtt` it is.
    pub fn from_extension(name: &str) -> Option<Self> {
        let path = name.split(['?', '#']).next().unwrap_or(name);
        let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match extension.as_str() {
            "vtt" | "webvtt" => Some(SubtitleFormat::Vtt),
            "srt" | "subrip" => Some(SubtitleFormat::Srt),
            _ => None,
        }
    }

    /// The format a body of subtitles is in, from what it says about itself.
    ///
    /// A WebVTT file names itself in its first line. A SubRip file does not,
    /// but it numbers its cues, and that is what tells the two apart.
    pub fn detect(text: &str) -> Option<Self> {
        let head = text.trim_start();
        if head.starts_with("WEBVTT") {
            return Some(SubtitleFormat::Vtt);
        }
        let mut lines = head.lines().filter(|line| !line.trim().is_empty());
        let first = lines.next()?;
        if first.contains("-->") {
            // A cue with no header and no number: a WebVTT file without its
            // first line.
            return Some(SubtitleFormat::Vtt);
        }
        let second = lines.next()?;
        if first.trim().parse::<u64>().is_ok() && second.contains("-->") {
            return Some(SubtitleFormat::Srt);
        }
        None
    }
}

/// One subtitle track the page offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrack {
    /// The site's code for the language (`en`, `pt-BR`).
    pub code: String,
    /// The site's name for the language (`English`).
    pub label: String,
    /// Whether the uploader wrote these or a machine heard them.
    pub kind: SubtitleKind,
    /// Where the bytes are.
    pub url: String,
    /// The format the bytes are in.
    pub format: SubtitleFormat,
}

impl SubtitleTrack {
    /// A track, with the format read from the address.
    ///
    /// An address that names no format is read as WebVTT, because that is
    /// what a site serves when it has nothing else to offer.
    pub fn new(
        code: impl Into<String>,
        label: impl Into<String>,
        kind: SubtitleKind,
        url: impl Into<String>,
    ) -> Self {
        let url = url.into();
        let format = SubtitleFormat::from_extension(&url).unwrap_or(SubtitleFormat::Vtt);
        Self {
            code: code.into(),
            label: label.into(),
            kind,
            url,
            format,
        }
    }

    /// The same track read in another format.
    pub fn with_format(mut self, format: SubtitleFormat) -> Self {
        self.format = format;
        self
    }
}

/// The tracks to take: the uploader's first, then the automatic captions.
///
/// The automatic captions come last because they are the weaker of the two,
/// but they are still offered: a video with no subtitles of its own is
/// better watched with a rough transcript than with none. An automatic track
/// the uploader already gave is left out, though, because the better one is
/// already on the list.
pub fn pick(tracks: &[SubtitleTrack]) -> Vec<&SubtitleTrack> {
    let mut out: Vec<&SubtitleTrack> = tracks
        .iter()
        .filter(|track| track.kind == SubtitleKind::Uploader)
        .collect();
    for track in tracks
        .iter()
        .filter(|track| track.kind == SubtitleKind::Automatic)
    {
        if out.iter().any(|taken| taken.code == track.code) {
            continue;
        }
        out.push(track);
    }
    out
}

/// One line of subtitles and the time it is on screen.
///
/// Both text formats are read into this one shape, so a conversion is a
/// change of words and never a change of timing: a subtitle that moved by a
/// millisecond would sit out of step with the sound.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    /// When the line appears, in seconds from the start of the video.
    pub start: f64,
    /// When it goes away.
    pub end: f64,
    /// The words, with the line breaks kept.
    pub text: String,
}

/// The cues of a WebVTT file.
pub fn parse_vtt(text: &str) -> Vec<Cue> {
    parse_cues(text)
}

/// The cues of a SubRip file.
///
/// The two formats differ in the separator of the timestamps and in the
/// number before each cue, and in nothing else that matters: the same
/// reader reads both.
pub fn parse_srt(text: &str) -> Vec<Cue> {
    parse_cues(text)
}

/// The cues of a file in the given format.
pub fn parse(text: &str, format: SubtitleFormat) -> Vec<Cue> {
    match format {
        SubtitleFormat::Vtt => parse_vtt(text),
        SubtitleFormat::Srt => parse_srt(text),
    }
}

/// The same subtitles written in another format.
///
/// The times are kept to the millisecond, so a converted file still sits
/// exactly where the original one did.
pub fn convert(
    text: &str,
    from: SubtitleFormat,
    to: SubtitleFormat,
) -> Result<String, SubtitleError> {
    let cues = parse(text, from);
    if cues.is_empty() {
        return Err(SubtitleError::Malformed(
            "no subtitle lines were found in the file".to_string(),
        ));
    }
    Ok(match to {
        SubtitleFormat::Vtt => to_vtt(&cues),
        SubtitleFormat::Srt => to_srt(&cues),
    })
}

/// The WebVTT text of these cues.
pub fn to_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for (number, cue) in cues.iter().enumerate() {
        out.push_str(&format!("{}\n", number + 1));
        out.push_str(&format!(
            "{} --> {}\n",
            stamp(cue.start, '.'),
            stamp(cue.end, '.')
        ));
        out.push_str(&cue.text);
        out.push_str("\n\n");
    }
    out
}

/// The SubRip text of these cues.
pub fn to_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (number, cue) in cues.iter().enumerate() {
        out.push_str(&format!("{}\n", number + 1));
        out.push_str(&format!(
            "{} --> {}\n",
            stamp(cue.start, ','),
            stamp(cue.end, ',')
        ));
        out.push_str(&cue.text);
        out.push_str("\n\n");
    }
    out
}

/// Read every cue of a subtitle file.
///
/// The reader walks to the timestamp lines and takes the lines after each of
/// them as the words, so the parts that are not subtitles (the WebVTT
/// header, its notes and styles, the numbers before SubRip cues) are
/// skipped without being read one by one.
fn parse_cues(text: &str) -> Vec<Cue> {
    let lines: Vec<&str> = text.lines().collect();
    let mut cues: Vec<Cue> = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some((start, end)) = parse_timestamp_line(lines[index]) else {
            index += 1;
            continue;
        };
        index += 1;
        let mut words: Vec<&str> = Vec::new();
        while index < lines.len() {
            let line = lines[index];
            if line.trim().is_empty() || parse_timestamp_line(line).is_some() {
                break;
            }
            // The number before the next SubRip cue is not part of this one.
            if is_number_before_cue(&lines, index) {
                break;
            }
            words.push(line);
            index += 1;
        }
        if !words.is_empty() {
            cues.push(Cue {
                start,
                end,
                text: words.join("\n"),
            });
        }
    }
    cues
}

/// The times of a timestamp line, when it is one.
fn parse_timestamp_line(line: &str) -> Option<(f64, f64)> {
    let (left, right) = line.split_once("-->")?;
    let start = parse_timestamp(left.trim())?;
    // A WebVTT cue may carry settings after the times (`align:start`).
    let end = parse_timestamp(right.split_whitespace().next()?)?;
    Some((start, end))
}

/// A timestamp in seconds, when it is one.
///
/// Both formats write `HH:MM:SS` and a fraction of a second; only the
/// character before the fraction differs.
fn parse_timestamp(text: &str) -> Option<f64> {
    let parts: Vec<&str> = text.split(':').collect();
    let (hours, minutes, seconds) = match parts.len() {
        2 => ("0", parts[0], parts[1]),
        3 => (parts[0], parts[1], parts[2]),
        _ => return None,
    };
    let hours: f64 = hours.parse().ok()?;
    let minutes: f64 = minutes.parse().ok()?;
    let seconds: f64 = seconds.replace(',', ".").parse().ok()?;
    if !(0.0..60.0).contains(&minutes) || !(0.0..60.0).contains(&seconds) {
        return None;
    }
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

/// Whether the line at `index` is the number that belongs to the cue after
/// it.
fn is_number_before_cue(lines: &[&str], index: usize) -> bool {
    lines[index].trim().parse::<u64>().is_ok()
        && lines
            .get(index + 1)
            .is_some_and(|next| next.contains("-->"))
}

/// A timestamp written the way a subtitle file writes it.
fn stamp(seconds: f64, separator: char) -> String {
    let total = (seconds.max(0.0) * 1000.0).round() as u64;
    let milliseconds = total % 1000;
    let seconds = total / 1000;
    format!(
        "{:02}:{:02}:{:02}{separator}{:03}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60,
        milliseconds
    )
}

/// The subtitles of one track, read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleFile {
    /// The language code of the track.
    pub code: String,
    /// The site's name for the language.
    pub label: String,
    /// The format the bytes were in.
    pub format: SubtitleFormat,
    /// The cues, in the order they are shown.
    pub cues: Vec<Cue>,
}

impl SubtitleFile {
    /// The name the subtitles are saved under beside the video.
    ///
    /// The language is part of the name because a video may have several of
    /// them side by side.
    pub fn file_name(&self, stem: &str) -> String {
        format!("{stem}.{}.{}", self.code, self.format.extension())
    }

    /// The same subtitles written in another format.
    pub fn render(&self, format: SubtitleFormat) -> String {
        match format {
            SubtitleFormat::Vtt => to_vtt(&self.cues),
            SubtitleFormat::Srt => to_srt(&self.cues),
        }
    }
}

/// Why a subtitle could not be read.
///
/// The reasons are kept apart because the caller acts on them differently: a
/// site that limits its requests wants the run to come back later, while a
/// site that says "not found" will say it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubtitleError {
    /// The site limits how often the subtitles may be fetched (HTTP 429).
    ///
    /// The file exists and will still be there after a pause, so this is a
    /// reason to wait and ask again, not a refusal.
    RateLimited {
        /// How long the site asked the caller to wait, when it said.
        retry_after: Option<Duration>,
    },
    /// The server refused the request for another reason.
    Refused {
        /// The status the server answered with.
        status: u16,
        /// What the server said, in the words of its status.
        reason: String,
    },
    /// The request never reached the server.
    Network(String),
    /// What came back is not subtitles.
    Malformed(String),
}

impl SubtitleError {
    /// Whether asking again can help.
    ///
    /// A site that answers "too many requests" or a network that blinks are
    /// both worth a second try; a file that is not subtitles never is.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            SubtitleError::RateLimited { .. } | SubtitleError::Network(_)
        )
    }
}

impl std::fmt::Display for SubtitleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubtitleError::RateLimited { retry_after } => match retry_after {
                Some(wait) => write!(
                    f,
                    "the site is limiting how often these subtitles may be fetched; it asks to wait {} s",
                    wait.as_secs()
                ),
                None => write!(
                    f,
                    "the site is limiting how often these subtitles may be fetched"
                ),
            },
            SubtitleError::Refused { status, reason } => {
                if reason.is_empty() {
                    write!(f, "the site refused the subtitles (HTTP {status})")
                } else {
                    write!(f, "the site refused the subtitles (HTTP {status}: {reason})")
                }
            }
            SubtitleError::Network(why) => write!(f, "the subtitles could not be reached: {why}"),
            SubtitleError::Malformed(why) => write!(f, "the subtitles could not be read: {why}"),
        }
    }
}

impl std::error::Error for SubtitleError {}

/// The error a status the server answered with means.
///
/// The status is turned into the error before anything else happens, so the
/// question "may we ask again?" has one answer in one place.
fn error_for_status(status: u16, retry_after: Option<Duration>) -> SubtitleError {
    if status == 429 {
        return SubtitleError::RateLimited { retry_after };
    }
    SubtitleError::Refused {
        status,
        reason: String::new(),
    }
}

/// How long the server asked the caller to wait, when it said.
///
/// Only the number of seconds is read: the HTTP date form asks for more
/// knowledge of the clock than a caller of this module should need.
fn parse_retry_after(value: &str) -> Option<Duration> {
    let seconds: u64 = value.trim().parse().ok()?;
    Some(Duration::from_secs(seconds))
}

/// Fetch one track's subtitles.
pub async fn fetch(
    client: &reqwest::Client,
    track: &SubtitleTrack,
) -> Result<SubtitleFile, SubtitleError> {
    let response = client
        .get(&track.url)
        .send()
        .await
        .map_err(|why| SubtitleError::Network(why.to_string()))?;

    let status = response.status();
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_retry_after);

    if !status.is_success() {
        let mut error = error_for_status(status.as_u16(), retry_after);
        if let SubtitleError::Refused { reason, .. } = &mut error {
            *reason = status
                .canonical_reason()
                .unwrap_or("the request was refused")
                .to_string();
        }
        return Err(error);
    }

    let body = response
        .text()
        .await
        .map_err(|why| SubtitleError::Network(why.to_string()))?;

    let cues = parse(&body, track.format);
    if cues.is_empty() {
        return Err(SubtitleError::Malformed(
            "the file held no subtitle lines".to_string(),
        ));
    }

    Ok(SubtitleFile {
        code: track.code.clone(),
        label: track.label.clone(),
        format: track.format,
        cues,
    })
}

/// What one track's fetch ended with.
#[derive(Debug, Clone, PartialEq)]
pub enum SubtitleOutcome {
    /// The subtitles came.
    Fetched(SubtitleFile),
    /// They did not, and the reason says whether asking again can help.
    Refused {
        /// The language code of the track.
        code: String,
        /// The site's name for the language.
        label: String,
        /// Why they did not come.
        error: SubtitleError,
    },
}

impl SubtitleOutcome {
    /// The reason of a refusal, when this outcome is one.
    pub fn error(&self) -> Option<&SubtitleError> {
        match self {
            SubtitleOutcome::Fetched(_) => None,
            SubtitleOutcome::Refused { error, .. } => Some(error),
        }
    }
}

/// A run that fetches nothing but the subtitles.
///
/// The video is not touched: a subtitle file that arrives later can only sit
/// beside the video, never inside it. Every track is asked for on its own,
/// so one refused language does not cost the others.
pub async fn run(client: &reqwest::Client, tracks: &[SubtitleTrack]) -> Vec<SubtitleOutcome> {
    let mut out = Vec::with_capacity(tracks.len());
    for track in tracks {
        let outcome = match fetch(client, track).await {
            Ok(file) => SubtitleOutcome::Fetched(file),
            Err(error) => SubtitleOutcome::Refused {
                code: track.code.clone(),
                label: track.label.clone(),
                error,
            },
        };
        out.push(outcome);
    }
    out
}

/// Ask again for the tracks the site limited.
///
/// A site that answers "too many requests" wants a pause, not a quick
/// second try, so how long to wait is the caller's to decide: the run holds
/// on for `pause` and then asks once more, and only for these tracks.
pub async fn retry_limited(
    client: &reqwest::Client,
    tracks: &[SubtitleTrack],
    pause: Duration,
) -> Vec<SubtitleOutcome> {
    if tracks.is_empty() {
        return Vec::new();
    }
    tokio::time::sleep(pause).await;
    run(client, tracks).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const VTT: &str = "WEBVTT\n\
Kind: captions\n\
Language: en\n\
\n\
NOTE this note is not a subtitle\n\
\n\
1\n\
00:00:01.000 --> 00:00:04.000 align:start position:10%\n\
The first line\n\
and the second\n\
\n\
2\n\
00:01:02,500 --> 00:01:05.000\n\
The third line\n";

    const SRT: &str = "1\n\
00:00:01,000 --> 00:00:04,000\n\
The first line\n\
and the second\n\
\n\
2\n\
00:01:02,500 --> 00:01:05,000\n\
The third line\n\
\n";

    fn track(code: &str, kind: SubtitleKind) -> SubtitleTrack {
        SubtitleTrack::new(
            code,
            format!("Name of {code}"),
            kind,
            format!("https://example.org/{code}.vtt"),
        )
    }

    #[test]
    fn a_webvtt_file_becomes_cues() {
        let cues = parse_vtt(VTT);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start, 1.0);
        assert_eq!(cues[0].end, 4.0);
        assert_eq!(cues[0].text, "The first line\nand the second");
        assert_eq!(cues[1].start, 62.5);
        assert_eq!(cues[1].end, 65.0);
    }

    #[test]
    fn a_subrip_file_becomes_cues() {
        let cues = parse_srt(SRT);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start, 1.0);
        assert_eq!(cues[0].text, "The first line\nand the second");
        assert_eq!(cues[1].start, 62.5);
        assert_eq!(cues[1].text, "The third line");
    }

    #[test]
    fn the_two_formats_read_the_same_file_the_same_way() {
        assert_eq!(parse_vtt(SRT), parse_srt(VTT));
    }

    #[test]
    fn a_timestamp_without_hours_is_read() {
        let cues = parse_srt("1\n01:02,250 --> 01:03,000\nShort\n");
        assert_eq!(cues[0].start, 62.25);
        assert_eq!(cues[0].end, 63.0);
    }

    #[test]
    fn the_cues_survive_a_round_trip() {
        let srt = convert(SRT, SubtitleFormat::Srt, SubtitleFormat::Vtt).expect("srt to vtt");
        assert!(srt.starts_with("WEBVTT"));
        assert_eq!(parse_vtt(&srt), parse_srt(SRT));

        let back = convert(&srt, SubtitleFormat::Vtt, SubtitleFormat::Srt).expect("vtt to srt");
        assert_eq!(back, SRT);
    }

    #[test]
    fn a_file_that_is_not_subtitles_says_so() {
        let error = convert(
            "not subtitles at all",
            SubtitleFormat::Vtt,
            SubtitleFormat::Srt,
        )
        .expect_err("no cues");
        assert!(matches!(error, SubtitleError::Malformed(_)));
    }

    #[test]
    fn the_uploader_comes_before_the_automatic_captions() {
        let tracks = vec![
            track("en", SubtitleKind::Automatic),
            track("de", SubtitleKind::Uploader),
            track("es", SubtitleKind::Automatic),
        ];
        let picked: Vec<&str> = pick(&tracks)
            .into_iter()
            .map(|track| track.code.as_str())
            .collect();
        assert_eq!(picked, vec!["de", "en", "es"]);
    }

    #[test]
    fn an_automatic_track_the_uploader_already_gave_is_not_offered_twice() {
        let tracks = vec![
            track("en", SubtitleKind::Uploader),
            track("en", SubtitleKind::Automatic),
            track("es", SubtitleKind::Automatic),
        ];
        let picked: Vec<&str> = pick(&tracks)
            .into_iter()
            .map(|track| track.code.as_str())
            .collect();
        assert_eq!(picked, vec!["en", "es"]);
    }

    #[test]
    fn a_limited_site_is_a_problem_to_come_back_to() {
        let error = error_for_status(429, Some(Duration::from_secs(20)));
        assert!(matches!(error, SubtitleError::RateLimited { .. }));
        assert!(error.is_retryable());
        assert!(error.to_string().contains("limiting"));
    }

    #[test]
    fn a_refusal_that_will_stay_a_refusal_is_not_worth_a_retry() {
        let error = error_for_status(404, None);
        assert!(!error.is_retryable());
        assert!(error.to_string().contains("404"));
    }

    #[test]
    fn the_wait_the_site_asked_for_is_read() {
        assert_eq!(parse_retry_after("20"), Some(Duration::from_secs(20)));
        assert_eq!(parse_retry_after(" Wed, 21 Oct 2015 07:28:00 GMT"), None);
    }

    #[test]
    fn the_format_is_read_from_the_address() {
        assert_eq!(
            SubtitleFormat::from_extension("https://x.example/a.srt?sig=1"),
            Some(SubtitleFormat::Srt)
        );
        assert_eq!(
            SubtitleFormat::from_extension("https://x.example/a.vtt"),
            Some(SubtitleFormat::Vtt)
        );
        assert_eq!(SubtitleFormat::from_extension("https://x.example/a"), None);
    }

    #[test]
    fn a_format_is_told_from_its_own_text() {
        assert_eq!(SubtitleFormat::detect(VTT), Some(SubtitleFormat::Vtt));
        assert_eq!(SubtitleFormat::detect(SRT), Some(SubtitleFormat::Srt));
        assert_eq!(SubtitleFormat::detect("nothing here"), None);
    }

    #[test]
    fn the_subtitles_are_saved_beside_the_video_under_their_language() {
        let file = SubtitleFile {
            code: "pt-BR".to_string(),
            label: "Portuguese".to_string(),
            format: SubtitleFormat::Srt,
            cues: parse_srt(SRT),
        };
        assert_eq!(file.file_name("movie"), "movie.pt-BR.srt");
    }

    #[test]
    fn every_container_names_the_caption_codec_it_wants() {
        assert_eq!(container_codec("mp4"), "mov_text");
        assert_eq!(container_codec("m4a"), "mov_text");
        assert_eq!(container_codec("mkv"), "S_TEXT/UTF8");
        assert_eq!(container_codec("webm"), "S_TEXT/UTF8");
        assert_eq!(container_codec("ts"), TEXT_AS_IS);
        assert_eq!(
            container_codec("something-nobody-knows"),
            TEXT_AS_IS,
            "an unknown container keeps the text as it is"
        );
    }

    #[test]
    fn a_webvtt_rendition_is_written_as_subrip_for_an_mp4() {
        let rendered = render_for_container(VTT, SubtitleFormat::Vtt, "mp4");
        assert!(!rendered.starts_with("WEBVTT"));
        assert_eq!(parse_srt(&rendered), parse_vtt(VTT));
        assert_eq!(container_codec("mp4"), "mov_text");
    }

    #[test]
    fn a_manifest_caption_row_says_its_language_and_its_default() {
        let rendition = SubtitleRendition {
            id: "s0".to_string(),
            name: "English".to_string(),
            language: Some("en".to_string()),
            url: "https://cdn.example.org/subs/en.vtt".to_string(),
            container: "vtt".to_string(),
            is_default: true,
        };
        assert_eq!(rendition.row_id(), "s0-en-default");
        assert_eq!(rendition.row_label(), "English (en, default)");

        let format = rendition.format();
        assert_eq!(format.kind, MediaKind::Subtitle);
        assert_eq!(format.id, "s0-en-default");
        assert_eq!(format.container, "vtt");
    }

    #[test]
    fn the_caption_rows_are_added_to_the_manifest() {
        let mut manifest = Manifest {
            title: String::new(),
            duration: None,
            formats: Vec::new(),
            pairs: Vec::new(),
            container_hint: Some("hls".to_string()),
            subtitles: Vec::new(),
        };
        add_formats(
            &mut manifest,
            &[SubtitleRendition {
                id: "s0".to_string(),
                name: "English".to_string(),
                language: Some("en".to_string()),
                url: "https://cdn.example.org/subs/en.vtt".to_string(),
                container: "vtt".to_string(),
                is_default: false,
            }],
        );
        assert_eq!(manifest.formats.len(), 1);
        assert_eq!(manifest.formats[0].kind, MediaKind::Subtitle);
    }

    #[test]
    fn the_caption_format_is_read_from_the_address() {
        assert_eq!(container_of("https://x.example/a.vtt"), "vtt");
        assert_eq!(container_of("https://x.example/a.srt"), "srt");
        assert_eq!(container_of("https://x.example/a.ttml"), "ttml");
        assert_eq!(container_of("https://x.example/a"), "vtt");
    }
}
