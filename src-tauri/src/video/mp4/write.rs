//! Writing the joined progressive MP4: `ftyp`, `moov`, `mdat`.

use super::*;
use super::boxes::boxed;
use super::super::mux::is_cancelled;

/* ---------------------------------------------------------------- writing */

pub(super) fn write_joined(
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
            if is_cancelled(output) {
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

pub(super) fn build_moov(tracks: &[Track]) -> Vec<u8> {
    let mut children: Vec<Vec<u8>> = Vec::new();
    children.push(mvhd(tracks));
    for (index, track) in tracks.iter().enumerate() {
        children.push(trak(track, index as u32 + 1));
    }
    boxed(b"moov", &children.concat())
}

fn mvhd(tracks: &[Track]) -> Vec<u8> {
    let duration = tracks
        .iter()
        .map(|track| {
            (track.presentation_units() as f64 * MOVIE_TIMESCALE as f64 / track.timescale as f64)
                .round() as u64
        })
        .max()
        .unwrap_or(0);
    // Two source files very often number their only track `1`, so the output's
    // ids are simply the order of its tracks.
    let next_track_id = tracks.len() as u32 + 1;
    let version = duration > u32::MAX as u64;
    let mut body = Vec::with_capacity(112);
    let version_word: u32 = if version { 0x0100_0000 } else { 0 };
    body.extend(&version_word.to_be_bytes());
    body.extend(&0u32.to_be_bytes()); // creation time
    body.extend(&0u32.to_be_bytes()); // modification time
    body.extend(&MOVIE_TIMESCALE.to_be_bytes());
    if version {
        body.extend(&duration.to_be_bytes());
    } else {
        body.extend(&(duration as u32).to_be_bytes());
    }
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

fn trak(track: &Track, id: u32) -> Vec<u8> {
    let mut table: Vec<Vec<u8>> = vec![boxed(b"stsd", &track.stsd), stts(track)];
    // A stream whose pictures are presented out of decode order needs its
    // offsets, or every player presents them in the wrong order.
    if track.reorders() {
        table.push(ctts(track));
    }
    table.push(stsc(track));
    table.push(stsz(track));
    table.push(co64(track));
    let sync = stss(track);
    if !sync.is_empty() {
        table.push(sync);
    }
    let stbl = boxed(b"stbl", &table.concat());
    let minf = boxed(b"minf", &stbl);
    let mdia = boxed(
        b"mdia",
        &[boxed(b"mdhd", &track.mdhd), hdlr(track.handler), minf].concat(),
    );
    let mut children = Vec::new();
    // The track's own number is replaced: two files that each call their only
    // track `1` would otherwise produce a movie with two track 1s.
    let mut tkhd = track.tkhd.clone();
    patch_track_id(&mut tkhd, id);
    children.push(boxed(b"tkhd", &tkhd));
    let edit = edts(track);
    if !edit.is_empty() {
        children.push(edit);
    }
    children.push(mdia);
    boxed(b"trak", &children.concat())
}

/// An edit list that puts the media back at the time the source gave it.
///
/// A fragment series that begins at 5 s of the source's timeline is not a
/// stream that starts at zero; a leading empty edit says so, so a player does
/// not slide it forward. A source that starts at zero needs no edit at all.
pub(super) fn edts(track: &Track) -> Vec<u8> {
    let shift = track.media_shift();
    if shift == 0 {
        return Vec::new();
    }
    let movie = |units: u64| -> u64 {
        (units as f64 * MOVIE_TIMESCALE as f64 / track.timescale.max(1) as f64).round() as u64
    };
    let mut entries: Vec<u8> = Vec::new();
    // An empty edit: `media_time` of -1 moves the media without showing any.
    entries.extend(&(movie(shift).min(u32::MAX as u64) as u32).to_be_bytes());
    entries.extend(&(-1i32).to_be_bytes());
    entries.extend(&0x0001_0000u32.to_be_bytes()); // rate 1.0
    entries.extend(&(movie(track.duration_units()).min(u32::MAX as u64) as u32).to_be_bytes());
    entries.extend(&0i32.to_be_bytes());
    entries.extend(&0x0001_0000u32.to_be_bytes());
    let mut body = Vec::with_capacity(16);
    body.extend(&0u32.to_be_bytes()); // version 0 + flags
    body.extend(&2u32.to_be_bytes()); // entry count
    body.extend(&entries);
    boxed(b"edts", &boxed(b"elst", &body))
}

/// The composition offsets, run-length encoded the way `ctts` states them.
pub(super) fn ctts(track: &Track) -> Vec<u8> {
    let mut runs: Vec<(u32, i32)> = Vec::new();
    for sample in &track.samples {
        match runs.last_mut() {
            Some((count, offset)) if *offset == sample.composition_offset => *count += 1,
            _ => runs.push((1, sample.composition_offset)),
        }
    }
    let mut body = Vec::with_capacity(8 + runs.len() * 8);
    // Version 1, because a B-frame's offset is signed.
    body.extend(&0x0100_0000u32.to_be_bytes());
    body.extend(&(runs.len() as u32).to_be_bytes());
    for (count, offset) in runs {
        body.extend(&count.to_be_bytes());
        body.extend(&offset.to_be_bytes());
    }
    boxed(b"ctts", &body)
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

pub(super) fn patch_tkhd_duration(tkhd: &mut [u8], timescale: u32, duration_units: u64) {
    let movie_units =
        (duration_units as f64 * MOVIE_TIMESCALE as f64 / timescale.max(1) as f64).round() as u64;
    let version = tkhd.first().copied().unwrap_or(0);
    if version == 1 {
        // version(1) flags(3) creation(8) modification(8) track_id(4)
        // reserved(4) duration(8) — the duration starts at byte 28.
        if tkhd.len() >= 36 {
            tkhd[28..36].copy_from_slice(&movie_units.to_be_bytes());
        }
    } else if tkhd.len() >= 24 {
        // version(1) flags(3) creation(4) modification(4) track_id(4)
        // reserved(4) duration(4) — the duration starts at byte 20, and a
        // value that no longer fits is clamped rather than truncated.
        tkhd[20..24].copy_from_slice(&(movie_units.min(u32::MAX as u64) as u32).to_be_bytes());
    }
}

pub(super) fn patch_mdhd_duration(mdhd: &mut [u8], duration_units: u64) {
    let version = mdhd.first().copied().unwrap_or(0);
    if version == 1 {
        // version(1) flags(3) creation(8) modification(8) timescale(4)
        // duration(8) — the duration starts at byte 24.
        if mdhd.len() >= 32 {
            mdhd[24..32].copy_from_slice(&duration_units.to_be_bytes());
        }
    } else if mdhd.len() >= 20 {
        mdhd[16..20].copy_from_slice(&(duration_units.min(u32::MAX as u64) as u32).to_be_bytes());
    }
}

/// Give a copied `tkhd` the track id the output uses.
fn patch_track_id(tkhd: &mut [u8], id: u32) {
    let version = tkhd.first().copied().unwrap_or(0);
    if version == 1 {
        if tkhd.len() >= 24 {
            tkhd[20..24].copy_from_slice(&id.to_be_bytes());
        }
    } else if tkhd.len() >= 16 {
        tkhd[12..16].copy_from_slice(&id.to_be_bytes());
    }
}

fn temporary_path(work_dir: &Path, output: &Path) -> PathBuf {
    let name = output
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    work_dir.join(format!(".{name}.idmmux"))
}
