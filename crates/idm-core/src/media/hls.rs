//! HLS (`m3u8`) parsing.
//!
//! A master playlist names variants (one bitrate and resolution each) and the
//! audio renditions those variants share. A media playlist names the segments
//! of one variant, with an optional init segment and optional byte ranges.
//!
//! Only what a download needs is read: `BANDWIDTH`, `AVERAGE-BANDWIDTH`,
//! `RESOLUTION`, `CODECS`, `FRAME-RATE` and `AUDIO` on a variant; `#EXTINF`,
//! `#EXT-X-MAP`, `#EXT-X-BYTERANGE`, `#EXT-X-MEDIA` and `#EXT-X-ENDLIST` on a
//! playlist. Anything encrypted is refused with [`MediaError::Protected`],
//! never decrypted: the markers themselves live in [`super::drm`], and the
//! sentence names the key method or key format that was found. A playlist
//! without `#EXT-X-ENDLIST` is refused as live, which is
//! its own sentence: a live stream is not encryption.
//!
//! Byte ranges are read the way the format defines them: an omitted offset
//! continues the previous sub-range of the *same* resource and is refused when
//! there is none, an init segment's range needs its offset, and every range is
//! checked for overflow. A playlist that changes its init segment partway
//! through, restarts its timeline (a discontinuity in a fragmented stream), or
//! skips a segment it marked as a gap is refused rather than joined into a
//! file whose timeline claims bytes it does not have.

use reqwest::Url;

use super::subtitles::{self, SubtitleRendition};
use super::{drm, MediaError, MAX_DURATION, MAX_SEGMENTS};

/// The most variants a master may name. A master with more is not a playlist a
/// person is going to choose from.
const MAX_VARIANTS: usize = 64;

/// One variant of a master playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    /// The id the panel shows and the download asks for back.
    pub id: String,
    /// The absolute address of the variant's media playlist.
    pub uri: String,
    pub bandwidth: Option<u64>,
    pub average_bandwidth: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub codecs: Option<String>,
    pub frame_rate: Option<f64>,
    /// The `GROUP-ID` of the audio rendition this variant points at.
    pub audio_group: Option<String>,
    /// `HDR` appears in the codec string or in a `VIDEO-RANGE` attribute.
    pub hdr: bool,
}

/// One audio rendition of a master playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioRendition {
    pub id: String,
    pub uri: String,
    pub group: String,
    pub name: String,
    pub language: Option<String>,
    pub default: bool,
    pub channels: Option<u32>,
    pub codecs: Option<String>,
}

/// A master playlist.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Master {
    pub variants: Vec<Variant>,
    pub audio: Vec<AudioRendition>,
    /// The caption renditions the master offered, each on its own row.
    pub subtitles: Vec<SubtitleRendition>,
}

impl Master {
    pub fn variant(&self, id: &str) -> Option<&Variant> {
        self.variants.iter().find(|variant| variant.id == id)
    }

    pub fn audio_rendition(&self, id: &str) -> Option<&AudioRendition> {
        self.audio.iter().find(|audio| audio.id == id)
    }

    /// The audio renditions of one group, in the order the manifest named them.
    pub fn audio_group(&self, group: &str) -> Vec<&AudioRendition> {
        self.audio
            .iter()
            .filter(|audio| audio.group == group)
            .collect()
    }
}

/// One segment of a media playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct HlsSegment {
    /// The absolute address, with the byte range when the manifest named one.
    pub url: String,
    /// Inclusive byte range inside `url`.
    pub range: Option<(u64, u64)>,
    pub duration: f64,
}

/// A media playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPlaylist {
    pub segments: Vec<HlsSegment>,
    pub init: Option<HlsSegment>,
    pub duration: f64,
    pub ended: bool,
    pub target_duration: Option<f64>,
    pub version: Option<u32>,
    /// How often the playlist restarts its timeline. A playlist of plain
    /// MPEG-TS segments is joined byte for byte, where each segment carries
    /// its own timestamps, so a discontinuity is only a note. A fragmented
    /// playlist is refused before it gets this far.
    pub discontinuities: usize,
}

/// Read a playlist of either kind.
///
/// A master becomes one [`super::Manifest`] row per variant, paired with its
/// audio group, and one row per caption rendition. A media playlist becomes a
/// single muxed row; the app fetches it again as a media playlist when the
/// user chooses it.
pub fn parse(text: &str, base_url: &str) -> Result<super::Manifest, MediaError> {
    Ok(parse_with_subtitles(text, base_url)?.manifest)
}

/// Read a playlist of either kind, with the caption renditions it offered.
///
/// The renditions are the detail behind the caption rows: the language, the
/// name and whether the manifest marked one as the default.
pub fn parse_with_subtitles(
    text: &str,
    base_url: &str,
) -> Result<subtitles::SubtitleManifest, MediaError> {
    if looks_like_master(text) {
        let master = parse_master(text, base_url)?;
        let mut manifest = super::quality::manifest_from_hls(&master, None);
        subtitles::add_formats(&mut manifest, &master.subtitles);
        return Ok(subtitles::SubtitleManifest {
            manifest,
            renditions: master.subtitles,
        });
    }
    let media = parse_media(text, base_url)?;
    let format = super::quality::hls_media_format(&media);
    Ok(subtitles::SubtitleManifest {
        manifest: super::Manifest {
            title: String::new(),
            duration: Some(media.duration),
            formats: vec![format],
            pairs: Vec::new(),
            container_hint: Some("hls".to_string()),
            subtitles: Vec::new(),
        },
        renditions: Vec::new(),
    })
}

/// Whether a playlist is a master rather than a media playlist.
pub fn looks_like_master(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        line.starts_with("#EXT-X-STREAM-INF:")
    })
}

/// The first line has to be the HLS header; anything else is not a playlist.
fn require_header(text: &str) -> Result<(), MediaError> {
    let first = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if first != "#EXTM3U" {
        return Err(MediaError::Malformed(
            "the playlist does not start with #EXTM3U".to_string(),
        ));
    }
    Ok(())
}

/// The `key=value` pairs of an HLS tag, with quoted commas respected.
pub(crate) struct Attributes(Vec<(String, String)>);

impl Attributes {
    pub(crate) fn parse(text: &str) -> Self {
        let mut out = Vec::new();
        let mut current = String::new();
        let mut quoted = false;
        let mut parts = Vec::new();
        for character in text.chars() {
            match character {
                '"' => {
                    quoted = !quoted;
                    current.push(character);
                }
                ',' if !quoted => {
                    parts.push(std::mem::take(&mut current));
                }
                other => current.push(other),
            }
        }
        parts.push(current);

        for part in parts {
            let Some((name, value)) = part.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let value = value.trim().trim_matches('"');
            out.push((name.to_ascii_uppercase(), value.to_string()));
        }
        Self(out)
    }

    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn flag(&self, name: &str) -> bool {
        self.get(name)
            .map(|value| value.eq_ignore_ascii_case("YES"))
            .unwrap_or(false)
    }
}

fn parse_u64(value: &str) -> Option<u64> {
    value.trim().parse().ok()
}

fn parse_f64(value: &str) -> Option<f64> {
    let value: f64 = value.trim().parse().ok()?;
    value.is_finite().then_some(value)
}

/// `length` or `length@offset`, relative to the resource.
fn parse_byte_range(value: &str) -> Option<(u64, Option<u64>)> {
    let value = value.trim();
    let (length, offset) = match value.split_once('@') {
        Some((length, offset)) => (
            length.trim().parse().ok()?,
            Some(offset.trim().parse().ok()?),
        ),
        None => (value.parse().ok()?, None),
    };
    if length == 0 {
        return None;
    }
    Some((length, offset))
}

/// The inclusive last byte of `length` bytes starting at `offset`.
fn span(offset: u64, length: u64) -> Result<u64, MediaError> {
    offset
        .checked_add(length)
        .and_then(|end| end.checked_sub(1))
        .ok_or_else(|| {
            MediaError::Malformed("a byte range reaches past the end of the address".to_string())
        })
}

/// Resolve a playlist address against the address the playlist came from.
pub fn resolve(base_url: &str, uri: &str) -> Result<String, MediaError> {
    let base = Url::parse(base_url)
        .map_err(|_| MediaError::Malformed("the playlist address is not usable".to_string()))?;
    let joined = base
        .join(uri.trim())
        .map_err(|_| MediaError::Malformed(format!("the address {uri} is not usable")))?;
    if joined.scheme() != "http" && joined.scheme() != "https" {
        return Err(MediaError::Unsupported(
            "only http and https addresses are followed".to_string(),
        ));
    }
    Ok(joined.to_string())
}

mod master;
mod media;

pub use master::parse_master;
pub use media::parse_media;

#[cfg(test)]
mod tests;
