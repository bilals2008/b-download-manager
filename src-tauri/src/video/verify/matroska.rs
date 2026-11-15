//! Reading a Matroska/WebM header: EBML elements, duration, tracks, and whether the segment walks cleanly.

use super::*;

/* ----------------------------------------------------------------- matroska */

/// Read the EBML header, then `Info` and `Tracks` from the segment.
pub(super) fn parse_matroska(file: &mut File, bytes: u64) -> Result<MediaInfo, String> {
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
        info.playback = Playback::Broken("the file ends inside an element it declared".to_string());
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
