//! Reading a WebM input: its tracks, clusters and blocks.

use super::ebml::*;
use super::*;

/* --------------------------------------------------------------- reading */

pub(super) fn read_input(path: &Path) -> Result<Input, MuxError> {
    let mut file = File::open(path)
        .map_err(|error| MuxError::Io(format!("{} could not be read: {error}", path.display())))?;
    let size = file
        .metadata()
        .map(|meta| meta.len())
        .map_err(|error| MuxError::Io(format!("{} could not be read: {error}", path.display())))?;

    let mut input = Input {
        path: path.to_path_buf(),
        scale: DEFAULT_SCALE,
        ..Input::default()
    };

    // Find the Segment, skipping the EBML header and anything else at the top.
    let mut at = 0u64;
    let mut segment = None;
    while at + 4 <= size {
        let header = read_header(&mut file, at, size)?;
        if header.id == EBML {
            at = header.end;
            continue;
        }
        if header.id == SEGMENT {
            segment = Some(header);
            break;
        }
        if header.end <= at {
            break;
        }
        at = header.end;
    }
    let Some(segment) = segment else {
        return Ok(input);
    };
    let end = if segment.size.is_some() {
        segment.end
    } else {
        size
    };

    let mut cursor = segment.data;
    while cursor < end {
        let header = read_header(&mut file, cursor, end)?;
        match header.id {
            INFO => {
                let body = read_body(&mut file, &header)?;
                input.scale = element_uint(&body, TIMESTAMP_SCALE)
                    .filter(|v| *v > 0)
                    .unwrap_or(DEFAULT_SCALE);
            }
            TRACKS => {
                let body = read_body(&mut file, &header)?;
                input.tracks = parse_tracks(&body);
            }
            CLUSTER => parse_cluster(&mut file, &header, &mut input)?,
            _ => {}
        }
        if header.end <= cursor {
            break;
        }
        cursor = header.end;
    }
    Ok(input)
}

fn parse_tracks(body: &[u8]) -> Vec<RawTrack> {
    let mut tracks = Vec::new();
    for entry in elements(body) {
        if entry.id != TRACK_ENTRY {
            continue;
        }
        let data = &body[entry.data..entry.end];
        let mut track = RawTrack {
            number: element_uint(data, TRACK_NUMBER).unwrap_or(0),
            kind: element_uint(data, TRACK_TYPE).unwrap_or(0),
            codec_id: element_bytes(data, CODEC_ID).unwrap_or_default(),
            codec_private: element_bytes(data, CODEC_PRIVATE).unwrap_or_default(),
            default_duration: element_uint(data, DEFAULT_DURATION),
            codec_delay: element_uint(data, CODEC_DELAY).filter(|value| *value > 0),
            seek_pre_roll: element_uint(data, SEEK_PRE_ROLL).filter(|value| *value > 0),
            ..RawTrack::default()
        };
        if let Some(video) = element_body(data, VIDEO) {
            track.pixel_width = element_uint(video, PIXEL_WIDTH);
            track.pixel_height = element_uint(video, PIXEL_HEIGHT);
        }
        if let Some(audio) = element_body(data, AUDIO) {
            track.sample_rate = element_float(audio, SAMPLING_FREQUENCY);
            track.channels = element_uint(audio, CHANNELS);
        }
        if track.number > 0 {
            tracks.push(track);
        }
    }
    tracks
}

fn parse_cluster(file: &mut File, cluster: &Header, input: &mut Input) -> Result<(), MuxError> {
    let mut cluster_ts = 0u64;
    let mut at = cluster.data;
    while at < cluster.end {
        let header = read_header(file, at, cluster.end)?;
        match header.id {
            CLUSTER_TIMESTAMP => {
                let body = read_body(file, &header)?;
                cluster_ts = read_uint(&body).unwrap_or(0);
            }
            SIMPLE_BLOCK => {
                record_block(file, &header, cluster_ts, input.scale, None, None, input)?
            }
            BLOCK_GROUP => parse_block_group(file, &header, cluster_ts, input)?,
            // A cluster whose size is unknown runs to the end of its parent,
            // so the clusters that follow it arrive here as children. Without
            // this arm every block after the first such cluster is lost.
            CLUSTER => parse_cluster(file, &header, input)?,
            _ => {}
        }
        if header.end <= at {
            break;
        }
        at = header.end;
    }
    Ok(())
}

/// Read a `BlockGroup` into one `Block`.
///
/// A group is the only place a `DiscardPadding` can live, and its `Block`
/// carries no keyframe bit of its own: the absence of a `ReferenceBlock` is
/// what makes the frame a keyframe. Both facts have to survive, so the group's
/// children are read together rather than streamed as they are met.
fn parse_block_group(
    file: &mut File,
    group: &Header,
    cluster_ts: u64,
    input: &mut Input,
) -> Result<(), MuxError> {
    let mut inner = group.data;
    let mut keyframe = true;
    let mut block: Option<Header> = None;
    let mut padding: Option<i64> = None;
    while inner < group.end {
        let child = read_header(file, inner, group.end)?;
        match child.id {
            REFERENCE_BLOCK => keyframe = false,
            BLOCK => block = Some(child.clone()),
            DISCARD_PADDING => {
                let body = read_body(file, &child)?;
                padding = read_int(&body);
            }
            _ => {}
        }
        if child.end <= inner {
            break;
        }
        inner = child.end;
    }
    if let Some(block) = &block {
        record_block(
            file,
            block,
            cluster_ts,
            input.scale,
            Some(keyframe),
            padding,
            input,
        )?;
    }
    Ok(())
}

fn record_block(
    file: &mut File,
    header: &Header,
    cluster_ts: u64,
    scale: u64,
    group_keyframe: Option<bool>,
    discard_padding: Option<i64>,
    input: &mut Input,
) -> Result<(), MuxError> {
    let Some(prefix) = read_prefix(file, header.data, 8, header.end)? else {
        return Ok(());
    };
    let Some((track, vint_len)) = read_vint(&prefix) else {
        return Ok(());
    };
    if prefix.len() < vint_len + 3 {
        return Ok(());
    }
    let relative = i16::from_be_bytes([prefix[vint_len], prefix[vint_len + 1]]) as i64;
    let flags = prefix[vint_len + 2];
    let units = cluster_ts as i64 + relative;
    if units < 0 {
        return Ok(());
    }
    let time_ns = units as u64 * scale;
    let header_len = vint_len as u64 + 3;
    if header.size.unwrap_or(0) < header_len {
        return Ok(());
    }
    let keyframe = group_keyframe.unwrap_or(flags & 0x80 != 0);
    input.blocks.push(Block {
        source_track: track,
        flags,
        keyframe,
        time_ns,
        data_offset: header.data + header_len,
        data_len: header.size.unwrap_or(0) - header_len,
        discard_padding,
    });
    Ok(())
}
