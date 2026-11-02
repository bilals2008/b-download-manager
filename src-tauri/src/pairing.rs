//! The pairing record of the browser extension, stored in one small file.
//!
//! The loopback bridge is only useful if it can tell the extension from every
//! other program on this machine, and headers cannot do that: any local process
//! can send any header it likes. The application therefore makes up a token,
//! the reader copies it into the extension once, and every request after that
//! carries it as a bearer token. The record also keeps the extension origin the
//! token first arrived from, so a second extension that somehow learns the token
//! does not get the same rights.
//!
//! The token is a secret of this machine, not a password to anything remote. It
//! lives in its own file, never in `settings.json`, so a support file or a
//! settings export does not carry it, and deleting the file is the same as
//! un-pairing the browser. A process running as the same user on this machine
//! can still read it; the token stops the far more likely cases — a web page,
//! another browser's extension, or any local program that does not know it.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::bridge::auth;

/// The file name under the application's data directory.
pub const FILE_NAME: &str = "browser-pairing.json";

/// The version of the file this build reads.
pub const VERSION: u32 = 1;

/// The two values the bridge needs from the pairing: the secret and the origin
/// that first proved it knew the secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Pairing {
    pub version: u32,
    /// The token the reader copies into the extension. Empty means none, which
    /// makes every authenticated request a refusal rather than an open door.
    pub token: String,
    /// The extension origin that first carried this token, `""` until one does.
    pub origin: String,
}

impl Default for Pairing {
    fn default() -> Self {
        Self {
            version: VERSION,
            token: String::new(),
            origin: String::new(),
        }
    }
}

impl Pairing {
    /// A record with a fresh token and no origin pinned yet.
    pub fn fresh() -> Self {
        Self {
            version: VERSION,
            token: auth::new_token(),
            origin: String::new(),
        }
    }
}

/// Whether a stored token is one this build would accept. A file edited by
/// hand, or written by a different build with a shorter secret, is not.
fn token_is_usable(token: &str) -> bool {
    !token.is_empty()
        && token.len() >= 32
        && token.len() <= 128
        && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Read one field of a damaged file without keeping the junk around.
fn sanitize(mut pairing: Pairing) -> Pairing {
    pairing.version = VERSION;
    if !token_is_usable(&pairing.token) {
        pairing.token = String::new();
        pairing.origin = String::new();
    }
    if !auth::is_extension_origin(&pairing.origin) {
        pairing.origin = String::new();
    }
    pairing
}

/// Read the record, or the defaults when there is no usable file.
pub fn load(path: &Path) -> Pairing {
    let read = || -> Result<Pairing, String> {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        serde_json::from_slice::<Pairing>(&bytes).map_err(|error| error.to_string())
    };
    match read() {
        Ok(pairing) => sanitize(pairing),
        Err(error) => {
            idm_core::log::write(
                "warn",
                &format!("the browser pairing could not be read: {error}"),
            );
            Pairing::default()
        }
    }
}

/// Write the record whole, the way the other small files are written: a
/// temporary file beside the real one is renamed over it, so a crash leaves
/// either the old pairing or the new one, never half of either.
pub fn save(path: &Path, pairing: &Pairing) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("the pairing folder could not be made: {error}"))?;
    }
    let json = serde_json::to_vec_pretty(pairing)
        .map_err(|error| format!("the pairing could not be written: {error}"))?;
    let temporary = path.with_extension("json.new");
    std::fs::write(&temporary, &json)
        .map_err(|error| format!("the pairing could not be written: {error}"))?;
    if std::fs::rename(&temporary, path).is_ok() {
        return Ok(());
    }
    let result = std::fs::write(path, &json)
        .map_err(|error| format!("the pairing could not be written: {error}"));
    let _ = std::fs::remove_file(&temporary);
    result
}

/// The record of this machine, made on the spot if the file is not there yet.
///
/// A first run, or a file whose token was junk, gets a fresh token written to
/// disk before anything is answered: the bridge has to have a secret to compare
/// against from its very first request, and the settings page has to have
/// something real to show.
pub fn open(path: &Path) -> Pairing {
    let stored = load(path);
    if !stored.token.is_empty() {
        return stored;
    }
    let pairing = Pairing::fresh();
    if pairing.token.is_empty() {
        // No random source: the bridge refuses everything it should protect.
        idm_core::log::write("warn", "the browser bridge has no pairing token");
        return Pairing::default();
    }
    if let Err(error) = save(path, &pairing) {
        idm_core::log::write("warn", &error);
    }
    pairing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "b-download-manager-pairing-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join(FILE_NAME)
    }

    fn clean(path: &Path) {
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_record_survives_a_save_and_a_load() {
        let path = temp_file("round-trip");
        let pairing = Pairing {
            version: VERSION,
            token: "a".repeat(64),
            origin: "moz-extension://3c1e8f2a".to_string(),
        };
        save(&path, &pairing).expect("a saved file");
        assert_eq!(load(&path), pairing);
        clean(&path);
    }

    #[test]
    fn a_missing_file_pairs_nothing() {
        let path = temp_file("missing");
        assert_eq!(load(&path), Pairing::default());
        clean(&path);
    }

    #[test]
    fn a_damaged_file_pairs_nothing_and_the_next_save_works() {
        let path = temp_file("damaged");
        std::fs::write(&path, b"{ not json").expect("a damaged file");
        assert_eq!(load(&path), Pairing::default());
        save(&path, &Pairing::default()).expect("the save recovers");
        assert_eq!(load(&path), Pairing::default());
        clean(&path);
    }

    #[test]
    fn a_short_or_padded_token_is_not_a_token() {
        let path = temp_file("token");
        for junk in ["", "abc123", &"z".repeat(64), &"a".repeat(31)] {
            std::fs::write(
                &path,
                format!(r#"{{"version":1,"token":"{junk}","origin":"moz-extension://x1"}}"#)
                    .as_bytes(),
            )
            .expect("a written file");
            let pairing = load(&path);
            assert_eq!(pairing.token, "", "{junk:?} is not an accepted token");
            assert_eq!(
                pairing.origin, "",
                "an origin is only kept beside a real token"
            );
        }
        clean(&path);
    }

    #[test]
    fn an_origin_that_is_not_an_extension_is_dropped() {
        let path = temp_file("origin");
        std::fs::write(
            &path,
            br#"{"version":1,"token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","origin":"https://example.org"}"#,
        )
        .expect("a written file");
        assert_eq!(load(&path).origin, "");
        clean(&path);
    }

    #[test]
    fn opening_a_first_run_writes_a_real_token() {
        let path = temp_file("first-run");
        let pairing = open(&path);
        assert!(token_is_usable(&pairing.token));
        assert_eq!(pairing.origin, "");
        // The file is there, so a restart keeps the same pairing.
        assert_eq!(load(&path), pairing);
        assert_eq!(open(&path), pairing, "the second open changes nothing");
        clean(&path);
    }

    #[test]
    fn a_fresh_record_has_a_different_token_every_time() {
        assert_ne!(Pairing::fresh().token, Pairing::fresh().token);
    }
}
