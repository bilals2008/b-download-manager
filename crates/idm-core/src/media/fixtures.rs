//! The test fixtures, loaded into the tests as text.
//!
//! They are playlists and manifests, not media: a few hundred bytes each, so a
//! test can read them without a fixture folder on disk and without a network.

pub const HLS_MASTER: &str = include_str!("fixtures/hls/master.m3u8");
pub const HLS_MEDIA: &str = include_str!("fixtures/hls/media.m3u8");
pub const HLS_BAD: &str = include_str!("fixtures/hls/bad.m3u8");
pub const HLS_ENCRYPTED: &str = include_str!("fixtures/hls/encrypted.m3u8");
pub const DASH_MANIFEST: &str = include_str!("fixtures/dash/manifest.mpd");
pub const DASH_BAD: &str = include_str!("fixtures/dash/bad.mpd");

/// The bytes of a segment, small enough to compare in a test.
pub fn segment(tag: &str) -> Vec<u8> {
    format!("{tag}:0123456789abcdef").into_bytes()
}

/// The two segments of the fixture media playlist, in order.
pub fn segments() -> Vec<Vec<u8>> {
    vec![segment("SEG0"), segment("SEG1")]
}

/// The init segment of the fixture media playlist.
pub fn init() -> Vec<u8> {
    segment("INIT")
}
