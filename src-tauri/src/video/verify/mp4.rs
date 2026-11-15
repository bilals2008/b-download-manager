//! Reading an MP4 index: its tracks, codecs, duration, and whether the samples it names lie inside the file.

use super::*;

/* ---------------------------------------------------------------------- mp4 */

/// Read the top-level boxes until `moov`, then its track list.
pub(super) fn parse_mp4(file: &mut File, bytes: u64) -> Result<MediaInfo, String> {
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
            if sync
                .iter()
                .any(|number| *number == 0 || *number as u64 > table.count)
            {
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
        &[
            b"avcC", b"hvcC", b"av1C", b"vpcC", b"esds", b"dOps", b"dac3", b"dec3",
        ],
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
