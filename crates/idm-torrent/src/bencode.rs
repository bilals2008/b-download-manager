//! Bencode, the format a `.torrent` file is written in.
//!
//! A dictionary keeps its keys in the order they were read, because the info
//! hash is the hash of the exact bytes of the info dictionary and a torrent
//! has to be written back the way it came.

use serde::Serialize;

use crate::error::TorrentError;

/// The entries of a dictionary and the bytes each value took in the stream,
/// which is what the info hash is cut out of.
type DictEntries = (Vec<(String, BencodeValue)>, Vec<(usize, usize)>);

/// A value read from, or written to, a bencode stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BencodeValue {
    /// A string of bytes.
    Bytes(Vec<u8>),
    /// A whole number.
    Integer(i64),
    /// A list of values.
    List(Vec<BencodeValue>),
    /// A dictionary: the keys and the values, in the order they were read.
    Dict(Vec<(String, BencodeValue)>),
}

impl BencodeValue {
    /// The value under a key, when this is a dictionary that has it.
    pub fn get(&self, key: &str) -> Option<&BencodeValue> {
        match self {
            BencodeValue::Dict(entries) => entries
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The bytes, when this is a string.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            BencodeValue::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// The string, when this is a string that is text.
    pub fn as_text(&self) -> Option<String> {
        self.as_bytes()
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    }

    /// The number, when this is a number.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            BencodeValue::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// The values, when this is a list.
    pub fn as_list(&self) -> Option<&[BencodeValue]> {
        match self {
            BencodeValue::List(values) => Some(values),
            _ => None,
        }
    }

    /// The entries, when this is a dictionary.
    pub fn as_dict(&self) -> Option<&[(String, BencodeValue)]> {
        match self {
            BencodeValue::Dict(entries) => Some(entries),
            _ => None,
        }
    }
}

/// Read a bencode stream.
pub fn bencode_decode(bytes: &[u8]) -> Result<BencodeValue, TorrentError> {
    let mut decoder = Decoder { bytes, at: 0 };
    let value = decoder.value()?;
    Ok(value)
}

/// Read a bencode stream and say where the info dictionary was, so the info
/// hash can be taken from the bytes of the file as they came.
pub(crate) fn decode_with_info_span(
    bytes: &[u8],
) -> Result<(BencodeValue, Option<(usize, usize)>), TorrentError> {
    let mut decoder = Decoder { bytes, at: 0 };
    if decoder.byte()? != b'd' {
        return Err(TorrentError::Malformed(
            "it does not start with a dictionary".into(),
        ));
    }
    let (entries, spans) = decoder.dict_entries()?;
    let mut span = None;
    for (index, (key, _)) in entries.iter().enumerate() {
        if key == "info" {
            span = spans.get(index).copied();
        }
    }
    Ok((BencodeValue::Dict(entries), span))
}

/// Read one value and say how many bytes it took, so a message whose
/// dictionary leads a raw blob can be split in one pass.
pub(crate) fn decode_value_with_end(bytes: &[u8]) -> Result<(BencodeValue, usize), TorrentError> {
    let mut decoder = Decoder { bytes, at: 0 };
    let value = decoder.value()?;
    Ok((value, decoder.at))
}

/// Write a value as bencode.
pub fn bencode_encode(value: &BencodeValue) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(value, &mut out);
    out
}

fn encode_into(value: &BencodeValue, out: &mut Vec<u8>) {
    match value {
        BencodeValue::Bytes(bytes) => {
            out.extend_from_slice(bytes.len().to_string().as_bytes());
            out.push(b':');
            out.extend_from_slice(bytes);
        }
        BencodeValue::Integer(number) => {
            out.push(b'i');
            out.extend_from_slice(number.to_string().as_bytes());
            out.push(b'e');
        }
        BencodeValue::List(values) => {
            out.push(b'l');
            for value in values {
                encode_into(value, out);
            }
            out.push(b'e');
        }
        BencodeValue::Dict(entries) => {
            // Bencode wants the keys in order, so the bytes of a torrent do
            // not change when it is written again.
            let mut entries: Vec<&(String, BencodeValue)> = entries.iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            out.push(b'd');
            for (key, value) in entries {
                encode_into(&BencodeValue::Bytes(key.as_bytes().to_vec()), out);
                encode_into(value, out);
            }
            out.push(b'e');
        }
    }
}

/// Reads a bencode stream, one value at a time.
struct Decoder<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Decoder<'a> {
    fn byte(&mut self) -> Result<u8, TorrentError> {
        let byte = *self
            .bytes
            .get(self.at)
            .ok_or_else(|| TorrentError::Malformed("it ends too soon".into()))?;
        self.at += 1;
        Ok(byte)
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], TorrentError> {
        let end = self
            .at
            .checked_add(count)
            .ok_or_else(|| TorrentError::Malformed("it is too long".into()))?;
        if end > self.bytes.len() {
            return Err(TorrentError::Malformed("it ends too soon".into()));
        }
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn value(&mut self) -> Result<BencodeValue, TorrentError> {
        match self.byte()? {
            b'i' => self.integer(),
            b'l' => self.list(),
            b'd' => self.dict(),
            b'0'..=b'9' => self.string(),
            other => Err(TorrentError::Malformed(format!(
                "it starts with {}",
                describe(other)
            ))),
        }
    }

    /// A value and the bytes it took, so the info hash can be taken from the
    /// file as it is.
    fn value_with_span(&mut self) -> Result<(BencodeValue, usize, usize), TorrentError> {
        let start = self.at;
        let value = self.value()?;
        Ok((value, start, self.at))
    }

    fn integer(&mut self) -> Result<BencodeValue, TorrentError> {
        let end = self
            .bytes
            .iter()
            .skip(self.at)
            .position(|byte| *byte == b'e')
            .ok_or_else(|| TorrentError::Malformed("a number has no end".into()))?;
        let text = std::str::from_utf8(&self.bytes[self.at..self.at + end])
            .map_err(|_| TorrentError::Malformed("a number is not text".into()))?;
        let number = text
            .parse::<i64>()
            .map_err(|_| TorrentError::Malformed(format!("{text} is not a number")))?;
        self.at += end + 1;
        Ok(BencodeValue::Integer(number))
    }

    fn string(&mut self) -> Result<BencodeValue, TorrentError> {
        // The length was read as the first digit, so it is read again here.
        let start = self.at - 1;
        let colon = self
            .bytes
            .iter()
            .skip(start)
            .position(|byte| *byte == b':')
            .ok_or_else(|| TorrentError::Malformed("a string has no length".into()))?;
        let text = std::str::from_utf8(&self.bytes[start..start + colon])
            .map_err(|_| TorrentError::Malformed("a length is not text".into()))?;
        let length = text
            .parse::<usize>()
            .map_err(|_| TorrentError::Malformed(format!("{text} is not a length")))?;
        self.at = start + colon + 1;
        let bytes = self.take(length)?.to_vec();
        Ok(BencodeValue::Bytes(bytes))
    }

    fn list(&mut self) -> Result<BencodeValue, TorrentError> {
        let mut values = Vec::new();
        loop {
            match self.byte()? {
                b'e' => return Ok(BencodeValue::List(values)),
                _ => {
                    self.at -= 1;
                    values.push(self.value()?);
                }
            }
        }
    }

    fn dict(&mut self) -> Result<BencodeValue, TorrentError> {
        Ok(BencodeValue::Dict(self.dict_entries()?.0))
    }

    /// The entries of a dictionary and the bytes each value took.
    fn dict_entries(&mut self) -> Result<DictEntries, TorrentError> {
        let mut entries = Vec::new();
        let mut spans = Vec::new();
        loop {
            match self.byte()? {
                b'e' => return Ok((entries, spans)),
                _ => {
                    self.at -= 1;
                    let (key, _, _) = self.value_with_span()?;
                    let key = match key {
                        BencodeValue::Bytes(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                        _ => return Err(TorrentError::Malformed("a key is not a string".into())),
                    };
                    let (value, start, end) = self.value_with_span()?;
                    entries.push((key, value));
                    spans.push((start, end));
                }
            }
        }
    }
}

/// The name of a byte in a sentence.
fn describe(byte: u8) -> String {
    if byte.is_ascii_graphic() {
        format!("'{}'", byte as char)
    } else {
        format!("the byte {byte}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_goes_out_and_comes_back() {
        let value = BencodeValue::Dict(vec![
            (
                "announce".into(),
                BencodeValue::Bytes(b"http://tracker/".to_vec()),
            ),
            ("info".into(), BencodeValue::Integer(42)),
        ]);
        let bytes = bencode_encode(&value);
        assert_eq!(bytes, b"d8:announce15:http://tracker/4:infoi42ee".to_vec());
        assert_eq!(bencode_decode(&bytes).unwrap(), value);
    }

    #[test]
    fn a_list_and_a_string_are_read() {
        let value = bencode_decode(b"l4:spami3ee").unwrap();
        let list = value.as_list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].as_text().unwrap(), "spam");
        assert_eq!(list[1].as_int().unwrap(), 3);
    }

    #[test]
    fn a_broken_stream_is_refused() {
        for broken in [
            &b"d3:foo"[..],
            &b"i12"[..],
            &b"3:ab"[..],
            &b"x"[..],
            &b"d3:fooi1e"[..],
        ] {
            let error = bencode_decode(broken).unwrap_err();
            assert!(!error.to_string().is_empty());
        }
    }
}
