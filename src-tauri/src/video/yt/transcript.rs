//! The transcript panel: the captions YouTube's own "Show transcript" shows.
//!
//! The timed-text address a manifest names is throttled on many addresses
//! (HTTP 429), and a helper that asks that same address is throttled with it.
//! The panel YouTube's page opens when the reader asks for the transcript is a
//! different call, answered by a different service: it carries the whole
//! transcript of one video in a single answer, and it needs no token the page
//! itself would have to mint.
//!
//! It is a fallback and not the first choice, because it cannot name a
//! language: the panel shows the video's own transcript, the one YouTube would
//! show. The lines carry the moment they begin and no end, so each one lasts
//! until the next begins.

use serde_json::Value;

use idm_core::media::subtitles::{to_srt, Cue};

/// The panel the transcript view is drawn from.
const PANEL: &str = "PAmodern_transcript_view";

/// How the panel is asked. YouTube accepts a web client of any recent version
/// here, so the version stays written down: a version read from the page would
/// only be one more thing that can be missing.
const CLIENT_VERSION: &str = "2.20250101.00.00";

/// How long the last line stays on screen when nothing follows it.
const LAST_LINE_SECONDS: f64 = 5.0;

/// The video's own transcript, as SubRip text.
///
/// `Ok` with the text is an answer; `Err` is why the panel did not give one.
/// Both are the caller's to report, and neither may cost a reader the video
/// whose bytes are already on disk.
pub(crate) async fn fetch(page_url: &str) -> Result<String, String> {
    let id = super::video_id(page_url).ok_or("the page is not a YouTube watch page")?;
    let client = idm_core::net::client_builder(None, None, None)?
        .build()
        .map_err(|error| format!("the transcript client could not be made: {error}"))?;
    let body = serde_json::json!({
        "context": {
            "client": {
                "clientName": "WEB",
                "clientVersion": CLIENT_VERSION,
                "hl": "en",
                "gl": "US"
            }
        },
        "panelId": PANEL,
        "params": params(&id),
    });
    let response = client
        .post("https://www.youtube.com/youtubei/v1/get_panel?prettyPrint=false")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, idm_core::http::USER_AGENT)
        .header(reqwest::header::ORIGIN, "https://www.youtube.com")
        .header(reqwest::header::REFERER, page_url)
        .body(body.to_string())
        .send()
        .await
        .map_err(|error| format!("the transcript could not be asked for: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(idm_core::http::status_message(status.as_u16()));
    }
    let text = response
        .text()
        .await
        .map_err(|error| format!("the transcript answer could not be read: {error}"))?;
    let answer: Value = serde_json::from_str(&text)
        .map_err(|error| format!("the transcript answer could not be read: {error}"))?;
    let cues = cues(&answer);
    if cues.is_empty() {
        return Err("the transcript came back empty".to_string());
    }
    Ok(to_srt(&cues))
}

/// The `params` of a transcript request: the video id in a protobuf message,
/// written as base64.
///
/// The message is small enough to be written by hand: field 149 holds a
/// message with the id (field 1) and a constant 1 (field 3). Nothing else is
/// in it, which is why no language can be asked for.
fn params(id: &str) -> String {
    let mut inner = Vec::with_capacity(id.len() + 4);
    inner.push(0x0a);
    inner.push(id.len() as u8);
    inner.extend_from_slice(id.as_bytes());
    inner.extend_from_slice(&[0x18, 0x01]);
    let mut outer = Vec::with_capacity(inner.len() + 3);
    outer.extend_from_slice(&[0xaa, 0x09]);
    outer.push(inner.len() as u8);
    outer.extend_from_slice(&inner);
    base64(&outer)
}

/// The cues of a transcript answer.
///
/// Each line begins at its own timestamp and ends where the next line begins.
/// The lines are sorted by their start once more: the panel is drawn in
/// reading order, and a cue file is read in time order.
fn cues(answer: &Value) -> Vec<Cue> {
    let mut segments: Vec<(f64, String)> = Vec::new();
    collect(answer, &mut segments);
    segments.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut cues: Vec<Cue> = Vec::with_capacity(segments.len());
    for (index, (start, text)) in segments.iter().enumerate() {
        let end = segments
            .get(index + 1)
            .map(|next| next.0)
            .filter(|end| end > start)
            .unwrap_or(start + LAST_LINE_SECONDS);
        cues.push(Cue {
            start: *start,
            end,
            text: text.clone(),
        });
    }
    cues
}

/// Walk the answer for the lines of the transcript, wherever it holds them.
fn collect(node: &Value, out: &mut Vec<(f64, String)>) {
    match node {
        Value::Object(map) => {
            if let Some(segment) = map.get("transcriptSegmentViewModel") {
                if let Some(start) = segment
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(timestamp)
                {
                    if let Some(text) = text_of(segment) {
                        out.push((start, text));
                    }
                }
            }
            for value in map.values() {
                collect(value, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect(item, out);
            }
        }
        _ => {}
    }
}

/// The words of one line: the plain text when the line has it, and the runs
/// when it does not.
fn text_of(segment: &Value) -> Option<String> {
    if let Some(text) = segment.get("simpleText").and_then(Value::as_str) {
        let text = text.trim();
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }
    let runs = segment.get("runs")?.as_array()?;
    let text: String = runs
        .iter()
        .filter_map(|run| run.get("text").and_then(Value::as_str))
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// A timestamp as the panel writes it: `0:01` and `1:02:03`.
fn timestamp(text: &str) -> Option<f64> {
    let mut seconds = 0.0;
    let mut parts = 0;
    for part in text.trim().split(':') {
        seconds = seconds * 60.0 + part.trim().replace(',', ".").parse::<f64>().ok()?;
        parts += 1;
    }
    (parts > 0).then_some(seconds)
}

/// The bytes as base64 (RFC 4648, with the padding).
///
/// One request needs one short value encoded, so the alphabet is written here
/// rather than a dependency being added for it.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0] as u32;
        let second = *chunk.get(1).unwrap_or(&0) as u32;
        let third = *chunk.get(2).unwrap_or(&0) as u32;
        let word = first << 16 | second << 8 | third;
        out.push(ALPHABET[(word >> 18) as usize & 63] as char);
        out.push(ALPHABET[(word >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(word >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[word as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_names_the_video_the_way_youtube_expects() {
        // The value YouTube's own page sends for this video.
        assert_eq!(params("nu_pCVPKzTk"), "qgkPCgtudV9wQ1ZQS3pUaxgB");
        assert_eq!(params("jNQXAC9IVRw"), "qgkPCgtqTlFYQUM5SVZSdxgB");
    }

    #[test]
    fn a_timestamp_is_read_in_both_forms_the_panel_writes() {
        assert_eq!(timestamp("0:01"), Some(1.0));
        assert_eq!(timestamp("12:34"), Some(754.0));
        assert_eq!(timestamp("1:02:03"), Some(3723.0));
        assert_eq!(timestamp(""), None);
        assert_eq!(timestamp("soon"), None);
    }

    #[test]
    fn the_lines_become_cues_that_last_until_the_next_one() {
        let answer = serde_json::json!({
            "content": {
                "sectionListRenderer": {
                    "contents": [{
                        "itemSectionRenderer": {
                            "contents": [{
                                "timelineItemViewModel": {
                                    "contentItems": [
                                        { "transcriptSegmentViewModel": { "timestamp": "0:01", "simpleText": "First line" } },
                                        { "transcriptSegmentViewModel": { "timestamp": "0:04", "simpleText": "Second line" } },
                                        { "transcriptSegmentViewModel": { "timestamp": "0:09", "simpleText": "Last line" } }
                                    ]
                                }
                            }]
                        }
                    }]
                }
            }
        });
        let cues = cues(&answer);
        assert_eq!(cues.len(), 3);
        assert_eq!(cues[0].start, 1.0);
        assert_eq!(cues[0].end, 4.0);
        assert_eq!(cues[1].end, 9.0);
        assert_eq!(cues[2].end, 14.0);
        assert_eq!(cues[0].text, "First line");
    }

    #[test]
    fn a_line_written_as_runs_is_read_the_same_way() {
        let answer = serde_json::json!({
            "transcriptSegmentViewModel": {
                "timestamp": "0:02",
                "runs": [{ "text": "A line " }, { "text": "in pieces" }]
            }
        });
        let cues = cues(&answer);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "A line in pieces");
        assert_eq!(cues[0].start, 2.0);
    }

    #[test]
    fn an_answer_without_lines_is_no_transcript() {
        assert!(cues(&serde_json::json!({ "content": {} })).is_empty());
    }

    #[test]
    fn base64_carries_the_bytes_with_its_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
    }
}
