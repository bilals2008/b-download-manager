//! Reading the samples of a fragmented MP4 (`moof`/`trun`).

use super::boxes::*;
use super::*;

/* -------------------------------------------------------- fragmented reads */

/// Read the samples of one track out of its fragments.
///
/// Every `traf` for the track is read, and every `trun` inside it: a fragment
/// may split its samples into several runs, each with its own data offset, and
/// reading only the first one would silently drop the rest of the fragment.
/// Nothing indexes without a length check — the release profile aborts on a
/// panic, so a truncated fragment must become an error, not a crash.
pub(super) fn fragment_samples(
    fragments: &[Fragment],
    moov: &[u8],
    track_id: u32,
) -> Result<Vec<Sample>, MuxError> {
    let defaults = trex_defaults(moov, track_id);
    let mut samples = Vec::new();
    // The decode clock carries across fragments. A `tfdt` overrides it, and a
    // gap it names is preserved.
    let mut decode_clock = 0u64;
    for fragment in fragments {
        for atom in atoms(&fragment.moof) {
            if &atom.tag != b"traf" {
                continue;
            }
            let traf = &fragment.moof[atom.start..atom.end];
            let tfhd = find_box(traf, b"tfhd")
                .ok_or_else(|| MuxError::Io("a fragment has no header".to_string()))?;
            let tfhd = parse_tfhd(&traf[tfhd.start..tfhd.end], fragment.base, defaults)?;
            if tfhd.track_id != track_id {
                continue;
            }
            let mut decode_time = match find_box(traf, b"tfdt") {
                Some(body) => base_media_decode_time(&traf[body.start..body.end])?,
                None => decode_clock,
            };
            let mut data_offset = tfhd.base;
            let mut first_run = true;
            let mut current_flags = tfhd.flags;
            for run in atoms(traf) {
                if &run.tag != b"trun" {
                    continue;
                }
                let mut cursor = Cursor::new(&traf[run.start..run.end]);
                let version = cursor.u8()?;
                if version > 1 {
                    return Err(MuxError::Io(format!(
                        "a fragment's sample run names version {version}, which this build cannot read"
                    )));
                }
                let flags = cursor.u24()?;
                let count = cursor.u32()? as usize;
                // A run without its own data offset continues where the
                // previous run stopped; the first run of a `traf` starts at
                // the base the `tfhd` named.
                if flags & 0x1 != 0 {
                    let offset = cursor.i32()? as i64;
                    data_offset = add_signed(tfhd.base, offset)?;
                } else if first_run {
                    data_offset = tfhd.base;
                }
                let first_flags = if flags & 0x4 != 0 {
                    Some(cursor.u32()?)
                } else {
                    None
                };
                let mut running = 0u64;
                for index in 0..count {
                    let duration = if flags & 0x100 != 0 {
                        cursor.u32()?
                    } else {
                        tfhd.duration
                    };
                    let size = if flags & 0x200 != 0 {
                        cursor.u32()?
                    } else {
                        tfhd.size
                    };
                    let sample_flags = if flags & 0x400 != 0 {
                        cursor.u32()?
                    } else if index == 0 {
                        first_flags.unwrap_or(current_flags)
                    } else {
                        current_flags
                    };
                    let composition_offset = if flags & 0x800 != 0 {
                        // Version 0 states it unsigned, version 1 signed.
                        if version == 1 {
                            cursor.i32()?
                        } else {
                            cursor.u32()? as i32
                        }
                    } else {
                        0
                    };
                    let offset = data_offset.checked_add(running).ok_or_else(|| {
                        MuxError::Io("a sample offset runs past the file".to_string())
                    })?;
                    samples.push(Sample {
                        offset,
                        size,
                        duration,
                        decode_time,
                        composition_offset,
                        sync: sample_flags & 0x0001_0000 == 0,
                    });
                    decode_time = decode_time.saturating_add(duration as u64);
                    running = running.saturating_add(size as u64);
                    current_flags = sample_flags;
                }
                data_offset = data_offset.saturating_add(running);
                first_run = false;
            }
            decode_clock = decode_time;
        }
    }
    Ok(samples)
}

/// A small forward reader that refuses to run off the end of a box.
struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], MuxError> {
        let end = self
            .at
            .checked_add(count)
            .ok_or_else(|| MuxError::Io("a box's length overflows".to_string()))?;
        let slice = self
            .data
            .get(self.at..end)
            .ok_or_else(|| MuxError::Io("a box ends before the fields it promises".to_string()))?;
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, MuxError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, MuxError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// The 24 flag bits that follow a full box's version byte.
    fn u24(&mut self) -> Result<u32, MuxError> {
        let bytes = self.take(3)?;
        Ok(u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]))
    }

    fn i32(&mut self) -> Result<i32, MuxError> {
        let bytes = self.take(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn u64(&mut self) -> Result<u64, MuxError> {
        let bytes = self.take(8)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }
}

fn add_signed(base: u64, delta: i64) -> Result<u64, MuxError> {
    let value = base as i128 + delta as i128;
    if value < 0 || value > u64::MAX as i128 {
        return Err(MuxError::Io(
            "a sample offset points outside the file".to_string(),
        ));
    }
    Ok(value as u64)
}

/// The defaults a track's `trex` states for its fragments.
#[derive(Debug, Clone, Copy, Default)]
struct FragmentDefaults {
    duration: u32,
    size: u32,
    flags: u32,
}

fn trex_defaults(moov: &[u8], track_id: u32) -> FragmentDefaults {
    let Some(mvex) = find_box(moov, b"mvex") else {
        return FragmentDefaults::default();
    };
    let mvex = &moov[mvex.start..mvex.end];
    for atom in atoms(mvex) {
        if &atom.tag != b"trex" {
            continue;
        }
        let body = &mvex[atom.start..atom.end];
        if body.len() < 24 {
            continue;
        }
        let id = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
        if id != track_id {
            continue;
        }
        return FragmentDefaults {
            duration: u32::from_be_bytes([body[12], body[13], body[14], body[15]]),
            size: u32::from_be_bytes([body[16], body[17], body[18], body[19]]),
            flags: u32::from_be_bytes([body[20], body[21], body[22], body[23]]),
        };
    }
    FragmentDefaults::default()
}

/// A `tfhd`, in the form the sample reader needs.
struct Tfhd {
    track_id: u32,
    /// Where the first run's data starts.
    base: u64,
    duration: u32,
    size: u32,
    flags: u32,
}

/// Read a `tfhd`, refusing a truncated one instead of indexing past it.
///
/// Every optional field is chosen by a flag bit, so a header that ends before
/// the fields it promises cannot be read; that is an error rather than a
/// default, because guessing a duration would produce a plausible wrong file.
fn parse_tfhd(body: &[u8], moof_start: u64, defaults: FragmentDefaults) -> Result<Tfhd, MuxError> {
    let mut cursor = Cursor::new(body);
    let _version = cursor.u8()?;
    let flags = cursor.u24()?;
    let track_id = cursor.u32()?;
    // `base-data-offset-present` names an absolute file offset;
    // `default-base-is-moof` and the implicit form both mean "the moof".
    let base = if flags & 0x1 != 0 {
        cursor.u64()?
    } else {
        moof_start
    };
    if flags & 0x2 != 0 {
        let _description_index = cursor.u32()?;
    }
    let duration = if flags & 0x8 != 0 {
        cursor.u32()?
    } else {
        defaults.duration
    };
    let size = if flags & 0x10 != 0 {
        cursor.u32()?
    } else {
        defaults.size
    };
    let flags_of_sample = if flags & 0x20 != 0 {
        cursor.u32()?
    } else {
        defaults.flags
    };
    Ok(Tfhd {
        track_id,
        base,
        duration,
        size,
        flags: flags_of_sample,
    })
}

/// The decode time a fragment says it begins at (`tfdt`).
fn base_media_decode_time(body: &[u8]) -> Result<u64, MuxError> {
    let mut cursor = Cursor::new(body);
    let version = cursor.u8()?;
    let _flags = cursor.u24()?;
    match version {
        0 => Ok(cursor.u32()? as u64),
        1 => cursor.u64(),
        other => Err(MuxError::Io(format!(
            "a fragment names decode-time version {other}, which this build cannot read"
        ))),
    }
}
