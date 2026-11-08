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

/* --------------------------------------------------------------- reading */

fn read_input(path: &Path) -> Result<Input, MuxError> {
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
            SIMPLE_BLOCK => record_block(
                file,
                &header,
                cluster_ts,
                input.scale,
                None,
                None,
                input,
            )?,
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

/* ---------------------------------------------------------------- writing */

fn write_joined(
    inputs: &[Input],
    output: &Path,
    work_dir: &Path,
    container: Container,
) -> Result<(), MuxError> {
    let temporary = temporary_path(work_dir, output);
    let mut file = File::create(&temporary)
        .map_err(|error| MuxError::Io(format!("the file could not be written: {error}")))?;

    let mut tracks: Vec<RawTrack> = Vec::new();
    let mut blocks = Vec::new();
    for input in inputs {
        for track in &input.tracks {
            let order = tracks.len();
            tracks.push(track.clone());
            for block in input
                .blocks
                .iter()
                .filter(|block| block.source_track == track.number)
            {
                blocks.push(OutputBlock {
                    path: input.path.clone(),
                    out_number: 0, // numbered once the tracks are known
                    order,
                    block: block.clone(),
                });
            }
        }
    }
    if tracks.is_empty() || blocks.is_empty() {
        return Err(MuxError::Io("the tracks have no frames".to_string()));
    }
    blocks.sort_by_key(|block| block.block.time_ns);

    // A track that contributed no frame would still be declared in `Tracks`,
    // and a file that promises a stream it never delivers is refused by some
    // players. Drop it, and number what is left from one.
    let mut used = vec![false; tracks.len()];
    for block in &blocks {
        used[block.order] = true;
    }
    let mut renumber = vec![0usize; tracks.len()];
    let mut kept: Vec<RawTrack> = Vec::with_capacity(tracks.len());
    for (index, track) in tracks.into_iter().enumerate() {
        if !used[index] {
            continue;
        }
        renumber[index] = kept.len();
        kept.push(track);
    }
    for block in &mut blocks {
        block.order = renumber[block.order];
        block.out_number = block.order as u64 + 1;
    }
    let tracks = kept;

    // The file ends when its last frame ends, not when that frame starts; a
    // track's own frame length is the only statement of that the file carries.
    let duration_ns = blocks
        .iter()
        .map(|block| {
            let frame = tracks
                .get(block.order)
                .and_then(|track| track.default_duration)
                .unwrap_or(0);
            block.block.time_ns.saturating_add(frame)
        })
        .max()
        .unwrap_or(0);

    let doc_type = if container == Container::Mkv {
        "matroska"
    } else {
        "webm"
    };
    let ebml = ebml_header(doc_type);
    file.write_all(&ebml).map_err(write_error)?;
    // A Segment of unknown size is the normal shape for a streamed file. Every
    // cue position is measured from the first byte after its size field, so the
    // header's own length has to be known here.
    let segment_header = [id(SEGMENT), unknown_size()].concat();
    file.write_all(&segment_header).map_err(write_error)?;
    let segment_data = ebml.len() as u64 + segment_header.len() as u64;
    file.write_all(&info(duration_ns)).map_err(write_error)?;
    file.write_all(&tracks_element(&tracks))
        .map_err(write_error)?;

    let points = write_clusters(&mut file, &blocks, segment_data)?;
    // Without cues a player has to scan every cluster to find a keyframe, which
    // is why a WebM that has them scrubs and one that does not stutters.
    if !points.is_empty() {
        file.write_all(&cues(&points)).map_err(write_error)?;
    }

    file.flush().map_err(write_error)?;
    drop(file);
    idm_core::disk::replace_file(&temporary, output)
        .map_err(|error| MuxError::Io(error.to_string()))
}

#[derive(Clone)]
struct OutputBlock {
    path: PathBuf,
    out_number: u64,
    order: usize,
    block: Block,
}

fn write_clusters(
    file: &mut File,
    blocks: &[OutputBlock],
    segment_data: u64,
) -> Result<Vec<CuePoint>, MuxError> {
    let mut readers: Vec<Option<File>> = (0..count_tracks(blocks)).map(|_| None).collect();
    let mut points = Vec::new();
    let mut index = 0usize;
    while index < blocks.len() {
        let cluster_ms = blocks[index].block.time_ns / DEFAULT_SCALE;
        let mut cluster_blocks = Vec::new();
        while index < blocks.len() {
            let block_ms = blocks[index].block.time_ns / DEFAULT_SCALE;
            let relative = block_ms as i64 - cluster_ms as i64;
            if !cluster_blocks.is_empty() && relative > MAX_RELATIVE {
                break;
            }
            if !cluster_blocks.is_empty() && block_ms.saturating_sub(cluster_ms) > CLUSTER_SPAN_MS {
                break;
            }
            cluster_blocks.push((relative, blocks[index].clone()));
            index += 1;
        }

        let mut body = Vec::new();
        body.extend(uint_element(CLUSTER_TIMESTAMP, cluster_ms));
        for (relative, block) in &cluster_blocks {
            let entry = read_block_frames(&mut readers, block)?;
            body.extend(output_block(block, *relative, &entry));
        }
        let cluster = element(CLUSTER, &body);
        // Only a cluster that opens on a keyframe is somewhere a seek can land.
        if let Some((_, first)) = cluster_blocks.first() {
            if first.block.keyframe {
                let at = file.stream_position().map_err(write_error)?;
                points.push(CuePoint {
                    time_ms: cluster_ms,
                    track: first.out_number,
                    cluster: at.saturating_sub(segment_data),
                });
            }
        }
        file.write_all(&cluster).map_err(write_error)?;
    }
    Ok(points)
}

/// Where a cue point sends the player: the cluster's offset inside the Segment,
/// and the time of the keyframe it starts with.
struct CuePoint {
    time_ms: u64,
    track: u64,
    cluster: u64,
}

fn cues(points: &[CuePoint]) -> Vec<u8> {
    let mut body = Vec::new();
    for point in points {
        let mut positions = Vec::new();
        positions.extend(uint_element(CUE_TRACK, point.track));
        positions.extend(uint_element(CUE_CLUSTER_POSITION, point.cluster));
        let mut entry = Vec::new();
        entry.extend(uint_element(CUE_TIME, point.time_ms));
        entry.extend(element(CUE_TRACK_POSITIONS, &positions));
        body.extend(element(CUE_POINT, &entry));
    }
    element(CUES, &body)
}

fn count_tracks(blocks: &[OutputBlock]) -> usize {
    blocks.iter().map(|block| block.order).max().unwrap_or(0) + 1
}

fn read_block_frames(
    readers: &mut [Option<File>],
    block: &OutputBlock,
) -> Result<Vec<u8>, MuxError> {
    let slot = &mut readers[block.order];
    let reader = match slot {
        Some(reader) => reader,
        None => {
            let file = File::open(&block.path)
                .map_err(|error| MuxError::Io(format!("a track could not be read: {error}")))?;
            slot.insert(file)
        }
    };
    reader
        .seek(SeekFrom::Start(block.block.data_offset))
        .map_err(|error| MuxError::Io(format!("a frame could not be read: {error}")))?;
    let mut frames = vec![0u8; block.block.data_len as usize];
    reader
        .read_exact(&mut frames)
        .map_err(|error| MuxError::Io(format!("a frame could not be read: {error}")))?;
    Ok(frames)
}

/// One output block: a `SimpleBlock`, or a `BlockGroup` when the block has
/// padding to carry. A group is the only element that can state padding, so the
/// block keeps its group rather than losing the tail of its last frame.
fn output_block(block: &OutputBlock, relative: i64, frames: &[u8]) -> Vec<u8> {
    if block.block.discard_padding.is_some() {
        let mut group = Vec::new();
        group.extend(element(BLOCK, &block_payload(block, relative, frames, false)));
        if !block.block.keyframe {
            // Inside a group the keyframe bit is not used; a reference to the
            // frame before is what says this one is not a keyframe.
            group.extend(uint_element(REFERENCE_BLOCK, 0));
        }
        if let Some(padding) = block.block.discard_padding {
            group.extend(element(DISCARD_PADDING, &padding.to_be_bytes()));
        }
        return element(BLOCK_GROUP, &group);
    }
    element(SIMPLE_BLOCK, &block_payload(block, relative, frames, true))
}

fn block_payload(block: &OutputBlock, relative: i64, frames: &[u8], simple: bool) -> Vec<u8> {
    let mut payload = Vec::with_capacity(frames.len() + 4);
    payload.extend(encode_vint(block.out_number));
    payload.extend(&(relative as i16).to_be_bytes());
    // A `SimpleBlock` states whether it is a keyframe; a plain `Block` must not
    // set that bit, because it means something else there.
    let flags = if simple {
        (block.block.flags & 0x7F) | if block.block.keyframe { 0x80 } else { 0 }
    } else {
        block.block.flags & 0x0F
    };
    payload.push(flags);
    payload.extend(frames);
    payload
}

/* -------------------------------------------------------------- EBML write */

fn ebml_header(doc_type: &str) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(string_element(DOC_TYPE, doc_type.as_bytes()));
    body.extend(uint_element(DOC_TYPE_VERSION, 4));
    body.extend(uint_element(DOC_TYPE_READ_VERSION, 2));
    element(EBML, &body)
}

fn info(duration_ns: u64) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(uint_element(TIMESTAMP_SCALE, DEFAULT_SCALE));
    let duration_ms = (duration_ns as f64 / DEFAULT_SCALE as f64).max(1.0);
    body.extend(element(DURATION, &duration_ms.to_be_bytes()));
    body.extend(string_element(MUXING_APP, b"B Download Manager"));
    body.extend(string_element(WRITING_APP, b"B Download Manager"));
    element(INFO, &body)
}

fn tracks_element(tracks: &[RawTrack]) -> Vec<u8> {
    let mut body = Vec::new();
    for (index, track) in tracks.iter().enumerate() {
        body.extend(track_entry(index as u64 + 1, track));
    }
    element(TRACKS, &body)
}

fn track_entry(number: u64, track: &RawTrack) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(uint_element(TRACK_NUMBER, number));
    body.extend(uint_element(TRACK_UID, number));
    body.extend(uint_element(TRACK_TYPE, track.kind));
    body.extend(string_element(CODEC_ID, &track.codec_id));
    if !track.codec_private.is_empty() {
        body.extend(element(CODEC_PRIVATE, &track.codec_private));
    }
    if let Some(duration) = track.default_duration {
        body.extend(uint_element(DEFAULT_DURATION, duration));
    }
    // The decoder's own start offset: without it the sound of an Opus track is
    // ahead of the picture by the length of its pre-skip.
    if let Some(delay) = track.codec_delay {
        body.extend(uint_element(CODEC_DELAY, delay));
    }
    if let Some(pre_roll) = track.seek_pre_roll {
        body.extend(uint_element(SEEK_PRE_ROLL, pre_roll));
    }
    if track.kind == 1 {
        let mut video = Vec::new();
        if let Some(width) = track.pixel_width {
            video.extend(uint_element(PIXEL_WIDTH, width));
        }
        if let Some(height) = track.pixel_height {
            video.extend(uint_element(PIXEL_HEIGHT, height));
        }
        body.extend(element(VIDEO, &video));
    } else if track.kind == 2 {
        let mut audio = Vec::new();
        if let Some(rate) = track.sample_rate {
            audio.extend(element(SAMPLING_FREQUENCY, &rate.to_be_bytes()));
        }
        if let Some(channels) = track.channels {
            audio.extend(uint_element(CHANNELS, channels));
        }
        body.extend(element(AUDIO, &audio));
    }
    element(TRACK_ENTRY, &body)
}

fn element(id_bytes: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 9);
    out.extend(id(id_bytes));
    out.extend(encode_size(payload.len() as u64));
    out.extend(payload);
    out
}

fn uint_element(id_bytes: u64, value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let first = bytes.iter().position(|byte| *byte != 0).unwrap_or(7);
    element(id_bytes, &bytes[first..])
}

fn string_element(id_bytes: u64, value: &[u8]) -> Vec<u8> {
    element(id_bytes, value)
}

fn id(value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let first = bytes.iter().position(|byte| *byte != 0).unwrap_or(7);
    bytes[first..].to_vec()
}

fn unknown_size() -> Vec<u8> {
    vec![0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
}

fn encode_size(value: u64) -> Vec<u8> {
    for length in 1..=8u32 {
        let limit = 1u64 << (7 * length);
        if value < limit - 1 {
            let marked = value | limit;
            let bytes = marked.to_be_bytes();
            return bytes[(8 - length) as usize..].to_vec();
        }
    }
    let marked = value | (1u64 << 56);
    marked.to_be_bytes()[1..].to_vec()
}

fn encode_vint(value: u64) -> Vec<u8> {
    // A track number is a VINT of at most four bytes.
    for length in 1..=4u32 {
        let limit = 1u64 << (7 * length);
        if value < limit {
            let marked = value | limit;
            let bytes = marked.to_be_bytes();
            return bytes[(8 - length) as usize..].to_vec();
        }
    }
    let bytes = (value | (1u64 << 28)).to_be_bytes();
    bytes[4..].to_vec()
}

/* --------------------------------------------------------------- EBML read */

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

fn read_header(file: &mut File, at: u64, end: u64) -> Result<Header, MuxError> {
    if at + 2 > end {
        return Err(MuxError::Io("the file is truncated".to_string()));
    }
    let mut prefix = [0u8; 12];
    let read = read_prefix(file, at, 12, end)?.unwrap_or_default();
    let len = read.len();
    prefix[..len].copy_from_slice(&read);
    let Some((id_value, id_len)) = read_id(&prefix) else {
        return Err(MuxError::Io("the file has a bad element".to_string()));
    };
    let Some((size_value, size_len)) = read_size(&prefix[id_len..]) else {
        return Err(MuxError::Io("the file has a bad element size".to_string()));
    };
    let size = size_value.map(|value| value as u64);
    let data = at + id_len as u64 + size_len as u64;
    let end_of_box = match size {
        Some(value) => data.saturating_add(value),
        None => end,
    };
    if end_of_box < data || end_of_box > end {
        return Err(MuxError::Io("the file is truncated".to_string()));
    }
    Ok(Header {
        id: id_value,
        data,
        size,
        end: end_of_box,
    })
}

fn read_body(file: &mut File, header: &Header) -> Result<Vec<u8>, MuxError> {
    let Some(size) = header.size else {
        return Err(MuxError::Io("an element has no size".to_string()));
    };
    if size > 64 * 1024 * 1024 {
        return Err(MuxError::Io("an index is too large to read".to_string()));
    }
    let mut body = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(header.data))
        .map_err(|error| MuxError::Io(error.to_string()))?;
    file.read_exact(&mut body)
        .map_err(|error| MuxError::Io(error.to_string()))?;
    Ok(body)
}

fn read_prefix(file: &mut File, at: u64, len: u64, end: u64) -> Result<Option<Vec<u8>>, MuxError> {
    let available = end.saturating_sub(at).min(len);
    if available == 0 {
        return Ok(None);
    }
    let mut buffer = vec![0u8; available as usize];
    file.seek(SeekFrom::Start(at))
        .map_err(|error| MuxError::Io(error.to_string()))?;
    let read = file
        .read(&mut buffer)
        .map_err(|error| MuxError::Io(error.to_string()))?;
    buffer.truncate(read);
    Ok(Some(buffer))
}

fn elements(bytes: &[u8]) -> Vec<ElementRange> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 2 <= bytes.len() {
        let Some((id_value, id_len)) = read_id(&bytes[at..]) else {
            break;
        };
        let Some((size, size_len)) = read_size(&bytes[at + id_len..]) else {
            break;
        };
        let data = at + id_len + size_len;
        let end = match size {
            Some(size) => data.saturating_add(size),
            None => bytes.len(),
        };
        if end > bytes.len() || end < data {
            break;
        }
        out.push(ElementRange {
            id: id_value,
            data,
            end,
        });
        at = end;
    }
    out
}

fn element_body(bytes: &[u8], wanted: u64) -> Option<&[u8]> {
    let atom = elements(bytes).into_iter().find(|atom| atom.id == wanted)?;
    Some(&bytes[atom.data..atom.end])
}

fn element_bytes(bytes: &[u8], wanted: u64) -> Option<Vec<u8>> {
    element_body(bytes, wanted).map(|body| body.to_vec())
}

fn element_uint(bytes: &[u8], wanted: u64) -> Option<u64> {
    let body = element_body(bytes, wanted)?;
    read_uint(body)
}

fn element_float(bytes: &[u8], wanted: u64) -> Option<f64> {
    let body = element_body(bytes, wanted)?;
    match body.len() {
        4 => Some(f32::from_be_bytes([body[0], body[1], body[2], body[3]]) as f64),
        8 => Some(f64::from_be_bytes([
            body[0], body[1], body[2], body[3], body[4], body[5], body[6], body[7],
        ])),
        _ => None,
    }
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

/// A Matroska *signed* integer, which is sign-extended from its top bit.
fn read_int(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() || bytes.len() > 8 {
        return None;
    }
    let mut value = if bytes[0] & 0x80 != 0 { -1i64 } else { 0i64 };
    for byte in bytes {
        value = (value << 8) | *byte as i64;
    }
    Some(value)
}

fn read_id(bytes: &[u8]) -> Option<(u64, usize)> {
    let first = *bytes.first()?;
    let length = first.leading_zeros() as usize + 1;
    if length > 4 || bytes.len() < length {
        return None;
    }
    let value = bytes[..length]
        .iter()
        .fold(0u64, |value, byte| (value << 8) | *byte as u64);
    Some((value, length))
}

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

/// A track number's VINT, marker removed.
fn read_vint(bytes: &[u8]) -> Option<(u64, usize)> {
    let first = *bytes.first()?;
    let length = first.leading_zeros() as usize + 1;
    if length > 8 || bytes.len() < length {
        return None;
    }
    let mut value = (first as u64) & (0xFF >> length);
    for byte in &bytes[1..length] {
        value = (value << 8) | *byte as u64;
    }
    Some((value, length))
}

fn write_error(error: std::io::Error) -> MuxError {
    MuxError::Io(format!("the file could not be written: {error}"))
}

fn temporary_path(work_dir: &Path, output: &Path) -> PathBuf {
    let name = output
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    work_dir.join(format!(".{name}.idmmux"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn ffmpeg() -> Option<String> {
        for name in ["ffmpeg", "ffmpeg.exe"] {
            if Command::new(name)
                .arg("-version")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
            {
                return Some(name.to_string());
            }
        }
        None
    }

    fn folder(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bdm-webm-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        dir
    }

    fn make(ffmpeg: &str, args: &[&str], out: &Path) -> bool {
        Command::new(ffmpeg)
            .args(args)
            .arg(out)
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn decodes(ffmpeg: &str, path: &Path) {
        let status = Command::new(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-i"])
            .arg(path)
            .args(["-f", "null", "-"])
            .status()
            .expect("ffmpeg runs");
        assert!(status.success(), "the joined file must decode");
    }

    #[test]
    fn joins_a_vp9_picture_and_an_opus_sound() {
        let Some(ffmpeg) = ffmpeg() else { return };
        let dir = folder("vp9");
        let video = dir.join("video.track");
        let audio = dir.join("audio.track");
        // A build without libvpx/libopus skips this test rather than failing.
        let made_video = make(
            &ffmpeg,
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=30:duration=1",
                "-an",
                "-c:v",
                "libvpx-vp9",
                "-f",
                "webm",
            ],
            &video,
        );
        let made_audio = make(
            &ffmpeg,
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-vn",
                "-c:a",
                "libopus",
                "-f",
                "webm",
            ],
            &audio,
        );
        if !made_video || !made_audio {
            return;
        }

        let out = dir.join("joined.webm");
        join(&[video, audio], &out, &dir, Container::Webm).expect("a join");
        decodes(&ffmpeg, &out);
        let info = super::super::verify::inspect(&out).expect("the file is WebM");
        assert!(info.video_codec.is_some(), "the picture is there");
        assert!(info.audio_codec.is_some(), "the sound is there");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /* -------------------------------------------- deterministic fixtures */

    /// A whole WebM file, assembled from the same primitives the writer uses.
    ///
    /// Building it here instead of with an encoder is what makes the reader's
    /// edge cases testable everywhere: no ffmpeg, no sample files, no clock.
    fn fixture(tracks: &[RawTrack], clusters: &[Vec<u8>]) -> Vec<u8> {
        let mut out = ebml_header("webm");
        out.extend(id(SEGMENT));
        out.extend(unknown_size());
        out.extend(info(0));
        let mut entries = Vec::new();
        for track in tracks {
            entries.extend(track_entry(track.number, track));
        }
        out.extend(element(TRACKS, &entries));
        for cluster in clusters {
            out.extend_from_slice(cluster);
        }
        out
    }

    fn video_track(number: u64) -> RawTrack {
        RawTrack {
            number,
            kind: 1,
            codec_id: b"V_VP9".to_vec(),
            default_duration: Some(40_000_000),
            pixel_width: Some(320),
            pixel_height: Some(240),
            ..RawTrack::default()
        }
    }

    fn audio_track(number: u64) -> RawTrack {
        RawTrack {
            number,
            kind: 2,
            codec_id: b"A_OPUS".to_vec(),
            default_duration: Some(20_000_000),
            codec_delay: Some(6_500_000),
            seek_pre_roll: Some(80_000_000),
            sample_rate: Some(48_000.0),
            channels: Some(2),
            ..RawTrack::default()
        }
    }

    fn simple_block(track: u64, relative: i16, keyframe: bool, frames: &[u8]) -> Vec<u8> {
        let mut payload = encode_vint(track);
        payload.extend(relative.to_be_bytes());
        payload.push(if keyframe { 0x80 } else { 0x00 });
        payload.extend(frames);
        element(SIMPLE_BLOCK, &payload)
    }

    fn block_group(track: u64, relative: i16, keyframe: bool, padding: i64, frames: &[u8]) -> Vec<u8> {
        let mut payload = encode_vint(track);
        payload.extend(relative.to_be_bytes());
        // A plain `Block` must not set the keyframe bit: inside a group the
        // absence of a `ReferenceBlock` is what says the frame stands alone.
        payload.push(0x00);
        payload.extend(frames);
        let mut body = element(BLOCK, &payload);
        if !keyframe {
            body.extend(uint_element(REFERENCE_BLOCK, 0));
        }
        body.extend(element(DISCARD_PADDING, &padding.to_be_bytes()));
        element(BLOCK_GROUP, &body)
    }

    fn cluster(at_ms: u64, children: &[Vec<u8>], sized: bool) -> Vec<u8> {
        let mut body = uint_element(CLUSTER_TIMESTAMP, at_ms);
        for child in children {
            body.extend_from_slice(child);
        }
        let mut out = id(CLUSTER);
        out.extend(if sized {
            encode_size(body.len() as u64)
        } else {
            unknown_size()
        });
        out.extend(body);
        out
    }

    fn found(bytes: &[u8], wanted: u64) -> Option<usize> {
        let pattern = id(wanted);
        bytes
            .windows(pattern.len())
            .position(|window| window == pattern.as_slice())
    }

    /// The body of the first `wanted` element of a finished file.
    fn body_of(bytes: &[u8], wanted: u64) -> Option<Vec<u8>> {
        let at = found(bytes, wanted)?;
        let (_, id_len) = read_id(&bytes[at..])?;
        let (size, size_len) = read_size(&bytes[at + id_len..])?;
        let data = at + id_len + size_len;
        Some(bytes[data..data + size?].to_vec())
    }

    /// Where the Segment's data begins, which is what a cue position counts from.
    fn segment_start(bytes: &[u8]) -> usize {
        let (_, ebml_id) = read_id(bytes).expect("the EBML header");
        let (ebml_size, ebml_len) = read_size(&bytes[ebml_id..]).expect("its size");
        let after = ebml_id + ebml_len + ebml_size.expect("a known size");
        let (_, segment_id) = read_id(&bytes[after..]).expect("the Segment");
        let (_, segment_len) = read_size(&bytes[after + segment_id..]).expect("its size");
        after + segment_id + segment_len
    }

    fn written(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("the fixture");
        path
    }

    #[test]
    fn a_padding_is_a_signed_number() {
        assert_eq!(read_int(&[0x85]), Some(-123));
        assert_eq!(
            read_int(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x85]),
            Some(-123),
            "the top bit is what makes it negative"
        );
        assert_eq!(read_int(&[0x00, 0x7B]), Some(123));
        assert_eq!(read_int(&[]), None);
        assert_eq!(read_int(&[0; 9]), None, "a Matroska integer holds eight bytes");
    }

    #[test]
    fn a_block_group_keeps_its_padding_and_its_reference() {
        let dir = folder("padding");
        let input = written(
            &dir,
            "audio.track",
            &fixture(
                &[audio_track(1)],
                &[cluster(
                    0,
                    &[
                        block_group(1, 0, true, -123, &[1, 2]),
                        block_group(1, 20, false, 456, &[3, 4]),
                    ],
                    true,
                )],
            ),
        );
        let out = dir.join("joined.webm");
        join(&[input], &out, &dir, Container::Webm).expect("a join");

        let joined = read_input(&out).expect("the joined file");
        assert_eq!(joined.blocks.len(), 2, "both frames are kept");
        assert_eq!(joined.blocks[0].discard_padding, Some(-123));
        assert_eq!(joined.blocks[1].discard_padding, Some(456));
        assert!(
            joined.blocks[0].keyframe,
            "a padded first frame still stands alone"
        );
        assert!(
            !joined.blocks[1].keyframe,
            "a reference block is not a keyframe"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_size_cluster_does_not_swallow_the_clusters_after_it() {
        let dir = folder("nested");
        let input = written(
            &dir,
            "video.track",
            &fixture(
                &[video_track(1)],
                &[
                    cluster(0, &[simple_block(1, 0, true, &[1])], false),
                    cluster(2000, &[simple_block(1, 0, true, &[2])], true),
                ],
            ),
        );
        let out = dir.join("joined.webm");
        join(&[input], &out, &dir, Container::Webm).expect("a join");

        let joined = read_input(&out).expect("the joined file");
        assert_eq!(
            joined.blocks.len(),
            2,
            "the blocks after the open cluster are kept"
        );
        assert_eq!(joined.blocks[1].time_ns, 2_000_000_000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_track_that_never_appears_is_not_declared() {
        let dir = folder("unused-track");
        let input = written(
            &dir,
            "mixed.track",
            &fixture(
                &[video_track(1), audio_track(2)],
                &[cluster(0, &[simple_block(1, 0, true, &[1])], true)],
            ),
        );
        let out = dir.join("joined.webm");
        join(&[input], &out, &dir, Container::Webm).expect("a join");

        let joined = read_input(&out).expect("the joined file");
        assert_eq!(
            joined.tracks.len(),
            1,
            "a stream that never delivers a frame is not promised"
        );
        assert_eq!(joined.tracks[0].number, 1);
        assert_eq!(joined.tracks[0].kind, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_codec_delay_and_the_seek_pre_roll_survive_the_join() {
        let dir = folder("delay");
        let input = written(
            &dir,
            "audio.track",
            &fixture(
                &[audio_track(1)],
                &[cluster(0, &[simple_block(1, 0, true, &[1])], true)],
            ),
        );
        let out = dir.join("joined.webm");
        join(&[input], &out, &dir, Container::Webm).expect("a join");

        let joined = read_input(&out).expect("the joined file");
        assert_eq!(
            joined.tracks[0].codec_delay,
            Some(6_500_000),
            "without this the sound starts ahead of the picture"
        );
        assert_eq!(joined.tracks[0].seek_pre_roll, Some(80_000_000));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_file_ends_when_its_last_frame_ends() {
        let dir = folder("duration");
        let input = written(
            &dir,
            "video.track",
            &fixture(
                &[video_track(1)],
                &[cluster(
                    0,
                    &[
                        simple_block(1, 0, true, &[1]),
                        simple_block(1, 1000, true, &[2]),
                    ],
                    true,
                )],
            ),
        );
        let out = dir.join("joined.webm");
        join(&[input], &out, &dir, Container::Webm).expect("a join");

        let bytes = std::fs::read(&out).expect("the file");
        let info = body_of(&bytes, INFO).expect("an Info");
        let duration = element_float(&info, DURATION).expect("a duration");
        // The last frame starts at one second and is forty milliseconds long.
        assert!(
            (duration - 1040.0).abs() < 0.5,
            "the duration was {duration}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_cue_point_lands_on_the_cluster_it_names() {
        let dir = folder("cues");
        let input = written(
            &dir,
            "video.track",
            &fixture(
                &[video_track(1)],
                &[
                    cluster(0, &[simple_block(1, 0, true, &[1])], true),
                    cluster(2000, &[simple_block(1, 0, true, &[2])], true),
                ],
            ),
        );
        let out = dir.join("joined.webm");
        join(&[input], &out, &dir, Container::Webm).expect("a join");

        let bytes = std::fs::read(&out).expect("the file");
        let cues = body_of(&bytes, CUES).expect("a cue index");
        let start = segment_start(&bytes);
        let mut points = 0;
        for point in elements(&cues) {
            if point.id != CUE_POINT {
                continue;
            }
            let entry = &cues[point.data..point.end];
            let positions = element_body(entry, CUE_TRACK_POSITIONS).expect("track positions");
            let position = element_uint(positions, CUE_CLUSTER_POSITION).expect("a position");
            let at = start + position as usize;
            assert_eq!(
                &bytes[at..at + id(CLUSTER).len()],
                id(CLUSTER).as_slice(),
                "a cue must name the first byte of a cluster"
            );
            assert_eq!(element_uint(positions, CUE_TRACK), Some(1));
            points += 1;
        }
        assert_eq!(points, 2, "one cue per keyframe cluster");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
