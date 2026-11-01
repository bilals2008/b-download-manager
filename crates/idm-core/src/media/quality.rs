//! Turning parsed formats into the rows and pairings the quality panel shows.
//!
//! The vocabulary follows the reference product (resolution first, `4K` after
//! `2160p`, `60fps` only when the manifest states it, `high bitrate` only when
//! the manifest proves it) without copying its typos. A number that the
//! manifest did not state never appears in a label or a size.

use super::hls;
use super::{Format, Manifest, MediaKind, Pair};

/// Build the visible manifest from an HLS master.
pub fn manifest_from_hls(master: &hls::Master, duration: Option<f64>) -> Manifest {
    let mut formats: Vec<Format> = Vec::new();
    let mut pairs: Vec<Pair> = Vec::new();

    // Audio renditions become formats of their own, in group order.
    let mut audio_formats: Vec<Format> = Vec::new();
    for rendition in &master.audio {
        let container = audio_container(rendition.codecs.as_deref());
        let mut format = Format::new(&rendition.id, MediaKind::Audio, &container);
        format.codec = rendition.codecs.clone();
        format.label = audio_label(None);
        audio_formats.push(format);
    }

    for variant in &master.variants {
        let container = video_container(variant.codecs.as_deref());
        let kind = if variant.audio_group.is_some() {
            MediaKind::Video
        } else {
            MediaKind::Muxed
        };
        let mut format = Format::new(&variant.id, kind, container);
        format.codec = variant.codecs.clone();
        format.width = variant.width;
        format.height = variant.height;
        format.fps = variant.frame_rate;
        format.bandwidth = variant.bandwidth.or(variant.average_bandwidth);
        format.size = format.estimate_size(duration);
        formats.push(format);
    }

    finish(&mut formats, &mut pairs, duration);
    for format in audio_formats {
        formats.push(format);
    }

    // One pair per variant, audio paired from its group.
    for variant in &master.variants {
        let audio = variant
            .audio_group
            .as_deref()
            .and_then(|group| best_audio(master, group));
        let label = formats
            .iter()
            .find(|format| format.id == variant.id)
            .map(|format| format.label.clone())
            .unwrap_or_default();
        pairs.push(Pair {
            video: variant.id.clone(),
            audio: audio.map(|rendition| rendition.id.clone()),
            label,
        });
    }

    Manifest {
        title: String::new(),
        duration,
        formats,
        pairs,
        container_hint: Some("hls".to_string()),
    }
}

/// The one row of a media playlist fetched directly.
pub fn hls_media_format(media: &hls::MediaPlaylist) -> Format {
    let container = media
        .init
        .as_ref()
        .map(|init| container_of_url(&init.url))
        .or_else(|| {
            media
                .segments
                .first()
                .map(|segment| container_of_url(&segment.url))
        })
        .unwrap_or_else(|| "ts".to_string());
    let mut format = Format::new("media", MediaKind::Muxed, container);
    format.size = media
        .segments
        .iter()
        .map(|segment| segment.range.map(|(start, end)| end - start + 1))
        .sum::<Option<u64>>();
    format
}

/// Fill the labels, the sizes and the pairing of a manifest.
///
/// `formats` and `pairs` are built by the format module (HLS, DASH); this is
/// the one place that decides how they read.
pub fn finish(formats: &mut [Format], pairs: &mut [Pair], duration: Option<f64>) {
    for format in formats.iter_mut() {
        if format.size.is_none() {
            format.size = format.estimate_size(duration);
        }
    }

    let show_container = {
        let mut containers: Vec<&str> = formats
            .iter()
            .filter(|format| format.kind != MediaKind::Audio)
            .map(|format| format.container.as_str())
            .collect();
        containers.sort_unstable();
        containers.dedup();
        containers.len() > 1
    };

    // The bandwidth of every visual format with the same picture, to decide
    // whether one is more than 1.5 times another.
    let high: Vec<bool> = formats
        .iter()
        .map(|format| {
            if format.kind == MediaKind::Audio {
                return false;
            }
            let Some(bandwidth) = format.bandwidth else {
                return false;
            };
            formats.iter().any(|other| {
                other.kind != MediaKind::Audio
                    && other.id != format.id
                    && other.width == format.width
                    && other.height == format.height
                    && other.fps == format.fps
                    && other
                        .bandwidth
                        .is_some_and(|other| bandwidth > other.saturating_mul(3) / 2)
            })
        })
        .collect();

    for (index, format) in formats.iter_mut().enumerate() {
        format.label = if format.kind == MediaKind::Audio {
            audio_label(format.bandwidth)
        } else {
            visual_label(format, show_container, high[index])
        };
    }

    for pair in pairs.iter_mut() {
        if let Some(format) = formats.iter().find(|format| format.id == pair.video) {
            pair.label = format.label.clone();
        }
    }
}

/// Pair a list of plain file formats: every picture with the sound in the
/// container it can be copied into, audio-only tracks left on their own.
///
/// A muxed file already carries its sound. A video-only file whose page never
/// offered an audio address stays a video-only pair, which the panel marks.
pub fn pair_file_formats(formats: &[Format]) -> Vec<Pair> {
    let audio: Vec<&Format> = formats
        .iter()
        .filter(|format| format.kind == MediaKind::Audio)
        .collect();
    let mut pairs = Vec::new();
    for video in formats
        .iter()
        .filter(|format| format.kind != MediaKind::Audio)
    {
        let audio_id = if video.kind == MediaKind::Muxed {
            None
        } else {
            let wanted = match video.container.as_str() {
                "mp4" | "m4s" | "ts" => Some("m4a"),
                "webm" => Some("webm"),
                _ => None,
            };
            wanted
                .and_then(|wanted| {
                    audio
                        .iter()
                        .find(|audio| audio.container == wanted)
                        .copied()
                })
                .or_else(|| audio.first().copied())
                .map(|audio| audio.id.clone())
        };
        pairs.push(Pair {
            video: video.id.clone(),
            audio: audio_id,
            label: String::new(),
        });
    }
    pairs
}

/// The audio rendition a variant should use: the default one when the group
/// marks one, otherwise the first the manifest named.
fn best_audio<'a>(master: &'a hls::Master, group: &str) -> Option<&'a hls::AudioRendition> {
    let renditions: Vec<&hls::AudioRendition> = master.audio_group(group);
    renditions
        .iter()
        .copied()
        .find(|rendition| rendition.default)
        .or_else(|| renditions.first().copied())
}

fn visual_label(format: &Format, show_container: bool, high_bitrate: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if show_container {
        parts.push(format.container.to_ascii_uppercase());
    }
    if let Some(height) = format.height {
        if height >= 2160 {
            parts.push(format!("{height}p 4K"));
        } else {
            parts.push(format!("{height}p"));
        }
    }
    if let Some(fps) = format.fps {
        if fps >= 50.0 {
            parts.push(format!("{fps:.0}fps"));
        }
    }
    if format.codec.as_deref().is_some_and(is_hdr) {
        parts.push("HDR".to_string());
    }
    if high_bitrate {
        parts.push("high bitrate".to_string());
    }
    if parts.is_empty() {
        // No resolution and no rate in the manifest: the container is the
        // only fact there is.
        parts.push(format.container.to_ascii_uppercase());
    }
    parts.join(" ")
}

fn audio_label(bandwidth: Option<u64>) -> String {
    match bandwidth.filter(|bits| *bits > 0) {
        Some(bits) => format!("Audio {} kbps", (bits + 500) / 1000),
        None => "Audio".to_string(),
    }
}

fn is_hdr(codec: &str) -> bool {
    let codec = codec.to_ascii_lowercase();
    codec.contains("hdr") || codec.contains("dvh") || codec.contains("hvc1.2")
}

/// The container a video codec belongs to.
///
/// The mux table decides the output file; this only names what the manifest
/// carries, so `avc1`/`hev1` are MP4, VP9/AV1 are WebM, and a playlist with no
/// codec at all is the TS stream HLS has always been.
pub fn video_container(codecs: Option<&str>) -> String {
    let Some(codecs) = codecs else {
        return "ts".to_string();
    };
    let codecs = codecs.to_ascii_lowercase();
    if codecs.contains("avc1")
        || codecs.contains("hvc1")
        || codecs.contains("hev1")
        || codecs.contains("mp4a")
        || codecs.contains("ac-3")
        || codecs.contains("ec-3")
    {
        return "mp4".to_string();
    }
    if codecs.contains("vp09")
        || codecs.contains("vp9")
        || codecs.contains("av01")
        || codecs.contains("opus")
        || codecs.contains("vorbis")
    {
        return "webm".to_string();
    }
    "mp4".to_string()
}

pub fn audio_container(codecs: Option<&str>) -> String {
    let Some(codecs) = codecs else {
        return "m4a".to_string();
    };
    let codecs = codecs.to_ascii_lowercase();
    if codecs.contains("opus") || codecs.contains("vorbis") || codecs.contains("vp09") {
        "webm".to_string()
    } else {
        "m4a".to_string()
    }
}

fn container_of_url(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match extension.as_str() {
        "mp4" | "m4s" | "m4a" | "ts" => extension,
        "webm" | "mkv" => "webm".to_string(),
        _ => "ts".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(id: &str, height: u32, fps: Option<f64>, bandwidth: u64) -> Format {
        let mut format = Format::new(id, MediaKind::Video, "mp4");
        format.height = Some(height);
        format.fps = fps;
        format.bandwidth = Some(bandwidth);
        format
    }

    #[test]
    fn a_resolution_is_the_first_thing_a_row_says() {
        let mut formats = vec![format("a", 1080, Some(60.0), 5_000_000)];
        let mut pairs = Vec::new();
        finish(&mut formats, &mut pairs, Some(10.0));
        assert_eq!(formats[0].label, "1080p 60fps");
        assert_eq!(formats[0].size, Some(6_250_000));

        let mut formats = vec![format("a", 2160, None, 20_000_000)];
        finish(&mut formats, &mut pairs, None);
        assert_eq!(formats[0].label, "2160p 4K");
    }

    #[test]
    fn a_container_is_named_only_when_there_is_a_choice() {
        let mut formats = vec![format("a", 1080, None, 1_000)];
        finish(&mut formats, &mut Vec::new(), None);
        assert_eq!(formats[0].label, "1080p");

        let mut other = format("b", 720, None, 1_000);
        other.container = "webm".to_string();
        let mut formats = vec![format("a", 1080, None, 1_000), other];
        finish(&mut formats, &mut Vec::new(), None);
        assert_eq!(formats[0].label, "MP4 1080p");
        assert_eq!(formats[1].label, "WEBM 720p");
    }

    #[test]
    fn a_frame_rate_below_fifty_is_not_a_label() {
        let mut formats = vec![format("a", 720, Some(30.0), 1_000)];
        finish(&mut formats, &mut Vec::new(), None);
        assert_eq!(formats[0].label, "720p");
    }

    #[test]
    fn high_bitrate_is_only_said_when_one_is_one_and_a_half_times_another() {
        let mut formats = vec![
            format("a", 1080, Some(30.0), 4_000_000),
            format("b", 1080, Some(30.0), 8_000_000),
        ];
        finish(&mut formats, &mut Vec::new(), None);
        assert!(formats[1].label.ends_with("high bitrate"));
        assert!(!formats[0].label.contains("high bitrate"));

        let mut formats = vec![
            format("a", 1080, Some(30.0), 5_000_000),
            format("b", 1080, Some(30.0), 6_000_000),
        ];
        finish(&mut formats, &mut Vec::new(), None);
        assert!(!formats
            .iter()
            .any(|format| format.label.contains("high bitrate")));
    }

    #[test]
    fn an_audio_row_says_the_rate_the_manifest_stated() {
        assert_eq!(audio_label(Some(128_000)), "Audio 128 kbps");
        assert_eq!(audio_label(None), "Audio");
        assert_eq!(audio_label(Some(0)), "Audio");
    }

    #[test]
    fn an_hls_master_is_paired_with_its_audio_group() {
        use crate::media::fixtures;
        let master = hls::parse_master(
            fixtures::HLS_MASTER,
            "https://cdn.example.org/hls/master.m3u8",
        )
        .expect("master");
        let manifest = manifest_from_hls(&master, None);
        assert_eq!(manifest.pairs.len(), 2);
        assert_eq!(manifest.pairs[0].video, "v0");
        assert_eq!(manifest.pairs[0].audio.as_deref(), Some("a0"));
        assert_eq!(manifest.pairs[1].video, "v1");
        assert_eq!(
            manifest
                .formats
                .iter()
                .filter(|f| f.kind == MediaKind::Audio)
                .count(),
            1
        );
        assert_eq!(
            manifest.formats.last().map(|f| f.kind),
            Some(MediaKind::Audio)
        );
        assert_eq!(
            manifest.pairs[1].label, "1080p 60fps",
            "two MP4 variants do not need a container prefix"
        );
    }

    #[test]
    fn file_formats_are_paired_by_container() {
        let mut mp4 = Format::new("137", MediaKind::Video, "mp4");
        mp4.height = Some(1080);
        let mut webm = Format::new("248", MediaKind::Video, "webm");
        webm.height = Some(1080);
        let m4a = Format::new("140", MediaKind::Audio, "m4a");
        let opus = Format::new("251", MediaKind::Audio, "webm");
        let mut muxed = Format::new("22", MediaKind::Muxed, "mp4");
        muxed.height = Some(720);
        let mut formats = vec![mp4, webm, m4a, opus, muxed];
        let mut pairs = pair_file_formats(&formats);
        finish(&mut formats, &mut pairs, None);
        assert_eq!(pairs[0].video, "137");
        assert_eq!(pairs[0].audio.as_deref(), Some("140"));
        assert_eq!(pairs[1].video, "248");
        assert_eq!(pairs[1].audio.as_deref(), Some("251"));
        assert!(
            pairs[2].audio.is_none(),
            "a muxed file needs no second track"
        );
    }

    #[test]
    fn a_codec_names_its_container() {
        assert_eq!(video_container(Some("avc1.640028")), "mp4");
        assert_eq!(video_container(Some("vp09.00.10.08")), "webm");
        assert_eq!(video_container(Some("av01.0.08M.08")), "webm");
        assert_eq!(video_container(None), "ts");
        assert_eq!(audio_container(Some("opus")), "webm");
        assert_eq!(audio_container(Some("mp4a.40.2")), "m4a");
    }
}
