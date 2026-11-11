//! Checking a finished file against the hash its publisher gave.
//!
//! The engine's `checksum` module does the hashing and reads the text a hash
//! is copied as; this is the window's side of it. The algorithm comes from the
//! reader's choice or, when they only pasted the hash, from its length.
//!
//! The file is read in pieces by the engine, so a file of any size is checked
//! without sitting in memory whole.

use std::path::Path;

use idm_core::checksum::{self, Algorithm};
use tauri::State;

use crate::state::AppState;

/// What a check found, written for the dialog.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecksumReport {
    /// The algorithm that was used, named the way the dialog prints it.
    pub algorithm: String,
    /// Whether the file's hash is the one that was expected.
    pub matches: bool,
    /// The expected hash, normalised for comparison.
    pub expected: String,
    /// The hash the file actually has.
    pub actual: String,
}

/// Verify a finished download against the hash the reader pasted.
///
/// `algorithm` is the reader's choice from the list; empty or absent means the
/// algorithm is read from the hash's length, which is how a publisher's page
/// usually gives it.
#[tauri::command]
pub fn verify_download_checksum(
    state: State<'_, AppState>,
    id: u64,
    expected: String,
    algorithm: Option<String>,
) -> Result<ChecksumReport, String> {
    let download = state.get(id).ok_or("unknown download")?;
    let dest = download.info().dest;
    check(Path::new(&dest), &expected, algorithm.as_deref())
}

/// The algorithm a pasted hash is checked with.
///
/// A name the reader chose wins; otherwise the length of the hex digits says
/// which algorithm it is. A text that holds no whole hash is refused with the
/// list of what the dialog accepts, rather than compared against nothing.
fn algorithm_for(expected: &str, named: Option<&str>) -> Result<Algorithm, String> {
    if let Some(name) = named.map(str::trim).filter(|name| !name.is_empty()) {
        if !name.eq_ignore_ascii_case("auto") {
            return Algorithm::parse(name)
                .ok_or_else(|| format!("{name} is not a checksum this build knows"));
        }
    }
    let digits = expected
        .chars()
        .filter(char::is_ascii_hexdigit)
        .count();
    if digits % 2 == 0 {
        if let Some(algorithm) = checksum::algorithm_for_length(digits / 2) {
            return Ok(algorithm);
        }
    }
    Err("Paste a CRC32, MD5, SHA-1, SHA-256 or SHA-512 hash.".to_string())
}

/// Check one file, without a window in the way, so this is what the tests
/// exercise.
fn check(path: &Path, expected: &str, named: Option<&str>) -> Result<ChecksumReport, String> {
    if !path.exists() {
        return Err(format!("{} is not there yet", path.display()));
    }
    let algorithm = algorithm_for(expected, named)?;
    let wanted = checksum::normalise(expected, algorithm).map_err(|error| error.to_string())?;
    let actual = checksum::hash_file(path, algorithm).map_err(|error| error.to_string())?;
    Ok(ChecksumReport {
        algorithm: algorithm.name().to_string(),
        matches: actual == wanted,
        expected: wanted,
        actual,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const FOX: &[u8] = b"The quick brown fox jumps over the lazy dog";
    const FOX_SHA256: &str = "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592";

    /// A file of the given contents, in the system's temporary directory.
    fn a_file_with(contents: &[u8]) -> PathBuf {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "bdm-checksum-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::write(&path, contents).expect("the file is written");
        path
    }

    #[test]
    fn the_algorithm_comes_from_the_length_of_the_hash() {
        assert_eq!(
            algorithm_for(FOX_SHA256, None).expect("a sha-256 hash"),
            Algorithm::Sha256
        );
        // The same hash with the spaces a publisher's page carries.
        assert_eq!(
            algorithm_for(
                "D7A8 FBB3 07D7 8094 69CA 9ABC B008 2E4F 8D56 51E4 6D3C DB76 2D02 D0BF 37C9 E592",
                None
            )
            .expect("a spaced hash"),
            Algorithm::Sha256
        );
        // The other lengths a publisher gives.
        assert_eq!(
            algorithm_for(&"a".repeat(40), None).expect("a sha-1 hash"),
            Algorithm::Sha1
        );
        assert_eq!(
            algorithm_for(&"a".repeat(128), None).expect("a sha-512 hash"),
            Algorithm::Sha512
        );
        assert_eq!(
            algorithm_for(&"a".repeat(32), None).expect("an md5 hash"),
            Algorithm::Md5
        );
        assert_eq!(
            algorithm_for(&"a".repeat(8), None).expect("a crc32 hash"),
            Algorithm::Crc32
        );
    }

    #[test]
    fn a_chosen_algorithm_wins_over_the_length() {
        assert_eq!(
            algorithm_for(FOX_SHA256, Some("SHA-256")).expect("the named algorithm"),
            Algorithm::Sha256
        );
        assert!(algorithm_for(FOX_SHA256, Some("sha3")).is_err());
        // "auto" is what the dialog sends when the reader picked no algorithm.
        assert_eq!(
            algorithm_for(FOX_SHA256, Some("auto")).expect("the length decides"),
            Algorithm::Sha256
        );
    }

    #[test]
    fn a_text_without_a_hash_is_refused() {
        let error = algorithm_for("not a hash", None).expect_err("no hash");
        assert!(error.contains("SHA-256"), "{error}");
        assert!(algorithm_for("", None).is_err());
    }

    #[test]
    fn a_matching_file_is_reported_as_matching() {
        let path = a_file_with(FOX);
        let report = check(&path, FOX_SHA256, None).expect("the file is checked");
        assert!(report.matches);
        assert_eq!(report.algorithm, "SHA-256");
        assert_eq!(report.expected, FOX_SHA256);
        assert_eq!(report.actual, FOX_SHA256);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn another_hash_is_reported_with_both_hashes() {
        let path = a_file_with(FOX);
        let report = check(&path, &"0".repeat(64), None).expect("the file is checked");
        assert!(!report.matches);
        assert_eq!(report.expected, "0".repeat(64));
        assert_eq!(report.actual, FOX_SHA256);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_hash_of_the_wrong_length_for_its_name_is_an_error() {
        let path = a_file_with(FOX);
        let error = check(&path, &"0".repeat(32), Some("SHA-256")).expect_err("the length is wrong");
        assert!(error.contains("64"), "{error}");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_file_that_is_not_there_is_an_error() {
        let error = check(
            Path::new("there-is-no-such-file.bin"),
            FOX_SHA256,
            None,
        )
        .expect_err("the file is not there");
        assert!(error.contains("not there"), "{error}");
    }
}
