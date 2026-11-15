//! Reading a master playlist: its variants and audio renditions.

use super::*;

/// Read a master playlist.
pub fn parse_master(text: &str, base_url: &str) -> Result<Master, MediaError> {
    require_header(text)?;
    // A protected playlist is refused before a single byte is fetched.
    if let Some(scheme) = drm::in_hls_playlist(text) {
        return Err(drm::error(scheme));
    }

    let mut master = Master::default();
    let mut pending: Option<PendingVariant> = None;
    let mut audio: Vec<AudioRendition> = Vec::new();
    let mut subtitle_renditions: Vec<SubtitleRendition> = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attrs = Attributes::parse(rest);
            let kind = attrs.get("TYPE").unwrap_or("");
            if kind.eq_ignore_ascii_case("SUBTITLES") {
                // A rendition without a URI carries its text inside the
                // variant, and there is nothing to download on its own.
                let Some(uri) = attrs.get("URI").filter(|uri| !uri.is_empty()) else {
                    continue;
                };
                let url = resolve(base_url, uri)?;
                subtitle_renditions.push(SubtitleRendition {
                    id: format!("s{}", subtitle_renditions.len()),
                    name: attrs.get("NAME").unwrap_or("Subtitles").to_string(),
                    language: attrs.get("LANGUAGE").map(str::to_string),
                    container: subtitles::container_of(&url),
                    url,
                    is_default: attrs.flag("DEFAULT"),
                });
                continue;
            }
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
    master.subtitles = subtitle_renditions;

    if master.variants.is_empty() && master.audio.is_empty() {
        return Err(MediaError::Malformed(
            "the playlist names no stream".to_string(),
        ));
    }
    Ok(master)
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
