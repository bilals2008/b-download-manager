//! The little conversions the rest of the crate needs: hex for the hashes,
//! percent-decoding for the magnet links, base32 for the magnets that carry
//! their info hash in it and base64 for the piece map the window draws.

/// The bytes as hex, in lower case.
pub fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

/// The bytes a hex text stands for, or nothing when the text is not hex.
pub fn hex_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in bytes.chunks(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        out.push((high * 16 + low) as u8);
    }
    Some(out)
}

/// The bytes as base64 (RFC 4648, with the padding).
///
/// The piece map of a big torrent is a bitmap of a few thousand bytes, and
/// base64 carries it in a third less than hex does.
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0] as u32;
        let second = *chunk.get(1).unwrap_or(&0) as u32;
        let third = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (first << 16) | (second << 8) | third;
        out.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(triple >> 6) as usize & 0x3f] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[triple as usize & 0x3f] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// The text a percent-encoded string stands for.
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                match (high, low) {
                    (Some(high), Some(low)) => {
                        out.push((high * 16 + low) as u8);
                        index += 3;
                    }
                    _ => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            other => {
                out.push(other);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The bytes a base32 text stands for, or nothing when the text is not
/// base32 (RFC 4648, without the padding).
pub fn base32_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let mut value: u32 = 0;
    let mut bits: u32 = 0;
    for character in text.trim().bytes() {
        if character == b'=' {
            break;
        }
        let digit = match character {
            b'A'..=b'Z' => character - b'A',
            b'2'..=b'7' => character - b'2' + 26,
            _ => return None,
        };
        value = (value << 5) | digit as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((value >> bits) as u8);
            value &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The bytes of a peer id that is not random but the same on every run of
/// the program: two runs of the download manager should not look like two
/// different people to a tracker.
pub fn peer_id() -> [u8; 20] {
    let mut seed = Vec::new();
    seed.extend_from_slice(b"idm-torrent");
    seed.extend_from_slice(&std::process::id().to_le_bytes());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    seed.extend_from_slice(&nanos.to_le_bytes());
    let mut out = [0u8; 20];
    out.copy_from_slice(&sha1_bytes(&seed));
    out
}

/// The SHA-1 of the bytes.
pub fn sha1_bytes(data: &[u8]) -> [u8; 20] {
    use sha1::Digest;
    let mut hasher = sha1::Sha1::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut out = [0u8; 20];
    out.copy_from_slice(&digest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_goes_both_ways() {
        assert_eq!(hex_encode(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(hex_decode("000FFF").unwrap(), vec![0x00, 0x0f, 0xff]);
        assert!(hex_decode("0f0").is_none());
        assert!(hex_decode("zz").is_none());
    }

    #[test]
    fn percent_escapes_are_read() {
        assert_eq!(percent_decode("Big%20Buck"), "Big Buck");
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn base32_is_read() {
        // The base32 of 20 zero bytes.
        assert_eq!(
            base32_decode("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap(),
            vec![0u8; 20]
        );
        assert!(base32_decode("0189").is_none());
    }

    #[test]
    fn base64_carries_the_bytes() {
        assert_eq!(base64_encode(&[]), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(&[0xff, 0xfe, 0xfd]), "//79");
    }

    #[test]
    fn a_peer_id_is_twenty_bytes() {
        assert_eq!(peer_id().len(), 20);
    }
}
