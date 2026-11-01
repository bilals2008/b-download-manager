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
//! never decrypted, and a playlist without `#EXT-X-ENDLIST` is refused as live.

use reqwest::Url;

use super::{MediaError, MAX_DURATION, MAX_SEGMENTS};

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
}

/// Read a playlist of either kind.
///
/// A master becomes one [`super::Manifest`] row per variant, paired with its
/// audio group. A media playlist becomes a single muxed row; the app fetches
/// it again as a media playlist when the user chooses it.
pub fn parse(text: &str, base_url: &str) -> Result<super::Manifest, MediaError> {
    if looks_like_master(text) {
        let master = parse_master(text, base_url)?;
        return Ok(super::quality::manifest_from_hls(&master, None));
    }
    let media = parse_media(text, base_url)?;
    let format = super::quality::hls_media_format(&media);
    Ok(super::Manifest {
        title: String::new(),
        duration: Some(media.duration),
        formats: vec![format],
        pairs: Vec::new(),
        container_hint: Some("hls".to_string()),
    })
}

/// Read a master playlist.
pub fn parse_master(text: &str, base_url: &str) -> Result<Master, MediaError> {
    require_header(text)?;

    let mut master = Master::default();
    let mut pending: Option<PendingVariant> = None;
    let mut audio: Vec<AudioRendition> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            refuse_key(rest)?;
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-SESSION-KEY:") {
            refuse_key(rest)?;
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attrs = Attributes::parse(rest);
            let kind = attrs.get("TYPE").unwrap_or("");
            if !kind.eq_ignore_ascii_case("AUDIO") {
                continue;
            }
            // A rendition without a URI carries its sound inside the variant.
            let Some(uri) = attrs.get("URI").filter(|uri| !uri.is_empty()) else {
                continue;
            };
            let Some(group) = attrs.get("GROUP-ID").filter(|group| !group.is_empty()) else {
                continue;
            };
            audio.push(AudioRendition {
                id: format!("a{}", audio.len()),
                uri: resolve(base_url, uri)?,
                group: group.to_string(),
                name: attrs.get("NAME").unwrap_or("Audio").to_string(),
                language: attrs.get("LANGUAGE").map(str::to_string),
                default: attrs.flag("DEFAULT"),
                channels: attrs.get("CHANNELS").and_then(|value| {
                    value
                        .split('/')
                        .next()
                        .and_then(|value| value.trim().parse().ok())
                }),
                codecs: attrs.get("CODECS").map(str::to_string),
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            if pending.is_some() {
                return Err(MediaError::Malformed(
                    "a variant is missing its playlist address".to_string(),
                ));
            }
            pending = Some(PendingVariant::parse(rest));
            continue;
        }
        if line.starts_with('#') {
            // `#EXT-X-I-FRAME-STREAM-INF` and the rest of the tags do not
            // describe a playable variant.
            continue;
        }

        if let Some(variant) = pending.take() {
            if master.variants.len() >= MAX_VARIANTS {
                return Err(MediaError::TooLarge(format!(
                    "more than {MAX_VARIANTS} variants"
                )));
            }
            let id = format!("v{}", master.variants.len());
            master.variants.push(variant.finish(id, base_url, line)?);
        }
    }

    if pending.is_some() {
        return Err(MediaError::Malformed(
            "the playlist ends before a variant's address".to_string(),
        ));
    }
    // A rendition that no variant points at is still audio the page offered;
    // keep it, so the panel can show it on its own.
    master.audio = audio;

    if master.variants.is_empty() && master.audio.is_empty() {
        return Err(MediaError::Malformed(
            "the playlist names no stream".to_string(),
        ));
    }
    Ok(master)
}

/// Read a media playlist.
pub fn parse_media(text: &str, base_url: &str) -> Result<MediaPlaylist, MediaError> {
    require_header(text)?;

    let mut segments: Vec<HlsSegment> = Vec::new();
    let mut init = None;
    let mut duration_total = 0.0f64;
    let mut ended = false;
    let mut target_duration = None;
    let mut version = None;
    let mut pending_duration: Option<f64> = None;
    let mut pending_range: Option<(u64, Option<u64>)> = None;
    // The end of the previous sub-range of the same resource, for a
    // `#EXT-X-BYTERANGE` that names only a length.
    let mut last_range_end: Option<(String, u64)> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            refuse_key(rest)?;
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-SESSION-KEY:") {
            refuse_key(rest)?;
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            let attrs = Attributes::parse(rest);
            let Some(uri) = attrs.get("URI").filter(|uri| !uri.is_empty()) else {
                return Err(MediaError::Malformed(
                    "an init segment has no address".to_string(),
                ));
            };
            let range =
                attrs
                    .get("BYTERANGE")
                    .and_then(parse_byte_range)
                    .map(|(length, offset)| {
                        let offset = offset.unwrap_or(0);
                        (offset, offset + length.saturating_sub(1))
                    });
            let url = resolve(base_url, uri)?;
            last_range_end = range.map(|(_, end)| (url.clone(), end + 1));
            init = Some(HlsSegment {
                url,
                range,
                duration: 0.0,
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            let (length, offset) = parse_byte_range(rest).ok_or_else(|| {
                MediaError::Malformed("a byte range could not be read".to_string())
            })?;
            pending_range = Some((length, offset));
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let value = rest.split(',').next().unwrap_or("").trim();
            let seconds: f64 = value.parse().map_err(|_| {
                MediaError::Malformed(format!("a segment duration is not a number: {value}"))
            })?;
            if !seconds.is_finite() || seconds < 0.0 {
                return Err(MediaError::Malformed(
                    "a segment duration is not a positive number".to_string(),
                ));
            }
            pending_duration = Some(seconds);
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            target_duration = rest.trim().parse().ok();
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-VERSION:") {
            version = rest.trim().parse().ok();
            continue;
        }
        if line == "#EXT-X-ENDLIST" {
            ended = true;
            continue;
        }
        if line.starts_with('#') {
            // `#EXT-X-DISCONTINUITY`, `#EXT-X-PROGRAM-DATE-TIME`, `#EXT-X-GAP`
            // and the rest do not change which bytes are downloaded.
            continue;
        }

        // A URI line belongs to the `#EXTINF` before it.
        let duration = pending_duration.take().unwrap_or(0.0);
        let url = resolve(base_url, line)?;
        let range = pending_range.take().map(|(length, offset)| {
            let offset = offset.unwrap_or_else(|| {
                // An omitted offset continues after the previous sub-range of
                // the same resource. An explicit `@0` is the first byte.
                last_range_end
                    .as_ref()
                    .filter(|(last_url, _)| *last_url == url)
                    .map(|(_, end)| *end)
                    .unwrap_or(0)
            });
            (offset, offset + length.saturating_sub(1))
        });
        if let Some((_, end)) = range {
            last_range_end = Some((url.clone(), end + 1));
        } else {
            last_range_end = None;
        }

        segments.push(HlsSegment {
            url,
            range,
            duration,
        });
        duration_total += duration;

        if segments.len() > MAX_SEGMENTS {
            return Err(MediaError::TooLarge(format!(
                "more than {MAX_SEGMENTS} segments"
            )));
        }
        if duration_total > MAX_DURATION {
            return Err(MediaError::TooLarge("longer than 12 hours".to_string()));
        }
    }

    if segments.is_empty() {
        return Err(MediaError::Malformed(
            "the playlist names no segments".to_string(),
        ));
    }
    if !ended {
        return Err(MediaError::Live);
    }

    Ok(MediaPlaylist {
        segments,
        init,
        duration: duration_total,
        ended,
        target_duration,
        version,
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

/// `#EXT-X-KEY:METHOD=NONE` is the absence of encryption; everything else is a
/// protected stream. The key is never fetched and the bytes are never touched.
fn refuse_key(rest: &str) -> Result<(), MediaError> {
    let attrs = Attributes::parse(rest);
    let method = attrs.get("METHOD").unwrap_or("").trim();
    if method.eq_ignore_ascii_case("NONE") || method.is_empty() {
        return Ok(());
    }
    Err(MediaError::Protected)
}

struct PendingVariant {
    bandwidth: Option<u64>,
    average_bandwidth: Option<u64>,
    width: Option<u32>,
    height: Option<u32>,
    codecs: Option<String>,
    frame_rate: Option<f64>,
    audio_group: Option<String>,
    hdr: bool,
}

impl PendingVariant {
    fn parse(rest: &str) -> Self {
        let attrs = Attributes::parse(rest);
        let (width, height) = attrs
            .get("RESOLUTION")
            .and_then(|value| value.split_once(['x', 'X']))
            .map(|(width, height)| (width.trim().parse().ok(), height.trim().parse().ok()))
            .unwrap_or((None, None));
        let video_range = attrs.get("VIDEO-RANGE").unwrap_or("");
        let codecs = attrs.get("CODECS").map(str::to_string);
        let hdr = video_range.eq_ignore_ascii_case("PQ")
            || video_range.eq_ignore_ascii_case("HLG")
            || codecs
                .as_deref()
                .map(|codecs| {
                    let codecs = codecs.to_ascii_lowercase();
                    codecs.contains("hdr") || codecs.contains("dvh") || codecs.contains("hvc1.2")
                })
                .unwrap_or(false);
        Self {
            bandwidth: attrs.get("BANDWIDTH").and_then(parse_u64),
            average_bandwidth: attrs.get("AVERAGE-BANDWIDTH").and_then(parse_u64),
            width,
            height,
            codecs,
            frame_rate: attrs.get("FRAME-RATE").and_then(parse_f64),
            audio_group: attrs
                .get("AUDIO")
                .filter(|group| !group.is_empty())
                .map(str::to_string),
            hdr,
        }
    }

    fn finish(self, id: String, base_url: &str, uri: &str) -> Result<Variant, MediaError> {
        Ok(Variant {
            id,
            uri: resolve(base_url, uri)?,
            bandwidth: self.bandwidth,
            average_bandwidth: self.average_bandwidth,
            width: self.width,
            height: self.height,
            codecs: self.codecs,
            frame_rate: self.frame_rate,
            audio_group: self.audio_group,
            hdr: self.hdr,
        })
    }
}

/// The `key=value` pairs of an HLS tag, with quoted commas respected.
struct Attributes(Vec<(String, String)>);

impl Attributes {
    fn parse(text: &str) -> Self {
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

    fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn flag(&self, name: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::fixtures;

    const BASE: &str = "https://cdn.example.org/hls/master.m3u8";

    fn audio_names(master: &Master) -> Vec<&str> {
        master
            .audio
            .iter()
            .map(|audio| audio.name.as_str())
            .collect()
    }

    #[test]
    fn a_master_playlist_is_read_with_its_audio_group() {
        let master = parse_master(fixtures::HLS_MASTER, BASE).expect("a master");
        assert_eq!(master.variants.len(), 2);

        let low = &master.variants[0];
        assert_eq!(low.id, "v0");
        assert_eq!(low.width, Some(640));
        assert_eq!(low.height, Some(360));
        assert_eq!(low.bandwidth, Some(800_000));
        assert_eq!(low.frame_rate, Some(30.0));
        assert_eq!(low.audio_group.as_deref(), Some("aud"));
        assert_eq!(
            low.uri, "https://cdn.example.org/hls/low/index.m3u8",
            "a relative address is resolved against the playlist"
        );

        let high = &master.variants[1];
        assert_eq!(high.height, Some(1080));
        assert_eq!(high.frame_rate, Some(60.0));

        assert_eq!(audio_names(&master), vec!["English"]);
        let audio = &master.audio[0];
        assert_eq!(audio.group, "aud");
        assert!(audio.default);
        assert_eq!(audio.uri, "https://cdn.example.org/hls/audio.m3u8");
        assert_eq!(master.audio_group("aud").len(), 1);
    }

    #[test]
    fn a_media_playlist_is_read_with_its_init_segment() {
        let media = parse_media(fixtures::HLS_MEDIA, BASE).expect("a media playlist");
        assert_eq!(media.segments.len(), 2);
        assert!(media.ended);
        assert_eq!(media.duration, 7.5);
        assert_eq!(media.target_duration, Some(4.0));
        assert_eq!(media.version, Some(7));

        let init = media.init.expect("an init segment");
        assert_eq!(init.url, "https://cdn.example.org/hls/init.mp4");
        assert_eq!(init.duration, 0.0);

        assert_eq!(
            media.segments[0].url,
            "https://cdn.example.org/hls/seg0.m4s"
        );
        assert_eq!(media.segments[0].duration, 4.0);
        assert_eq!(media.segments[1].duration, 3.5);
    }

    #[test]
    fn a_byte_range_carries_its_offset_and_a_bare_length_follows_the_previous_one() {
        let text = "#EXTM3U\n\
            #EXT-X-VERSION:4\n\
            #EXT-X-TARGETDURATION:4\n\
            #EXTINF:4,\n\
            #EXT-X-BYTERANGE:100@0\n\
            all.ts\n\
            #EXTINF:4,\n\
            #EXT-X-BYTERANGE:50\n\
            all.ts\n\
            #EXT-X-ENDLIST\n";
        let media = parse_media(text, "https://cdn.example.org/a/index.m3u8").expect("a playlist");
        assert_eq!(media.segments[0].range, Some((0, 99)));
        assert_eq!(media.segments[1].range, Some((100, 149)));
        assert_eq!(media.segments[0].url, media.segments[1].url);
    }

    #[test]
    fn a_relative_address_is_resolved_against_the_real_playlist() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXTINF:2,\n../parts/a.ts\n#EXT-X-ENDLIST\n";
        let media =
            parse_media(text, "https://cdn.example.org/live/720/index.m3u8").expect("a playlist");
        assert_eq!(
            media.segments[0].url,
            "https://cdn.example.org/live/parts/a.ts"
        );
    }

    #[test]
    fn an_encrypted_playlist_is_refused_with_one_sentence() {
        let error = parse_media(fixtures::HLS_ENCRYPTED, BASE).expect_err("encrypted");
        assert_eq!(error, MediaError::Protected);
        assert!(error.to_string().contains("protected"));
    }

    #[test]
    fn a_key_of_method_none_is_not_encryption() {
        let text = "#EXTM3U\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:2,\na.ts\n#EXT-X-ENDLIST\n";
        assert!(parse_media(text, BASE).is_ok());
    }

    #[test]
    fn a_truncated_playlist_is_refused() {
        let error = parse_master(fixtures::HLS_BAD, BASE).expect_err("truncated");
        assert!(matches!(error, MediaError::Malformed(_)), "{error:?}");
    }

    #[test]
    fn a_media_playlist_without_an_end_is_live() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXTINF:2,\na.ts\n";
        assert_eq!(parse_media(text, BASE).unwrap_err(), MediaError::Live);
    }

    #[test]
    fn a_playlist_without_the_header_is_refused() {
        assert!(matches!(
            parse_media("#EXTINF:2,\na.ts\n", BASE),
            Err(MediaError::Malformed(_))
        ));
    }

    #[test]
    fn the_kinds_of_a_playlist_are_told_apart() {
        assert!(looks_like_master(fixtures::HLS_MASTER));
        assert!(!looks_like_master(fixtures::HLS_MEDIA));
        assert!(
            parse(fixtures::HLS_MASTER, BASE)
                .expect("a manifest")
                .container_hint
                .as_deref()
                == Some("hls")
        );
    }

    #[test]
    fn a_playlist_over_the_segment_cap_is_refused() {
        let mut text = String::from(
            "#EXTM3U
#EXT-X-TARGETDURATION:1
",
        );
        for _ in 0..=MAX_SEGMENTS {
            text.push_str(
                "#EXTINF:1,
a.ts
",
            );
        }
        text.push_str(
            "#EXT-X-ENDLIST
",
        );
        assert!(matches!(
            parse_media(&text, BASE),
            Err(MediaError::TooLarge(_))
        ));
    }

    #[test]
    fn a_playlist_longer_than_twelve_hours_is_refused() {
        let text = "#EXTM3U
#EXT-X-TARGETDURATION:18000
            #EXTINF:18000,
a.ts
#EXTINF:18000,
b.ts
#EXTINF:18000,
c.ts
#EXT-X-ENDLIST
";
        assert!(matches!(
            parse_media(text, BASE),
            Err(MediaError::TooLarge(_))
        ));
    }

    #[test]
    fn an_attribute_list_keeps_quoted_commas_together() {
        let attrs = Attributes::parse(r#"BANDWIDTH=1,CODECS="avc1.4d401e,mp4a.40.2",AUDIO="aud""#);
        assert_eq!(attrs.get("CODECS"), Some("avc1.4d401e,mp4a.40.2"));
        assert_eq!(attrs.get("AUDIO"), Some("aud"));
        assert_eq!(attrs.get("BANDWIDTH"), Some("1"));
    }
}
