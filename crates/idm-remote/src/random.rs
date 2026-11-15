//! The random values the remote hands out.
//!
//! Every value here is a secret, so none of it comes from a seeded generator:
//! it comes from the operating system's random source, which is the only one
//! that cannot be guessed from what the machine has done before.

/// Random bytes, or `None` when the operating system has none to give.
pub fn bytes(count: usize) -> Option<Vec<u8>> {
    let mut buffer = vec![0u8; count];
    match getrandom::fill(&mut buffer) {
        Ok(()) => Some(buffer),
        Err(_) => None,
    }
}

/// Random bytes as hex.
pub fn hex(count: usize) -> Option<String> {
    bytes(count).map(|bytes| {
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push(char::from(DIGITS[usize::from(byte >> 4)]));
            out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
        }
        out
    })
}

/// Random digits, without the bias a plain modulo would leave behind.
///
/// The bytes above 250 are thrown away rather than folded in, so every digit
/// is equally likely.
pub fn digits(count: usize) -> Option<String> {
    let mut out = String::with_capacity(count);
    let mut buffer = [0u8; 64];
    while out.len() < count {
        if getrandom::fill(&mut buffer).is_err() {
            return None;
        }
        for byte in buffer {
            if byte < 250 {
                out.push(char::from(b'0' + byte % 10));
                if out.len() == count {
                    break;
                }
            }
        }
    }
    Some(out)
}

/// The digits a hex number is written with.
const DIGITS: &[u8; 16] = b"0123456789abcdef";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digits_are_digits() {
        let code = digits(8).expect("the random source is there");
        assert_eq!(code.len(), 8);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn the_hex_is_hex() {
        let value = hex(16).expect("the random source is there");
        assert_eq!(value.len(), 32);
        assert!(value.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
