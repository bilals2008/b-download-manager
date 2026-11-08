//! Media manifests: HLS, MPEG-DASH and plain progressive files.
//!
//! Everything that reads a playlist lives here, in the engine crate, with no
//! window toolkit beside it: [`hls`] and [`dash`] turn a manifest into a list of
//! formats and the byte ranges behind them, and [`quality`] turns those
//! formats into the rows a person sees. The engine itself never learns what a
//! video is: a track is an ordinary download of one address after another.
//!
//! DRM is recognised only to be refused. Nothing here decrypts anything, and
//! nothing here solves a signature: a protected stream produces
//! [`MediaError::Protected`] with one sentence the window can show.

pub mod dash;
pub mod download;
pub mod hls;
pub mod quality;
mod xml;
pub mod youtube;

#[cfg(test)]
pub mod fixtures;
#[cfg(test)]
pub mod testserver;

use serde::{Deserialize, Serialize};

/// What one stream of a manifest carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Video,
    Audio,
    /// Picture and sound in the same stream, as an old HLS variant or a plain
    /// progressive file.
    Muxed,
}

/// One row of the quality list.
///
/// `id` is the manifest's own name for the stream, kept as text because HLS
/// variants have no number and DASH representation ids are often numbers
/// (`137`, `251`) that must survive back to the manifest unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Format {
    pub id: String,
    pub kind: MediaKind,
    /// `mp4`, `webm`, `ts`, `m4s`, `m4a`, `mkv` ...
    pub container: String,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub fps: Option<f64>,
    /// Bits per second, as the manifest states it.
    #[serde(default)]
    pub bandwidth: Option<u64>,
    /// The size in bytes when the manifest (or a measurement) gives one.
    #[serde(default)]
    pub size: Option<u64>,
    /// The row text, built by [`quality`]. Never a number the manifest did not
    /// state.
    pub label: String,
}

impl Format {
    /// A format whose label has not been decided yet.
    pub fn new(id: impl Into<String>, kind: MediaKind, container: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            container: container.into(),
            codec: None,
            width: None,
            height: None,
            fps: None,
            bandwidth: None,
            size: None,
            label: String::new(),
        }
    }

    /// The estimate of the bytes this stream takes, from the bitrate the
    /// manifest stated and the duration it stated. `None` when either is
    /// missing: a guess is worse than an empty column.
    pub fn estimate_size(&self, duration_seconds: Option<f64>) -> Option<u64> {
        let bandwidth = self.bandwidth?;
        let duration = duration_seconds?;
        if bandwidth == 0 || duration <= 0.0 {
            return None;
        }
        // Bandwidth is bits per second; the file is bytes.
        Some((bandwidth as f64 / 8.0 * duration).round() as u64)
    }
}

/// One video row and the audio that goes with it.
///
/// `audio` is `None` for a video that carries its own sound (a muxed stream),
/// or for a video whose audio track the page never handed over. The window
/// says which of the two it is from the selected format's kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pair {
    pub video: String,
    #[serde(default)]
    pub audio: Option<String>,
    pub label: String,
}

/// What a manifest offered, before a track is chosen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub title: String,
    #[serde(default)]
    pub duration: Option<f64>,
    pub formats: Vec<Format>,
    #[serde(default)]
    pub pairs: Vec<Pair>,
    /// `hls`, `dash`, `file` — which parser produced this. The download path
    /// needs it to fetch the right kind of segment list later.
    #[serde(default)]
    pub container_hint: Option<String>,
}

impl Manifest {
    pub fn is_empty(&self) -> bool {
        self.formats.is_empty()
    }
}

/// Why a manifest could not be used, in one sentence a person can act on.
///
/// Each refusal is its own variant so the panel can say *which* of the
/// reasons applies: encrypted media, a live stream, or a stream that only
/// exists inside the player's own transport. Labeling all three as "DRM"
/// would be a lie, and a generic HTTP failure would say nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaError {
    /// Encrypted media. The name is the protection scheme that was found
    /// (Widevine, PlayReady, FairPlay, AES-128, ...), or empty when the
    /// manifest did not say which one. Never downloaded, never decrypted.
    Protected(String),
    /// A live stream. Recording live is not in this feature.
    Live,
    /// The stream is only offered through the player's own protocol
    /// (YouTube's SABR or UMP), which has no plain address to fetch.
    PlayerTransport(String),
    /// A cap was reached: manifest size, segment count, duration.
    TooLarge(String),
    /// The manifest is not the format it claims to be.
    Malformed(String),
    /// A feature of the format this build does not download.
    Unsupported(String),
}

impl MediaError {
    /// The refusal of an encrypted stream, naming the scheme when one was
    /// recognised. An empty name keeps the plain "protected" sentence.
    pub fn protected(scheme: impl Into<String>) -> Self {
        MediaError::Protected(scheme.into().trim().to_string())
    }
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaError::Protected(scheme) if scheme.is_empty() => write!(
                f,
                "this stream is protected (DRM), so it cannot be downloaded"
            ),
            MediaError::Protected(scheme) => write!(
                f,
                "this stream is protected by {scheme}, so it cannot be downloaded"
            ),
            MediaError::Live => write!(
                f,
                "this is a live stream; recording live streams is not supported"
            ),
            MediaError::PlayerTransport(what) => write!(
                f,
                "this stream is only offered through {what}, which has no downloadable address"
            ),
            MediaError::TooLarge(what) => write!(f, "the stream is too large: {what}"),
            MediaError::Malformed(what) => write!(f, "the manifest could not be read: {what}"),
            MediaError::Unsupported(what) => write!(f, "this stream cannot be downloaded: {what}"),
        }
    }
}

impl std::error::Error for MediaError {}

/// The most segments one track may hold. The engine opens a download per
/// segment; beyond this the plan is refused rather than turned into hours of
/// requests nobody asked for.
pub const MAX_SEGMENTS: usize = 5_000;

/// The longest stream this feature accepts, in seconds (12 hours).
pub const MAX_DURATION: f64 = 12.0 * 60.0 * 60.0;

/// The most bytes a manifest may be (2 MiB).
pub const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;

/// The protection marker an init segment carries, if any.
///
/// `pssh` is the common-encryption header, and `encv`/`enca` are protected
/// sample entries. Their presence means the bytes are encrypted, so the media
/// path refuses the track with the same class of sentence as every other DRM
/// case — but naming what was found. Nothing here decrypts or inspects a
/// licence.
pub fn protection_in(bytes: &[u8]) -> Option<&'static str> {
    let mut sample_entry = false;
    for window in bytes.windows(4) {
        match window {
            b"pssh" => return Some("MPEG Common Encryption"),
            b"encv" | b"enca" => sample_entry = true,
            _ => {}
        }
    }
    sample_entry.then_some("an encrypted sample entry")
}

/// Whether an init segment carries a protection box.
pub fn looks_protected(bytes: &[u8]) -> bool {
    protection_in(bytes).is_some()
}

/// One address that has to be downloaded for a track, with the byte range the
/// manifest named when it named one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSegment {
    pub url: String,
    /// Inclusive byte range inside `url`, for an HLS `#EXT-X-BYTERANGE` or a
    /// DASH media range.
    pub range: Option<(u64, u64)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixtures_load() {
        assert!(fixtures::HLS_MASTER.contains("#EXTM3U"));
        assert!(fixtures::HLS_MEDIA.contains("#EXT-X-MAP"));
        assert!(fixtures::DASH_MANIFEST.contains("<MPD"));
        assert!(fixtures::DASH_BAD.contains("<MPD") || fixtures::DASH_BAD.trim().is_empty());
        assert!(fixtures::HLS_BAD.contains("#EXTM3U"));
    }

    #[test]
    fn a_size_estimate_needs_both_numbers() {
        let mut format = Format::new("137", MediaKind::Video, "mp4");
        format.bandwidth = Some(4_000_000);
        assert_eq!(format.estimate_size(None), None);
        assert_eq!(format.estimate_size(Some(0.0)), None);
        assert_eq!(format.estimate_size(Some(8.0)), Some(4_000_000));
    }

    #[test]
    fn every_refusal_reads_as_one_sentence() {
        for error in [
            MediaError::protected(""),
            MediaError::protected("Widevine"),
            MediaError::Live,
            MediaError::PlayerTransport("YouTube's UMP protocol".to_string()),
            MediaError::TooLarge("too many segments".to_string()),
            MediaError::Malformed("no header".to_string()),
            MediaError::Unsupported("trickmode".to_string()),
        ] {
            let text = error.to_string();
            assert!(!text.is_empty());
            assert!(!text.contains('\n'));
            assert!(
                text.chars().filter(|character| *character == '.').count() <= 1,
                "one sentence: {text}"
            );
        }
        assert!(MediaError::protected("").to_string().contains("protected"));
    }

    #[test]
    fn each_refusal_says_which_reason_it_is() {
        let encrypted = MediaError::protected("Widevine").to_string();
        let live = MediaError::Live.to_string();
        let player = MediaError::PlayerTransport("YouTube's UMP protocol".to_string()).to_string();

        assert!(encrypted.contains("Widevine"), "{encrypted}");
        assert!(!encrypted.contains("live"), "{encrypted}");
        assert!(live.contains("live"), "{live}");
        assert!(!live.contains("protected"), "{live}");
        assert!(player.contains("UMP"), "{player}");
        assert!(
            !player.contains("protected") && !player.contains("DRM"),
            "a transport with no plain address is not encryption: {player}"
        );
        assert_ne!(encrypted, live);
        assert_ne!(live, player);
    }

    #[test]
    fn a_protection_box_is_recognised_in_an_init_segment() {
        assert!(looks_protected(b"\0\0\0\x20pssh\0\0\0\0"));
        assert!(looks_protected(b"....encv...."));
        assert!(looks_protected(b"..enca.."));
        assert!(!looks_protected(b"\0\0\0\x20ftypisom"));
        assert!(!looks_protected(b""));
        assert_eq!(
            protection_in(b"\0\0\0\x20pssh\0\0\0\0"),
            Some("MPEG Common Encryption")
        );
        assert_eq!(
            protection_in(b"....encv...."),
            Some("an encrypted sample entry")
        );
        assert_eq!(protection_in(b"ftypisom"), None);
    }
}
