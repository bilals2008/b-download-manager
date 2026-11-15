//! MP4 box layout helpers: headers, atoms, and the boxes the reader and writer share.

use super::*;

pub(super) fn read_box_header(file: &mut File, at: u64, end: u64) -> Result<Option<BoxHeader>, MuxError> {
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

pub(super) fn read_box_body(file: &mut File, boxed: &BoxHeader) -> Result<Vec<u8>, MuxError> {
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
pub(super) fn atoms(bytes: &[u8]) -> Vec<Atom> {
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

pub(super) fn find_box(bytes: &[u8], wanted: &[u8; 4]) -> Option<Atom> {
    atoms(bytes).into_iter().find(|atom| &atom.tag == wanted)
}

pub(super) fn track_by_id(moov: &[u8], wanted_id: u32) -> Result<&[u8], MuxError> {
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

pub(super) fn track_id(tkhd: &[u8]) -> u32 {
    // The slice is box content, so the full-box version byte is the first one.
    let version = tkhd.first().copied().unwrap_or(0);
    if version == 1 {
        u32::from_be_bytes([tkhd[20], tkhd[21], tkhd[22], tkhd[23]])
    } else {
        u32::from_be_bytes([tkhd[12], tkhd[13], tkhd[14], tkhd[15]])
    }
}

pub(super) fn handler_kind(hdlr: &[u8]) -> [u8; 4] {
    if hdlr.len() >= 12 {
        [hdlr[8], hdlr[9], hdlr[10], hdlr[11]]
    } else {
        *b"    "
    }
}

pub(super) fn mdhd_timescale(mdhd: &[u8]) -> u32 {
    let version = mdhd.first().copied().unwrap_or(0);
    if version == 1 {
        u32::from_be_bytes([mdhd[20], mdhd[21], mdhd[22], mdhd[23]])
    } else {
        u32::from_be_bytes([mdhd[12], mdhd[13], mdhd[14], mdhd[15]])
    }
}

pub(super) fn sample_description(media: &[u8]) -> Result<&[u8], MuxError> {
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

pub(super) fn boxed(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend(&((body.len() + 8) as u32).to_be_bytes());
    out.extend(tag);
    out.extend(body);
    out
}
