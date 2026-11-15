//! Writing the joined WebM/Matroska: tracks, clusters, cues.

use super::*;
use super::ebml::*;

/* ---------------------------------------------------------------- writing */

pub(super) fn write_joined(
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
        group.extend(element(
            BLOCK,
            &block_payload(block, relative, frames, false),
        ));
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

pub(super) fn ebml_header(doc_type: &str) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(string_element(DOC_TYPE, doc_type.as_bytes()));
    body.extend(uint_element(DOC_TYPE_VERSION, 4));
    body.extend(uint_element(DOC_TYPE_READ_VERSION, 2));
    element(EBML, &body)
}

pub(super) fn info(duration_ns: u64) -> Vec<u8> {
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

pub(super) fn track_entry(number: u64, track: &RawTrack) -> Vec<u8> {
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
fn temporary_path(work_dir: &Path, output: &Path) -> PathBuf {
    let name = output
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    work_dir.join(format!(".{name}.idmmux"))
}
