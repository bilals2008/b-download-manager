//! Reading an MP4 track: the progressive sample tables and the track list.

use super::boxes::*;
use super::fragment::fragment_samples;
use super::write::{patch_mdhd_duration, patch_tkhd_duration};
use super::*;

/// Read the first track of an MP4 file.
pub(super) fn read_track(path: &Path) -> Result<Track, MuxError> {
    let mut file = File::open(path)
        .map_err(|error| MuxError::Io(format!("{} could not be read: {error}", path.display())))?;
    let bytes = file
        .metadata()
        .map(|meta| meta.len())
        .map_err(|error| MuxError::Io(format!("{} could not be read: {error}", path.display())))?;
    if bytes < 8 {
        return Err(MuxError::Io(format!(
            "{} is not a media file",
            path.display()
        )));
    }

    let mut moov = None;
    let mut fragments: Vec<Fragment> = Vec::new();
    let mut at = 0u64;
    while at + 8 <= bytes {
        let Some(boxed) = read_box_header(&mut file, at, bytes)? else {
            break;
        };
        match &boxed.tag {
            b"moov" => moov = Some(read_box_body(&mut file, &boxed)?),
            b"moof" => {
                let body = read_box_body(&mut file, &boxed)?;
                fragments.push(Fragment {
                    moof: body,
                    base: boxed.start,
                });
            }
            _ => {}
        }
        at = boxed.start + boxed.size;
    }

    let Some(moov) = moov else {
        return Err(MuxError::Io(format!("{} has no MP4 index", path.display())));
    };

    for raw in moov_tracks(&moov)? {
        let samples = if fragments.is_empty() {
            progressive_samples(&moov, raw.id)?
        } else {
            fragment_samples(&fragments, &moov, raw.id)?
        };
        if samples.is_empty() {
            continue;
        }
        let mut track = Track {
            source: path.to_path_buf(),
            handler: raw.handler,
            timescale: raw.timescale,
            tkhd: raw.tkhd.to_vec(),
            mdhd: raw.mdhd.to_vec(),
            stsd: raw.stsd.to_vec(),
            start_time: samples[0].decode_time,
            samples,
        };
        let duration = track.duration_units();
        if duration == 0 {
            return Err(MuxError::Io(format!(
                "{} has no playable time",
                path.display()
            )));
        }
        let presentation = track.presentation_units();
        patch_tkhd_duration(&mut track.tkhd, track.timescale, presentation);
        patch_mdhd_duration(&mut track.mdhd, duration);
        return Ok(track);
    }

    Err(MuxError::Io(format!("{} has no tracks", path.display())))
}

pub(super) fn moov_tracks(moov: &[u8]) -> Result<Vec<RawTrack<'_>>, MuxError> {
    let mut tracks = Vec::new();
    for atom in atoms(moov) {
        if &atom.tag != b"trak" {
            continue;
        }
        let trak = &moov[atom.start..atom.end];
        let tkhd = find_box(trak, b"tkhd")
            .ok_or_else(|| MuxError::Io("a track has no header".to_string()))?;
        let mdia = find_box(trak, b"mdia")
            .ok_or_else(|| MuxError::Io("a track has no media".to_string()))?;
        let media = &trak[mdia.start..mdia.end];
        let hdlr = find_box(media, b"hdlr")
            .ok_or_else(|| MuxError::Io("a track has no handler".to_string()))?;
        let mdhd = find_box(media, b"mdhd")
            .ok_or_else(|| MuxError::Io("a track has no time".to_string()))?;
        tracks.push(RawTrack {
            id: track_id(&trak[tkhd.start..tkhd.end]),
            handler: handler_kind(&media[hdlr.start..hdlr.end]),
            timescale: mdhd_timescale(&media[mdhd.start..mdhd.end]),
            tkhd: &trak[tkhd.start..tkhd.end],
            mdhd: &media[mdhd.start..mdhd.end],
            stsd: sample_description(media)?,
        });
    }
    Ok(tracks)
}

fn progressive_samples(moov: &[u8], track_id: u32) -> Result<Vec<Sample>, MuxError> {
    let trak = track_by_id(moov, track_id)?;
    let mdia =
        find_box(trak, b"mdia").ok_or_else(|| MuxError::Io("a track has no media".to_string()))?;
    let media = &trak[mdia.start..mdia.end];
    let minf = find_box(media, b"minf")
        .ok_or_else(|| MuxError::Io("a track has no media info".to_string()))?;
    let stbl = find_box(&media[minf.start..minf.end], b"stbl")
        .ok_or_else(|| MuxError::Io("a track has no sample table".to_string()))?;
    let stbl = &media[minf.start..minf.end][stbl.start..stbl.end];

    let durations = expand_stts(find_box(stbl, b"stts").map(|a| &stbl[a.start..a.end]))?;
    let sizes = expand_stsz(find_box(stbl, b"stsz").map(|a| &stbl[a.start..a.end]))?;
    let offsets = chunk_offsets(
        find_box(stbl, b"stco").map(|a| &stbl[a.start..a.end]),
        find_box(stbl, b"co64").map(|a| &stbl[a.start..a.end]),
    )?;
    let runs = stsc_runs(find_box(stbl, b"stsc").map(|a| &stbl[a.start..a.end]))?;
    let sync = sync_samples(find_box(stbl, b"stss").map(|a| &stbl[a.start..a.end]))?;
    let compositions = composition_offsets(
        find_box(stbl, b"ctts").map(|a| &stbl[a.start..a.end]),
        sizes.len(),
    )?;

    let count = sizes.len();
    if count == 0 {
        return Ok(Vec::new());
    }
    if durations.len() != count {
        return Err(MuxError::Io(
            "the track's sample table is inconsistent".to_string(),
        ));
    }

    let chunk_of = chunk_of_sample(&runs, count)?;

    let mut samples = Vec::with_capacity(count);
    let mut within = 0u64;
    let mut last_chunk = usize::MAX;
    let mut decode_time = 0u64;
    for (index, size) in sizes.iter().enumerate() {
        let chunk = chunk_of[index];
        if chunk != last_chunk {
            within = 0;
            last_chunk = chunk;
        }
        let chunk_start = *offsets
            .get(chunk)
            .ok_or_else(|| MuxError::Io("a chunk is missing".to_string()))?;
        let offset = chunk_start
            .checked_add(within)
            .ok_or_else(|| MuxError::Io("a sample offset runs past the file".to_string()))?;
        within += *size as u64;
        let sync = match &sync {
            Some(list) => list.binary_search(&((index + 1) as u32)).is_ok(),
            None => true,
        };
        samples.push(Sample {
            offset,
            size: *size,
            duration: durations[index],
            decode_time,
            composition_offset: compositions.as_ref().map(|list| list[index]).unwrap_or(0),
            sync,
        });
        decode_time = decode_time.saturating_add(durations[index] as u64);
    }
    Ok(samples)
}

/// One composition offset per sample, from a `ctts` box.
///
/// Version 0 states an unsigned offset, version 1 a signed one. A B-frame
/// stream needs the signed form, so both are read and the value is kept as the
/// signed number the writer has to reproduce.
///
/// A `ctts` names how many *entries* it holds, not how many samples, so the
/// sample count has to come from `stsz`. The runs then have to add up to it
/// exactly: a table that falls short would leave trailing pictures with no
/// offset, and one that overruns would move a picture that is not there.
pub(super) fn composition_offsets(
    body: Option<&[u8]>,
    samples: usize,
) -> Result<Option<Vec<i32>>, MuxError> {
    let Some(body) = body else {
        return Ok(None);
    };
    if body.len() < 8 {
        return Err(MuxError::Io(
            "the composition table is truncated".to_string(),
        ));
    }
    let version = body[0];
    if version > 1 {
        return Err(MuxError::Io(format!(
            "the composition table names version {version}, which this build cannot read"
        )));
    }
    let entries = u32::from_be_bytes([body[4], body[5], body[6], body[7]]) as usize;
    let mut out: Vec<i32> = Vec::with_capacity(samples.min(1 << 16));
    let mut at = 8usize;
    for _ in 0..entries {
        if out.len() == samples {
            break;
        }
        let end = at + 8;
        if end > body.len() {
            return Err(MuxError::Io(
                "the composition table is truncated".to_string(),
            ));
        }
        let run = u32::from_be_bytes([body[at], body[at + 1], body[at + 2], body[at + 3]]);
        let value = u32::from_be_bytes([body[at + 4], body[at + 5], body[at + 6], body[at + 7]]);
        at = end;
        if run == 0 {
            return Err(MuxError::Io(
                "a composition entry names no samples".to_string(),
            ));
        }
        if run as usize > samples - out.len() {
            return Err(MuxError::Io(
                "the composition table names more samples than the track has".to_string(),
            ));
        }
        // Version 0 states an unsigned offset and version 1 a signed one. A
        // version-0 value above `i32::MAX` is not a plausible composition
        // offset, so it is kept as the same bit pattern rather than refused.
        let offset = value as i32;
        for _ in 0..run {
            out.push(offset);
        }
    }
    if out.len() != samples {
        return Err(MuxError::Io(
            "the composition table does not cover every sample".to_string(),
        ));
    }
    Ok(Some(out))
}

/// The chunk (0-based) each sample belongs to, from the `stsc` runs.
fn chunk_of_sample(runs: &[StscRun], count: usize) -> Result<Vec<usize>, MuxError> {
    let mut out = Vec::with_capacity(count);
    for (run_index, run) in runs.iter().enumerate() {
        let per = run.samples_per_chunk as usize;
        if per == 0 {
            return Err(MuxError::Io("a chunk holds no samples".to_string()));
        }
        let chunk_start = run.first_chunk as usize - 1;
        let chunk_end = if run_index + 1 < runs.len() {
            runs[run_index + 1].first_chunk as usize - 1
        } else {
            // The last run covers everything left.
            chunk_start + count.saturating_sub(out.len()).div_ceil(per)
        };
        for chunk in chunk_start..chunk_end {
            for _ in 0..per {
                if out.len() >= count {
                    break;
                }
                out.push(chunk);
            }
            if out.len() >= count {
                break;
            }
        }
        if out.len() >= count {
            break;
        }
    }
    if out.len() != count {
        return Err(MuxError::Io(
            "the track's sample map is inconsistent".to_string(),
        ));
    }
    Ok(out)
}

struct StscRun {
    first_chunk: u32,
    samples_per_chunk: u32,
}

fn stsc_runs(body: Option<&[u8]>) -> Result<Vec<StscRun>, MuxError> {
    let Some(data) = body else {
        return Err(MuxError::Io("a track has no chunk map".to_string()));
    };
    if data.len() < 8 {
        return Err(MuxError::Io("a track's chunk map is too short".to_string()));
    }
    let entries = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let mut runs = Vec::with_capacity(entries);
    let mut at = 8usize;
    for _ in 0..entries {
        if at + 12 > data.len() {
            break;
        }
        runs.push(StscRun {
            first_chunk: u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]),
            samples_per_chunk: u32::from_be_bytes([
                data[at + 4],
                data[at + 5],
                data[at + 6],
                data[at + 7],
            ]),
        });
        at += 12;
    }
    Ok(runs)
}

fn expand_stts(body: Option<&[u8]>) -> Result<Vec<u32>, MuxError> {
    let Some(data) = body else {
        return Err(MuxError::Io("a track has no time map".to_string()));
    };
    if data.len() < 8 {
        return Err(MuxError::Io("a track's time map is too short".to_string()));
    }
    let entries = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let mut out = Vec::new();
    let mut at = 8usize;
    for _ in 0..entries {
        if at + 8 > data.len() {
            break;
        }
        let count = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        let delta = u32::from_be_bytes([data[at + 4], data[at + 5], data[at + 6], data[at + 7]]);
        for _ in 0..count {
            out.push(delta);
        }
        at += 8;
    }
    Ok(out)
}

fn expand_stsz(body: Option<&[u8]>) -> Result<Vec<u32>, MuxError> {
    let Some(data) = body else {
        return Err(MuxError::Io("a track has no size map".to_string()));
    };
    if data.len() < 12 {
        return Err(MuxError::Io("a track's size map is too short".to_string()));
    }
    let uniform = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let count = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    if uniform != 0 {
        return Ok(vec![uniform; count]);
    }
    let mut out = Vec::with_capacity(count);
    let mut at = 12usize;
    for _ in 0..count {
        if at + 4 > data.len() {
            break;
        }
        out.push(u32::from_be_bytes([
            data[at],
            data[at + 1],
            data[at + 2],
            data[at + 3],
        ]));
        at += 4;
    }
    Ok(out)
}

fn chunk_offsets(stco: Option<&[u8]>, co64: Option<&[u8]>) -> Result<Vec<u64>, MuxError> {
    let (data, width) = match (stco, co64) {
        (Some(body), _) => (body, 4),
        (_, Some(body)) => (body, 8),
        _ => return Err(MuxError::Io("a track has no chunk offsets".to_string())),
    };
    if data.len() < 8 {
        return Err(MuxError::Io(
            "a track's chunk offsets are too short".to_string(),
        ));
    }
    let entries = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let mut out = Vec::with_capacity(entries);
    let mut at = 8usize;
    for _ in 0..entries {
        if at + width > data.len() {
            break;
        }
        if width == 4 {
            out.push(
                u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as u64,
            );
        } else {
            out.push(u64::from_be_bytes([
                data[at],
                data[at + 1],
                data[at + 2],
                data[at + 3],
                data[at + 4],
                data[at + 5],
                data[at + 6],
                data[at + 7],
            ]));
        }
        at += width;
    }
    Ok(out)
}

fn sync_samples(body: Option<&[u8]>) -> Result<Option<Vec<u32>>, MuxError> {
    let Some(data) = body else {
        return Ok(None);
    };
    if data.len() < 8 {
        return Ok(None);
    }
    let entries = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let mut out = Vec::with_capacity(entries);
    let mut at = 8usize;
    for _ in 0..entries {
        if at + 4 > data.len() {
            break;
        }
        out.push(u32::from_be_bytes([
            data[at],
            data[at + 1],
            data[at + 2],
            data[at + 3],
        ]));
        at += 4;
    }
    Ok(Some(out))
}
