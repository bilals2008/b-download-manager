//! EBML primitives: reading and writing the variable-length elements both the reader and the muxer share.

use super::*;

pub(super) fn element(id_bytes: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 9);
    out.extend(id(id_bytes));
    out.extend(encode_size(payload.len() as u64));
    out.extend(payload);
    out
}

pub(super) fn uint_element(id_bytes: u64, value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let first = bytes.iter().position(|byte| *byte != 0).unwrap_or(7);
    element(id_bytes, &bytes[first..])
}

pub(super) fn string_element(id_bytes: u64, value: &[u8]) -> Vec<u8> {
    element(id_bytes, value)
}

pub(super) fn id(value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let first = bytes.iter().position(|byte| *byte != 0).unwrap_or(7);
    bytes[first..].to_vec()
}

pub(super) fn unknown_size() -> Vec<u8> {
    vec![0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
}

pub(super) fn encode_size(value: u64) -> Vec<u8> {
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

pub(super) fn encode_vint(value: u64) -> Vec<u8> {
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
pub(super) fn read_header(file: &mut File, at: u64, end: u64) -> Result<Header, MuxError> {
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

pub(super) fn read_body(file: &mut File, header: &Header) -> Result<Vec<u8>, MuxError> {
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

pub(super) fn read_prefix(
    file: &mut File,
    at: u64,
    len: u64,
    end: u64,
) -> Result<Option<Vec<u8>>, MuxError> {
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

pub(super) fn elements(bytes: &[u8]) -> Vec<ElementRange> {
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

pub(super) fn element_body(bytes: &[u8], wanted: u64) -> Option<&[u8]> {
    let atom = elements(bytes).into_iter().find(|atom| atom.id == wanted)?;
    Some(&bytes[atom.data..atom.end])
}

pub(super) fn element_bytes(bytes: &[u8], wanted: u64) -> Option<Vec<u8>> {
    element_body(bytes, wanted).map(|body| body.to_vec())
}

pub(super) fn element_uint(bytes: &[u8], wanted: u64) -> Option<u64> {
    let body = element_body(bytes, wanted)?;
    read_uint(body)
}

pub(super) fn element_float(bytes: &[u8], wanted: u64) -> Option<f64> {
    let body = element_body(bytes, wanted)?;
    match body.len() {
        4 => Some(f32::from_be_bytes([body[0], body[1], body[2], body[3]]) as f64),
        8 => Some(f64::from_be_bytes([
            body[0], body[1], body[2], body[3], body[4], body[5], body[6], body[7],
        ])),
        _ => None,
    }
}

pub(super) fn read_uint(bytes: &[u8]) -> Option<u64> {
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
pub(super) fn read_int(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() || bytes.len() > 8 {
        return None;
    }
    let mut value = if bytes[0] & 0x80 != 0 { -1i64 } else { 0i64 };
    for byte in bytes {
        value = (value << 8) | *byte as i64;
    }
    Some(value)
}

pub(super) fn read_id(bytes: &[u8]) -> Option<(u64, usize)> {
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

pub(super) fn read_size(bytes: &[u8]) -> Option<(Option<usize>, usize)> {
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
pub(super) fn read_vint(bytes: &[u8]) -> Option<(u64, usize)> {
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

pub(super) fn write_error(error: std::io::Error) -> MuxError {
    MuxError::Io(format!("the file could not be written: {error}"))
}
