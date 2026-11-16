//! The cues of a subtitle file: both text formats read into one shape and
//! written back out.
//!
//! A conversion is a change of words and never a change of timing: a
//! subtitle that moved by a millisecond would sit out of step with the
//! sound.

use serde::{Deserialize, Serialize};

use super::{SubtitleError, SubtitleFormat};

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
