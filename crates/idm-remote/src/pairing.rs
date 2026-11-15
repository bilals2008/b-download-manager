//! The one-time code a phone is paired with.
//!
//! The reader sees the code in the application and types it on the phone, or
//! the phone reads it from a QR code. It is good for a few minutes and for one
//! phone only: the moment it is exchanged it is gone.

use std::time::{Duration, SystemTime};

use subtle::ConstantTimeEq;

use crate::random;

/// How long a code stays good.
const CODE_LIFETIME: Duration = Duration::from_secs(300);

/// How many digits the code has.
const CODE_DIGITS: usize = 8;

/// A code the reader shows and the phone offers once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingCode {
    pub code: String,
    pub expires_at: SystemTime,
}

impl PairingCode {
    /// Whether the code is too old to be used.
    pub fn is_expired(&self) -> bool {
        SystemTime::now() >= self.expires_at
    }

    /// Whether this is the code the phone offered, and it is still good.
    pub fn accepts(&self, offered: &str) -> bool {
        !self.is_expired() && verify_pairing(&self.code, offered)
    }
}

/// A new code, good for a few minutes.
///
/// Without a random source there is no code worth showing, so the one that is
/// handed back is already expired and the reader is asked for another.
pub fn new_pairing_code() -> PairingCode {
    match random::digits(CODE_DIGITS) {
        Some(code) => PairingCode {
            code,
            expires_at: SystemTime::now() + CODE_LIFETIME,
        },
        None => PairingCode {
            code: String::new(),
            expires_at: SystemTime::now(),
        },
    }
}

/// Whether the offered code is the code.
///
/// The comparison takes the same time for every difference, so a caller
/// cannot learn the code one digit at a time by watching how long it takes.
pub fn verify_pairing(code: &str, offered: &str) -> bool {
    code.as_bytes().ct_eq(offered.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_right_code_is_accepted() {
        let code = new_pairing_code();
        assert!(code.accepts(&code.code));
        assert!(verify_pairing("12345678", "12345678"));
    }

    #[test]
    fn a_wrong_code_is_refused() {
        let code = new_pairing_code();
        assert!(!code.accepts("00000000"));
        assert!(!verify_pairing("12345678", "12345679"));
        assert!(!verify_pairing("12345678", "1234567"));
        assert!(!verify_pairing("12345678", ""));
    }

    #[test]
    fn an_expired_code_is_refused() {
        let code = PairingCode {
            code: "12345678".to_string(),
            expires_at: SystemTime::now() - Duration::from_secs(1),
        };
        assert!(code.is_expired());
        assert!(!code.accepts("12345678"));
    }

    #[test]
    fn a_new_code_has_eight_digits_and_a_future() {
        let code = new_pairing_code();
        assert_eq!(code.code.len(), CODE_DIGITS);
        assert!(code.code.chars().all(|c| c.is_ascii_digit()));
        assert!(!code.is_expired());
    }
}
