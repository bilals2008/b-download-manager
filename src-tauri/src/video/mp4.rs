//! A native MP4 remuxer.
//!
//! Joins one or two MP4-family tracks into a single progressive MP4 without
//! re-encoding a single sample. Both the plain MP4 a progressive download
//! yields and the fragmented MP4 (`moov` + `moof`/`mdat`) that YouTube and DASH
//! serve are read; the output is an `ftyp` + `moov` + `mdat` file with its
//! index first, so it seeks and streams like any downloaded file.
//!
//! The bytes of every sample are copied unchanged, and the codec configuration
//! (the `stsd` sample entry: `avcC`, `esds`, ...) is copied verbatim, so the
//! output is exactly what the player served, only re-arranged. Nothing here
//! knows what a codec is; it knows the MP4 box layout.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::mux::{Container, MuxError};

/// One decoded sample: where its bytes are, how long they are, how long the
/// sample plays, and whether it is a sync (key) sample.
#[derive(Debug, Clone)]
struct Sample {
    offset: u64,
    size: u32,
    duration: u32,
    sync: bool,
}

/// One track, reduced to what the writer needs to rebuild it.
struct Track {
    /// Where the sample bytes are read from.
    source: PathBuf,
    /// `vide` or `soun`.
    handler: [u8; 4],
    /// The track's own timescale, from `mdhd`.
    timescale: u32,
    /// The `tkhd` box verbatim; its duration is patched before writing.
    tkhd: Vec<u8>,
    /// The `mdhd` box verbatim; its duration is patched before writing.
    mdhd: Vec<u8>,
    /// The `stsd` box verbatim.
    stsd: Vec<u8>,
    samples: Vec<Sample>,
}

impl Track {
    fn duration_units(&self) -> u64 {
        self.samples
            .iter()
            .map(|sample| sample.duration as u64)
            .sum()
    }
}

/// The movie timescale the output `mvhd` uses.
const MOVIE_TIMESCALE: u32 = 1000;

/// Join the tracks into one progressive MP4 at `output`.
pub(crate) fn join(
    inputs: &[PathBuf],
    output: &Path,
    work_dir: &Path,
    container: Container,
) -> Result<(), MuxError> {
    if !matches!(container, Container::Mp4 | Container::M4a) {
        return Err(MuxError::Io(format!(
            "the {} container cannot be written yet; only MP4 and M4A are",
            container.extension()
        )));
    }
    if inputs.is_empty() || inputs.len() > 2 {
        return Err(MuxError::Io(
            "one or two MP4 tracks are expected".to_string(),
        ));
    }

    let mut tracks = Vec::with_capacity(inputs.len());
    for input in inputs {
        tracks.push(read_track(input)?);
    }
    std::fs::create_dir_all(work_dir)
        .map_err(|error| MuxError::Io(format!("the storage folder could not be made: {error}")))?;
    write_joined(&tracks, output, work_dir, container)
}

/* --------------------------------------------------------------- reading */

/// Read the first track of an MP4 file.
fn read_track(path: &Path) -> Result<Track, MuxError> {
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
            fragment_samples(&fragments, raw.id)?
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
            samples,
        };
        let duration = track.duration_units();
        if duration == 0 {
            return Err(MuxError::Io(format!(
                "{} has no playable time",
                path.display()
            )));
        }
        patch_tkhd_duration(&mut track.tkhd, track.timescale, duration);
        patch_mdhd_duration(&mut track.mdhd, duration);
        return Ok(track);
    }

    Err(MuxError::Io(format!("{} has no tracks", path.display())))
}

struct Fragment {
    moof: Vec<u8>,
    base: u64,
}

struct RawTrack<'a> {
    id: u32,
    handler: [u8; 4],
    timescale: u32,
    tkhd: &'a [u8],
    mdhd: &'a [u8],
    stsd: &'a [u8],
}

fn moov_tracks(moov: &[u8]) -> Result<Vec<RawTrack<'_>>, MuxError> {
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
    for (index, size) in sizes.iter().enumerate() {
        let chunk = chunk_of[index];
        if chunk != last_chunk {
            within = 0;
            last_chunk = chunk;
        }
        let chunk_start = *offsets
            .get(chunk)
            .ok_or_else(|| MuxError::Io("a chunk is missing".to_string()))?;
        let offset = chunk_start + within;
        within += *size as u64;
        let sync = match &sync {
            Some(list) => list.binary_search(&((index + 1) as u32)).is_ok(),
            None => true,
        };
        samples.push(Sample {
            offset,
            size: *size,
            duration: durations[index],
            sync,
        });
    }
    Ok(samples)
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

/* -------------------------------------------------------- fragmented reads */

fn fragment_samples(fragments: &[Fragment], track_id: u32) -> Result<Vec<Sample>, MuxError> {
    let mut samples = Vec::new();
    for fragment in fragments {
        for atom in atoms(&fragment.moof) {
            if &atom.tag != b"traf" {
                continue;
            }
            let traf = &fragment.moof[atom.start..atom.end];
            let tfhd = find_box(traf, b"tfhd")
                .ok_or_else(|| MuxError::Io("a fragment has no header".to_string()))?;
            let (frag_track_id, base_data_offset, default_duration, default_size, default_flags) =
                parse_tfhd(&traf[tfhd.start..tfhd.end], fragment.base);
            if frag_track_id != track_id {
                continue;
            }
            let Some(trun) = find_box(traf, b"trun") else {
                continue;
            };
            let trun = &traf[trun.start..trun.end];
            let data = trun;
            if data.len() < 8 {
                continue;
            }
            let flags = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
            let count = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
            let mut at = 8usize;
            let mut data_offset = base_data_offset;
            if flags & 0x1 != 0 {
                data_offset = base_data_offset.wrapping_add(i32::from_be_bytes([
                    data[at],
                    data[at + 1],
                    data[at + 2],
                    data[at + 3],
                ]) as u64);
                at += 4;
            }
            let mut current_flags = if flags & 0x4 != 0 {
                let value =
                    u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
                at += 4;
                value
            } else {
                default_flags
            };
            let mut running = 0u64;
            for _ in 0..count {
                let duration = if flags & 0x100 != 0 {
                    let value =
                        u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
                    at += 4;
                    value
                } else {
                    default_duration
                };
                let size = if flags & 0x200 != 0 {
                    let value =
                        u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
                    at += 4;
                    value
                } else {
                    default_size
                };
                let sample_flags = if flags & 0x400 != 0 {
                    let value =
                        u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
                    at += 4;
                    value
                } else {
                    current_flags
                };
                if flags & 0x800 != 0 {
                    at += 4;
                }
                samples.push(Sample {
                    offset: data_offset + running,
                    size,
                    duration,
                    sync: sample_flags & 0x0001_0000 == 0,
                });
                running += size as u64;
                current_flags = sample_flags;
            }
        }
    }
    Ok(samples)
}

fn parse_tfhd(body: &[u8], base: u64) -> (u32, u64, u32, u32, u32) {
    let data = body;
    if data.len() < 8 {
        return (0, base, 0, 0, 0);
    }
    let flags = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let track_id = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let mut at = 8usize;
    let mut base_data_offset = base;
    if flags & 0x1 != 0 {
        base_data_offset = u64::from_be_bytes([
            data[at],
            data[at + 1],
            data[at + 2],
            data[at + 3],
            data[at + 4],
            data[at + 5],
            data[at + 6],
            data[at + 7],
        ]);
        at += 8;
    }
    let mut default_duration = 0u32;
    if flags & 0x8 != 0 {
        default_duration = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        at += 4;
    }
    let mut default_size = 0u32;
    if flags & 0x10 != 0 {
        default_size = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        at += 4;
    }
    let mut default_flags = 0u32;
    if flags & 0x20 != 0 {
        default_flags = u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    }
    (
        track_id,
        base_data_offset,
        default_duration,
        default_size,
        default_flags,
    )
}

/* ---------------------------------------------------------------- writing */

fn write_joined(
    tracks: &[Track],
    output: &Path,
    work_dir: &Path,
    container: Container,
) -> Result<(), MuxError> {
    let temporary = temporary_path(work_dir, output);
    let mut file = File::create(&temporary)
        .map_err(|error| MuxError::Io(format!("the file could not be written: {error}")))?;

    let ftyp = ftyp(container);
    let mut moov = build_moov(tracks);

    let data_size: u64 = tracks
        .iter()
        .flat_map(|track| track.samples.iter())
        .map(|sample| sample.size as u64)
        .sum();
    let mdat_header = if data_size + 8 > u32::MAX as u64 {
        16u64
    } else {
        8u64
    };
    // The chunk offsets point into the `mdat` that follows. The `mdat` header
    // length is fixed by the data size, so the `moov` length does not move when
    // its offsets are patched.
    let mut offsets = Vec::with_capacity(tracks.len());
    let mut cursor = ftyp.len() as u64 + moov.len() as u64 + mdat_header;
    for track in tracks {
        offsets.push(cursor);
        cursor += track
            .samples
            .iter()
            .map(|sample| sample.size as u64)
            .sum::<u64>();
    }
    patch_chunk_offsets(&mut moov, &offsets);

    file.write_all(&ftyp)
        .and_then(|_| file.write_all(&moov))
        .map_err(|error| MuxError::Io(format!("the file could not be written: {error}")))?;

    if mdat_header == 16 {
        file.write_all(&(data_size + 16).to_be_bytes())
            .and_then(|_| file.write_all(b"mdat"))
            .and_then(|_| file.write_all(&data_size.to_be_bytes()))
            .map_err(|error| MuxError::Io(format!("the file could not be written: {error}")))?;
    } else {
        file.write_all(&((data_size + 8) as u32).to_be_bytes())
            .and_then(|_| file.write_all(b"mdat"))
            .map_err(|error| MuxError::Io(format!("the file could not be written: {error}")))?;
    }

    for track in tracks {
        let mut reader = File::open(&track.source)
            .map_err(|error| MuxError::Io(format!("a track could not be read: {error}")))?;
        for sample in &track.samples {
            if super::mux::is_cancelled(output) {
                let _ = std::fs::remove_file(&temporary);
                return Err(MuxError::Cancelled);
            }
            copy_sample(&mut reader, &mut file, sample)?;
        }
    }
    file.flush()
        .map_err(|error| MuxError::Io(format!("the file could not be written: {error}")))?;
    drop(file);

    idm_core::disk::replace_file(&temporary, output)
        .map_err(|error| MuxError::Io(error.to_string()))
}

fn copy_sample(reader: &mut File, writer: &mut File, sample: &Sample) -> Result<(), MuxError> {
    reader
        .seek(SeekFrom::Start(sample.offset))
        .map_err(|error| MuxError::Io(format!("a sample could not be read: {error}")))?;
    let mut buffer = vec![0u8; sample.size as usize];
    reader
        .read_exact(&mut buffer)
        .map_err(|error| MuxError::Io(format!("a sample could not be read: {error}")))?;
    writer
        .write_all(&buffer)
        .map_err(|error| MuxError::Io(format!("the file could not be written: {error}")))?;
    Ok(())
}

fn ftyp(container: Container) -> Vec<u8> {
    let brand = if container == Container::M4a {
        *b"M4A "
    } else {
        *b"isom"
    };
    let mut out = Vec::new();
    let body_len = 16u32; // major brand + minor + two compatible brands
    out.extend(&(body_len + 8).to_be_bytes());
    out.extend(b"ftyp");
    out.extend(&brand);
    out.extend(&0u32.to_be_bytes());
    out.extend(&brand);
    out.extend(b"mp41");
    out
}

fn build_moov(tracks: &[Track]) -> Vec<u8> {
    let mut children: Vec<Vec<u8>> = Vec::new();
    children.push(mvhd(tracks));
    for track in tracks {
        children.push(trak(track));
    }
    boxed(b"moov", &children.concat())
}

fn mvhd(tracks: &[Track]) -> Vec<u8> {
    let duration = tracks
        .iter()
        .map(|track| {
            (track.duration_units() as f64 * MOVIE_TIMESCALE as f64 / track.timescale as f64)
                .round() as u64
        })
        .max()
        .unwrap_or(0);
    let next_track_id = tracks
        .iter()
        .map(|track| track_id(&track.tkhd))
        .max()
        .unwrap_or(0)
        + 1;
    let mut body = Vec::with_capacity(100);
    body.extend(&0u32.to_be_bytes()); // version 0 + flags
    body.extend(&0u32.to_be_bytes()); // creation time
    body.extend(&0u32.to_be_bytes()); // modification time
    body.extend(&MOVIE_TIMESCALE.to_be_bytes());
    body.extend(&(duration as u32).to_be_bytes());
    body.extend(&0x0001_0000u32.to_be_bytes()); // rate 1.0
    body.extend(&0x0100u16.to_be_bytes()); // volume 1.0
    body.extend(&[0u8; 10]); // reserved
    body.extend(&identity_matrix());
    body.extend(&[0u8; 24]); // pre_defined
    body.extend(&next_track_id.to_be_bytes());
    boxed(b"mvhd", &body)
}

fn identity_matrix() -> [u8; 36] {
    let mut matrix = [0u8; 36];
    matrix[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    matrix[12..16].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    matrix[32..36].copy_from_slice(&0x4000_0000u32.to_be_bytes());
    matrix
}

fn trak(track: &Track) -> Vec<u8> {
    let stbl = boxed(
        b"stbl",
        &[
            boxed(b"stsd", &track.stsd),
            stts(track),
            stsc(track),
            stsz(track),
            co64(track),
            stss(track),
        ]
        .concat(),
    );
    let minf = boxed(b"minf", &stbl);
    let mdia = boxed(
        b"mdia",
        &[boxed(b"mdhd", &track.mdhd), hdlr(track.handler), minf].concat(),
    );
    boxed(b"trak", &[boxed(b"tkhd", &track.tkhd), mdia].concat())
}

fn hdlr(handler: [u8; 4]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(&0u32.to_be_bytes()); // version + flags
    body.extend(&0u32.to_be_bytes()); // pre_defined
    body.extend(&handler);
    body.extend(&[0u8; 12]); // reserved
    body.extend(b"B Download Manager\0");
    boxed(b"hdlr", &body)
}

fn stts(track: &Track) -> Vec<u8> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for sample in &track.samples {
        match runs.last_mut() {
            Some((count, delta)) if *delta == sample.duration => *count += 1,
            _ => runs.push((1, sample.duration)),
        }
    }
    let mut body = Vec::with_capacity(8 + runs.len() * 8);
    body.extend(&0u32.to_be_bytes()); // version + flags
    body.extend(&(runs.len() as u32).to_be_bytes());
    for (count, delta) in runs {
        body.extend(&count.to_be_bytes());
        body.extend(&delta.to_be_bytes());
    }
    boxed(b"stts", &body)
}

fn stsc(track: &Track) -> Vec<u8> {
    let mut body = Vec::with_capacity(20);
    body.extend(&0u32.to_be_bytes()); // version + flags
    body.extend(&1u32.to_be_bytes()); // entry count
    body.extend(&1u32.to_be_bytes()); // first chunk
    body.extend(&(track.samples.len() as u32).to_be_bytes()); // samples per chunk
    body.extend(&1u32.to_be_bytes()); // sample description index
    boxed(b"stsc", &body)
}

fn stsz(track: &Track) -> Vec<u8> {
    let mut body = Vec::with_capacity(16 + track.samples.len() * 4);
    body.extend(&0u32.to_be_bytes()); // version + flags
    body.extend(&0u32.to_be_bytes()); // not uniform
    body.extend(&(track.samples.len() as u32).to_be_bytes());
    for sample in &track.samples {
        body.extend(&sample.size.to_be_bytes());
    }
    boxed(b"stsz", &body)
}

fn co64(_track: &Track) -> Vec<u8> {
    let mut body = Vec::with_capacity(20);
    body.extend(&0u32.to_be_bytes()); // version + flags
    body.extend(&1u32.to_be_bytes()); // entry count
    body.extend(&0u64.to_be_bytes()); // placeholder, patched later
    boxed(b"co64", &body)
}

fn stss(track: &Track) -> Vec<u8> {
    if track.handler != *b"vide" {
        return Vec::new();
    }
    if track.samples.iter().all(|sample| sample.sync) {
        return Vec::new();
    }
    let sync: Vec<u32> = track
        .samples
        .iter()
        .enumerate()
        .filter(|(_, sample)| sample.sync)
        .map(|(index, _)| (index + 1) as u32)
        .collect();
    let mut body = Vec::with_capacity(8 + sync.len() * 4);
    body.extend(&0u32.to_be_bytes()); // version + flags
    body.extend(&(sync.len() as u32).to_be_bytes());
    for number in sync {
        body.extend(&number.to_be_bytes());
    }
    boxed(b"stss", &body)
}

/// Write each track's single `co64` entry, in the order the tracks were built.
///
/// The `moov` is our own bytes and carries no media, so the only `co64` tags it
/// contains are the chunk offsets just written. Each box is sane-checked by its
/// size (24 bytes for one entry) before it is patched.
fn patch_chunk_offsets(moov: &mut [u8], offsets: &[u64]) {
    let mut patched = 0usize;
    let mut at = 0usize;
    while at + 4 <= moov.len() && patched < offsets.len() {
        if &moov[at..at + 4] == b"co64" {
            // tag (4) + version/flags (4) + entry count (4) precede the entry.
            let entry = at + 12;
            let size_ok = at >= 4
                && u32::from_be_bytes([moov[at - 4], moov[at - 3], moov[at - 2], moov[at - 1]])
                    == 24;
            if size_ok && entry + 8 <= moov.len() {
                moov[entry..entry + 8].copy_from_slice(&offsets[patched].to_be_bytes());
                patched += 1;
            }
        }
        at += 1;
    }
}

fn patch_tkhd_duration(tkhd: &mut [u8], timescale: u32, duration_units: u64) {
    let movie_units =
        (duration_units as f64 * MOVIE_TIMESCALE as f64 / timescale as f64).round() as u64;
    let version = tkhd.first().copied().unwrap_or(0);
    if version == 1 {
        tkhd[28..36].copy_from_slice(&movie_units.to_be_bytes());
    } else {
        tkhd[20..24].copy_from_slice(&(movie_units as u32).to_be_bytes());
    }
}

fn patch_mdhd_duration(mdhd: &mut [u8], duration_units: u64) {
    let version = mdhd.first().copied().unwrap_or(0);
    if version == 1 {
        mdhd[24..32].copy_from_slice(&duration_units.to_be_bytes());
    } else {
        mdhd[16..20].copy_from_slice(&(duration_units as u32).to_be_bytes());
    }
}

fn temporary_path(work_dir: &Path, output: &Path) -> PathBuf {
    let name = output
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    work_dir.join(format!(".{name}.idmmux"))
}

/* -------------------------------------------------------------- box helpers */

struct BoxHeader {
    start: u64,
    size: u64,
    tag: [u8; 4],
}

fn read_box_header(file: &mut File, at: u64, end: u64) -> Result<Option<BoxHeader>, MuxError> {
    if at + 8 > end {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(at))
        .map_err(|error| MuxError::Io(error.to_string()))?;
    let mut header = [0u8; 16];
    let read = file
        .read(&mut header)
        .map_err(|error| MuxError::Io(error.to_string()))?;
    if read < 8 {
        return Ok(None);
    }
    let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let size = if size32 == 1 {
        if read < 16 {
            return Ok(None);
        }
        u64::from_be_bytes([
            header[8], header[9], header[10], header[11], header[12], header[13], header[14],
            header[15],
        ])
    } else if size32 == 0 {
        end - at
    } else {
        size32 as u64
    };
    if size < 8 || at + size > end {
        return Err(MuxError::Io("the file is truncated".to_string()));
    }
    Ok(Some(BoxHeader {
        start: at,
        size,
        tag: [header[4], header[5], header[6], header[7]],
    }))
}

fn read_box_body(file: &mut File, boxed: &BoxHeader) -> Result<Vec<u8>, MuxError> {
    file.seek(SeekFrom::Start(boxed.start))
        .map_err(|error| MuxError::Io(error.to_string()))?;
    let mut head = [0u8; 16];
    let read = file
        .read(&mut head)
        .map_err(|error| MuxError::Io(error.to_string()))?;
    let header_len = if read >= 16 && u32::from_be_bytes([head[0], head[1], head[2], head[3]]) == 1
    {
        16u64
    } else {
        8u64
    };
    let body_len = boxed.size - header_len;
    let mut body = vec![0u8; body_len as usize];
    file.seek(SeekFrom::Start(boxed.start + header_len))
        .map_err(|error| MuxError::Io(error.to_string()))?;
    file.read_exact(&mut body)
        .map_err(|error| MuxError::Io(error.to_string()))?;
    Ok(body)
}

struct Atom {
    tag: [u8; 4],
    start: usize,
    end: usize,
}

fn atoms(bytes: &[u8]) -> Vec<Atom> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 8 <= bytes.len() {
        let size32 = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        let header = if size32 == 1 { 16usize } else { 8usize };
        let size = if size32 == 1 {
            if at + 16 > bytes.len() {
                break;
            }
            u64::from_be_bytes([
                bytes[at + 8],
                bytes[at + 9],
                bytes[at + 10],
                bytes[at + 11],
                bytes[at + 12],
                bytes[at + 13],
                bytes[at + 14],
                bytes[at + 15],
            ]) as usize
        } else if size32 == 0 {
            bytes.len() - at
        } else {
            size32 as usize
        };
        if size < header || at + size > bytes.len() {
            break;
        }
        out.push(Atom {
            tag: [bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]],
            start: at + header,
            end: at + size,
        });
        at += size;
    }
    out
}

fn find_box(bytes: &[u8], wanted: &[u8; 4]) -> Option<Atom> {
    atoms(bytes).into_iter().find(|atom| &atom.tag == wanted)
}

fn track_by_id(moov: &[u8], wanted_id: u32) -> Result<&[u8], MuxError> {
    for atom in atoms(moov) {
        if &atom.tag != b"trak" {
            continue;
        }
        let trak = &moov[atom.start..atom.end];
        if let Some(tkhd) = find_box(trak, b"tkhd") {
            if track_id(&trak[tkhd.start..tkhd.end]) == wanted_id {
                return Ok(trak);
            }
        }
    }
    Err(MuxError::Io("a track is missing".to_string()))
}

fn track_id(tkhd: &[u8]) -> u32 {
    // The slice is box content, so the full-box version byte is the first one.
    let version = tkhd.first().copied().unwrap_or(0);
    if version == 1 {
        u32::from_be_bytes([tkhd[20], tkhd[21], tkhd[22], tkhd[23]])
    } else {
        u32::from_be_bytes([tkhd[12], tkhd[13], tkhd[14], tkhd[15]])
    }
}

fn handler_kind(hdlr: &[u8]) -> [u8; 4] {
    if hdlr.len() >= 12 {
        [hdlr[8], hdlr[9], hdlr[10], hdlr[11]]
    } else {
        *b"    "
    }
}

fn mdhd_timescale(mdhd: &[u8]) -> u32 {
    let version = mdhd.first().copied().unwrap_or(0);
    if version == 1 {
        u32::from_be_bytes([mdhd[20], mdhd[21], mdhd[22], mdhd[23]])
    } else {
        u32::from_be_bytes([mdhd[12], mdhd[13], mdhd[14], mdhd[15]])
    }
}

fn sample_description(media: &[u8]) -> Result<&[u8], MuxError> {
    let minf = find_box(media, b"minf")
        .ok_or_else(|| MuxError::Io("a track has no media info".to_string()))?;
    let minf = &media[minf.start..minf.end];
    let stbl = find_box(minf, b"stbl")
        .ok_or_else(|| MuxError::Io("a track has no sample table".to_string()))?;
    let stbl = &minf[stbl.start..stbl.end];
    let stsd = find_box(stbl, b"stsd")
        .ok_or_else(|| MuxError::Io("a track has no sample description".to_string()))?;
    Ok(&stbl[stsd.start..stsd.end])
}

fn boxed(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend(&((body.len() + 8) as u32).to_be_bytes());
    out.extend(tag);
    out.extend(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// ffmpeg is a test tool here, never a runtime dependency. A machine
    /// without it skips the real-media tests rather than failing.
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
        let dir = std::env::temp_dir().join(format!("bdm-mp4-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        dir
    }

    fn make(ffmpeg: &str, args: &[&str], out: &Path) {
        let status = Command::new(ffmpeg)
            .args(args)
            .arg(out)
            .status()
            .expect("ffmpeg runs");
        assert!(status.success(), "ffmpeg produced {out:?} with {args:?}");
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
    fn joins_a_fragmented_picture_and_a_progressive_sound() {
        let Some(ffmpeg) = ffmpeg() else { return };
        let dir = folder("frag");
        let video = dir.join("video.track");
        let audio = dir.join("audio.track");
        make(
            &ffmpeg,
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=30:duration=2",
                "-an",
                "-c:v",
                "mpeg4",
                "-movflags",
                "frag_keyframe+empty_moov",
                "-f",
                "mp4",
            ],
            &video,
        );
        make(
            &ffmpeg,
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-vn",
                "-c:a",
                "aac",
                "-f",
                "mp4",
            ],
            &audio,
        );

        let out = dir.join("joined.mp4");
        join(&[video, audio], &out, &dir, Container::Mp4).expect("a join");
        decodes(&ffmpeg, &out);

        let info = super::super::verify::inspect(&out).expect("the file is an MP4");
        assert_eq!(info.video_codec.as_deref(), Some("mp4v"));
        assert_eq!(info.audio_codec.as_deref(), Some("mp4a"));
        assert!(info.duration.unwrap_or(0.0) > 1.0, "the joined file lasts");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rewrites_a_progressive_picture_alone() {
        let Some(ffmpeg) = ffmpeg() else { return };
        let dir = folder("prog");
        let video = dir.join("video.track");
        make(
            &ffmpeg,
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=30:duration=2",
                "-an",
                "-c:v",
                "mpeg4",
                "-f",
                "mp4",
            ],
            &video,
        );
        let out = dir.join("joined.mp4");
        join(&[video], &out, &dir, Container::Mp4).expect("a join");
        decodes(&ffmpeg, &out);
        let info = super::super::verify::inspect(&out).expect("the file is an MP4");
        assert_eq!(info.video_codec.as_deref(), Some("mp4v"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
