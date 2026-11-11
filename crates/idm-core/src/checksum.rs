//! Checksum verification of a finished file.
//!
//! The reader pastes a hash the publisher gave, and the file is checked
//! against it. The algorithms are the ones the reference download manager
//! offers.
//!
//! The file is read in pieces, so a file of any size is checked without
//! sitting in memory whole.

use std::fmt;
use std::io::Read;
use std::path::Path;

use sha2::Digest;

/// The algorithms a hash can be given in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// The checksum archives carry; it catches a broken transfer, not a
    /// broken file.
    Crc32,
    /// Broken for anything but accidents, but publishers still give it.
    Md5,
    /// Broken for anything but accidents, but publishers still give it.
    Sha1,
    /// What a publisher gives today.
    Sha256,
    /// The strongest of the five.
    Sha512,
}

impl Algorithm {
    /// Read the algorithm out of a name, with or without a `algo:` prefix.
    pub fn parse(name: &str) -> Option<Algorithm> {
        let cleaned = name
            .trim()
            .trim_start_matches("algo:")
            .trim_start_matches("ALGO:")
            .to_ascii_lowercase();
        match cleaned.as_str() {
            "crc32" | "crc-32" => Some(Algorithm::Crc32),
            "md5" => Some(Algorithm::Md5),
            "sha1" | "sha-1" => Some(Algorithm::Sha1),
            "sha256" | "sha-256" => Some(Algorithm::Sha256),
            "sha512" | "sha-512" => Some(Algorithm::Sha512),
            _ => None,
        }
    }

    /// The name the algorithm is written with in the places a hash is shown.
    pub fn name(self) -> &'static str {
        match self {
            Algorithm::Crc32 => "CRC32",
            Algorithm::Md5 => "MD5",
            Algorithm::Sha1 => "SHA-1",
            Algorithm::Sha256 => "SHA-256",
            Algorithm::Sha512 => "SHA-512",
        }
    }

    /// The length of the hash in bytes.
    pub fn length(self) -> usize {
        match self {
            Algorithm::Crc32 => 4,
            Algorithm::Md5 => 16,
            Algorithm::Sha1 => 20,
            Algorithm::Sha256 => 32,
            Algorithm::Sha512 => 64,
        }
    }
}

/// Read the algorithm out of the length of a hash, when the publisher gave
/// only the hash.
pub fn algorithm_for_length(length: usize) -> Option<Algorithm> {
    match length {
        4 => Some(Algorithm::Crc32),
        16 => Some(Algorithm::Md5),
        20 => Some(Algorithm::Sha1),
        32 => Some(Algorithm::Sha256),
        64 => Some(Algorithm::Sha512),
        _ => None,
    }
}

/// Whether a hash is one this engine can check: hex digits, and as many of
/// them as one of the algorithms has.
pub fn is_hash(text: &str) -> bool {
    let mut digits = 0usize;
    for c in text.chars() {
        if c.is_ascii_hexdigit() {
            digits += 1;
        } else if !c.is_whitespace() && c != ':' && c != '-' {
            return false;
        }
    }
    algorithm_for_length(digits).is_some()
}

/// Why a hash could not be read or used.
#[derive(Debug)]
pub enum Error {
    /// The file could not be read.
    Io(std::io::Error),
    /// The text holds no hash of the length the algorithm has.
    Length {
        /// The number of hex digits the algorithm's hash has.
        expected: usize,
        /// The number of hex digits the text had.
        found: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(error) => write!(f, "the file could not be read: {error}"),
            Error::Length { expected, found } => {
                write!(f, "expected {expected} hex digits, found {found}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(error) => Some(error),
            Error::Length { .. } => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Error {
        Error::Io(error)
    }
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The file's hash is the one that was expected.
    Matches,
    /// The file's hash is another one; both are given, so the reader can see
    /// which is which.
    Differs {
        /// The hash that was expected, as it was written down.
        expected: String,
        /// The hash the file has.
        actual: String,
    },
}

/// The expected hash as it is compared: lower-case hex digits, without the
/// spaces, colons and dashes a publisher's text may carry (`ab:cd:ef…`), and
/// without the name in front of it (`sha256:`) or the file name after it
/// (`…  *file.iso`).
///
/// A hash that is not the length of the algorithm is refused rather than
/// compared: a hash of another algorithm would only ever differ, and saying
/// so is more use than a mismatch.
pub fn normalise(text: &str, algorithm: Algorithm) -> Result<String, Error> {
    let wanted = algorithm.length() * 2;
    let mut longest = 0;
    for run in hex_runs(without_name(text)) {
        if run.len() == wanted {
            return Ok(run);
        }
        longest = longest.max(run.len());
    }
    Err(Error::Length {
        expected: wanted,
        found: longest,
    })
}

/// The hash of a file, as hex.
pub fn hash_file(path: &Path, algorithm: Algorithm) -> Result<String, Error> {
    let mut file = std::fs::File::open(path)?;
    let mut buffer = vec![0u8; 1 << 16];
    macro_rules! read_into {
        ($hasher:expr) => {{
            let mut hasher = $hasher;
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            hasher.finalize()
        }};
    }
    Ok(match algorithm {
        Algorithm::Crc32 => {
            let mut hasher = crc32fast::Hasher::new();
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            hex(&hasher.finalize().to_be_bytes())
        }
        Algorithm::Md5 => hex(&read_into!(md5::Md5::new())),
        Algorithm::Sha1 => hex(&read_into!(sha1::Sha1::new())),
        Algorithm::Sha256 => hex(&read_into!(sha2::Sha256::new())),
        Algorithm::Sha512 => hex(&read_into!(sha2::Sha512::new())),
    })
}

/// Whether the file's hash is the one that was expected.
pub fn verify_file(path: &Path, algorithm: Algorithm, expected: &str) -> Result<Outcome, Error> {
    let expected = normalise(expected, algorithm)?;
    let actual = hash_file(path, algorithm)?;
    Ok(if actual == expected {
        Outcome::Matches
    } else {
        Outcome::Differs { expected, actual }
    })
}

/// The text without the algorithm name in front of it (`sha256:…`,
/// `algo:md5=…`).
fn without_name(text: &str) -> &str {
    for (at, c) in text.char_indices() {
        if c != ':' && c != '=' {
            continue;
        }
        if Algorithm::parse(&text[..at]).is_some() {
            return &text[at + c.len_utf8()..];
        }
    }
    text
}

/// The hex runs of a text: the digits, with the spaces, colons and dashes
/// between them left out. A run stops where the text stops being a hash
/// (the file name after it, the brackets around it).
fn hex_runs(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut runs = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if !chars[at].is_ascii_hexdigit() {
            at += 1;
            continue;
        }
        let mut run = String::new();
        while at < chars.len() {
            let c = chars[at];
            if c.is_ascii_hexdigit() {
                run.push(c.to_ascii_lowercase());
                at += 1;
            } else if (c.is_whitespace() || c == ':' || c == '-')
                && chars
                    .get(at + 1)
                    .is_some_and(|next| next.is_ascii_hexdigit())
            {
                // A separator inside the hash: the digits go on either side.
                at += 1;
            } else {
                break;
            }
        }
        runs.push(run);
    }
    runs
}

/// The digits a hex number is written with.
const DIGITS: &[u8; 16] = b"0123456789abcdef";

/// The bytes as hex, in lower case.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const FOX: &[u8] = b"The quick brown fox jumps over the lazy dog";

    /// A file of the given contents, in the system's temporary directory.
    fn a_file_with(contents: &[u8]) -> PathBuf {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "idm-checksum-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, contents).expect("the file is written");
        path
    }

    #[test]
    fn an_algorithm_is_read_out_of_its_name() {
        assert_eq!(Algorithm::parse("SHA-256"), Some(Algorithm::Sha256));
        assert_eq!(Algorithm::parse("algo:md5"), Some(Algorithm::Md5));
        assert_eq!(Algorithm::parse("sha3"), None);
    }

    #[test]
    fn a_hash_is_told_from_its_length() {
        assert!(is_hash("d41d8cd98f00b204e9800998ecf8427e"));
        assert!(!is_hash("not a hash"));
    }

    #[test]
    fn a_file_hashes_to_what_is_known() {
        let path = a_file_with(FOX);
        let file = &path;
        assert_eq!(
            hash_file(file, Algorithm::Sha256).expect("the file is read"),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
        assert_eq!(
            hash_file(file, Algorithm::Md5).expect("the file is read"),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
        assert_eq!(
            hash_file(file, Algorithm::Sha1).expect("the file is read"),
            "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12"
        );
        assert_eq!(
            hash_file(file, Algorithm::Sha512).expect("the file is read"),
            "07e547d9586f6a73f73fbac0435ed76951218fb7d0c8d788a309d785436bbb642e93a252a954f2\
             3912547d1e8a3b5ed6e1bfd7097821233fa0538f3db854fee6"
        );
        assert_eq!(
            hash_file(file, Algorithm::Crc32).expect("the file is read"),
            "414fa339"
        );
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn a_hash_is_read_out_of_the_text_it_was_copied_from() {
        let hash = "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592";
        assert_eq!(normalise(hash, Algorithm::Sha256).expect("a hash"), hash);
        // The spaces, the colons and the dashes of a copied hash.
        assert_eq!(
            normalise(
                "D7A8 FBB3 07D7 8094 69CA 9ABC B008 2E4F 8D56 51E4 6D3C DB76 2D02 D0BF 37C9 E592",
                Algorithm::Sha256
            )
            .expect("a hash"),
            hash
        );
        assert_eq!(
            normalise(
                "d7a8:fbb3:07d7:8094:69ca:9abc:b008:2e4f:8d56:51e4:6d3c:db76:2d02:d0bf:37c9:e592",
                Algorithm::Sha256
            )
            .expect("a hash"),
            hash
        );
        // The name in front, and the file name after.
        assert_eq!(
            normalise(&format!("sha256:{hash} *fox.txt"), Algorithm::Sha256).expect("a hash"),
            hash
        );
        assert_eq!(
            normalise(&format!("algo:md5:{hash}"), Algorithm::Sha256).expect("a hash"),
            hash
        );
        assert_eq!(
            normalise(&format!("SHA-256 (fox.txt) = {hash}"), Algorithm::Sha256).expect("a hash"),
            hash
        );
    }

    #[test]
    fn a_hash_of_another_length_is_refused() {
        let error = normalise("9e107d9d372bb6826bd81d3542a419d6", Algorithm::Sha256)
            .expect_err("the length is wrong");
        assert!(matches!(
            error,
            Error::Length {
                expected: 64,
                found: 32
            }
        ));
        assert!(normalise("not a hash", Algorithm::Sha256).is_err());
        assert!(normalise("", Algorithm::Md5).is_err());
    }

    #[test]
    fn a_file_is_verified_against_the_hash_it_was_given() {
        let path = a_file_with(FOX);
        let file = &path;
        let outcome = verify_file(
            file,
            Algorithm::Sha256,
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592",
        )
        .expect("the file is checked");
        assert_eq!(outcome, Outcome::Matches);
        let outcome =
            verify_file(file, Algorithm::Sha256, &"0".repeat(64)).expect("the file is checked");
        assert_eq!(
            outcome,
            Outcome::Differs {
                expected: "0".repeat(64),
                actual: "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
                    .to_owned(),
            }
        );
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn a_file_that_is_not_there_is_an_error() {
        let error = hash_file(Path::new("there-is-no-such-file"), Algorithm::Md5)
            .expect_err("the file is not there");
        assert!(matches!(error, Error::Io(_)));
    }
}
