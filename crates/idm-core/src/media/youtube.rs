//! YouTube's stream numbers, and the old `fmt_stream_map` shape.
//!
//! The default path never needs this module: the page's player has already
//! requested a signed `videoplayback` URL, and the app downloads that URL as a
//! plain ranged file. What this module adds is the quality vocabulary of the
//! itag table (so a row says `1080p` instead of `file`) and a parser for a
//! player response whose URLs are **already signed**.
//!
//! There is no signature solver and no `n` solver here, and there never will
//! be. A map entry whose URL has no `signature` is skipped; appending the raw
//! `s` is the reference product's bug and ends in `403`.

use std::collections::HashMap;

use reqwest::Url;

use super::MediaKind;

/// What one itag carries, in the vocabulary of the research's table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Itag {
    pub container: &'static str,
    pub kind: MediaKind,
    pub height: Option<u32>,
    pub fps: Option<f64>,
}

impl Itag {
    const fn video(container: &'static str, height: u32) -> Self {
        Self {
            container,
            kind: MediaKind::Video,
            height: Some(height),
            fps: None,
        }
    }

    const fn hfr(container: &'static str, height: u32) -> Self {
        Self {
            container,
            kind: MediaKind::Video,
            height: Some(height),
            fps: Some(60.0),
        }
    }

    const fn muxed(container: &'static str, height: u32) -> Self {
        Self {
            container,
            kind: MediaKind::Muxed,
            height: Some(height),
            fps: None,
        }
    }

    const fn audio(container: &'static str) -> Self {
        Self {
            container,
            kind: MediaKind::Audio,
            height: None,
            fps: None,
        }
    }
}

/// The itag table, from the research (doc 10, step 6) plus the newer HFR
/// numbers that carry the same meaning.
pub fn describe(itag: u64) -> Option<Itag> {
    let value = match itag {
        5 => Itag::muxed("flv", 240),
        6 => Itag::muxed("flv", 270),
        17 => Itag::muxed("3gp", 144),
        18 => Itag::muxed("mp4", 360),
        22 => Itag::muxed("mp4", 720),
        34 => Itag::muxed("flv", 360),
        35 => Itag::muxed("flv", 480),
        36 => Itag::muxed("3gp", 240),
        37 => Itag::muxed("mp4", 1080),
        38 => Itag::muxed("mp4", 2160),
        43 => Itag::muxed("webm", 360),
        44 => Itag::muxed("webm", 480),
        45 => Itag::muxed("webm", 720),
        46 => Itag::muxed("webm", 1080),
        82 => Itag::muxed("mp4", 360),
        83 => Itag::muxed("mp4", 480),
        84 => Itag::muxed("mp4", 720),
        85 => Itag::muxed("mp4", 1080),
        133 => Itag::video("mp4", 240),
        134 => Itag::video("mp4", 360),
        135 => Itag::video("mp4", 480),
        136 => Itag::video("mp4", 720),
        137 => Itag::video("mp4", 1080),
        138 => Itag::video("mp4", 2160),
        160 => Itag::video("mp4", 144),
        171 => Itag::audio("m4a"),
        172 => Itag::audio("m4a"),
        242 => Itag::video("webm", 240),
        243 => Itag::video("webm", 360),
        244 => Itag::video("webm", 480),
        245 => Itag::video("webm", 480),
        246 => Itag::video("webm", 480),
        247 => Itag::video("webm", 720),
        248 => Itag::video("webm", 1080),
        264 => Itag::video("mp4", 1080),
        266 => Itag::video("mp4", 2160),
        271 => Itag::video("webm", 1440),
        272 => Itag::video("webm", 2160),
        278 => Itag::video("webm", 144),
        298 => Itag::hfr("mp4", 720),
        299 => Itag::hfr("mp4", 1080),
        302 => Itag::hfr("webm", 720),
        303 => Itag::hfr("webm", 1080),
        308 => Itag::hfr("webm", 1440),
        313 => Itag::video("webm", 2160),
        315 => Itag::hfr("webm", 2160),
        330 => Itag::hfr("webm", 144),
        331 => Itag::hfr("webm", 240),
        332 => Itag::hfr("webm", 360),
        333 => Itag::hfr("webm", 480),
        334 => Itag::hfr("webm", 720),
        335 => Itag::hfr("webm", 1080),
        336 => Itag::hfr("webm", 1440),
        337 => Itag::hfr("webm", 2160),
        139 => Itag::audio("m4a"),
        140 => Itag::audio("m4a"),
        141 => Itag::audio("m4a"),
        249 => Itag::audio("webm"),
        250 => Itag::audio("webm"),
        251 => Itag::audio("webm"),
        256 => Itag::audio("m4a"),
        258 => Itag::audio("m4a"),
        325 => Itag::audio("m4a"),
        327 => Itag::audio("m4a"),
        328 => Itag::audio("webm"),
        338 => Itag::audio("webm"),
        380 => Itag::audio("m4a"),
        _ => return None,
    };
    Some(value)
}

/// One entry of an old `fmt_stream_map` that may be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedFormat {
    pub url: String,
    pub itag: Option<u64>,
    pub size: Option<u64>,
}

/// Read `adaptive_fmts` or `url_encoded_fmt_stream_map`.
///
/// Entries whose URL does not already contain `signature=` are skipped. The
/// raw `s` value is never appended: that is what makes this path safe to keep
/// as a fallback instead of a weekly cipher chase.
pub fn parse_fmt_stream_map(text: &str) -> Vec<SignedFormat> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in text.split(',') {
        let Some(mut values) = query_values(entry) else {
            continue;
        };
        let Some(url) = values.remove("url").filter(|url| !url.is_empty()) else {
            continue;
        };
        let signed = Url::parse(&url)
            .ok()
            .map(|url| url.query_pairs().any(|(name, _)| name == "signature"))
            .unwrap_or(false);
        if !signed {
            // `s`/`sig` alone is not a usable address. Never build one.
            continue;
        }
        if !seen.insert(url.clone()) {
            continue;
        }
        out.push(SignedFormat {
            url,
            itag: values
                .get("itag")
                .and_then(|value| value.parse::<u64>().ok()),
            size: values
                .get("clen")
                .and_then(|value| value.parse::<u64>().ok()),
        });
    }
    out
}

fn query_values(entry: &str) -> Option<HashMap<String, String>> {
    let entry = entry.trim().trim_start_matches('&');
    if entry.is_empty() {
        return None;
    }
    let url = Url::parse(&format!("https://example.invalid/?{entry}")).ok()?;
    let values: HashMap<String, String> = url.query_pairs().into_owned().collect();
    (!values.is_empty()).then_some(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_names_a_picture_and_never_invents_a_number() {
        let itag = describe(137).expect("1080p");
        assert_eq!(itag.kind, MediaKind::Video);
        assert_eq!(itag.container, "mp4");
        assert_eq!(itag.height, Some(1080));
        assert_eq!(itag.fps, None);

        let hfr = describe(299).expect("1080p60");
        assert_eq!(hfr.fps, Some(60.0));

        let muxed = describe(22).expect("720p");
        assert_eq!(muxed.kind, MediaKind::Muxed);

        let audio = describe(251).expect("opus");
        assert_eq!(audio.kind, MediaKind::Audio);
        assert_eq!(audio.container, "webm");
        assert_eq!(audio.height, None);

        assert!(describe(9999).is_none());
    }

    #[test]
    fn only_a_url_that_already_carries_a_signature_is_kept() {
        let signed = "url=https%3A%2F%2Frr1---sn.googlevideo.com%2Fvideoplayback%3Fitag%3D137%26clen%3D331000000%26signature%3Dabc&itag=137&clen=331000000";
        let unsigned = "url=https%3A%2F%2Frr1---sn.googlevideo.com%2Fvideoplayback%3Fitag%3D137%26s%3Dobfuscated&itag=137&s=obfuscated";
        let formats = parse_fmt_stream_map(&format!("{signed},{unsigned}"));
        assert_eq!(formats.len(), 1);
        assert_eq!(formats[0].itag, Some(137));
        assert_eq!(formats[0].size, Some(331_000_000));
        assert!(formats[0].url.contains("signature=abc"));
        assert!(!formats[0].url.contains("s=obfuscated"));
    }

    #[test]
    fn a_repeated_url_is_offered_once() {
        let entry = "url=https%3A%2F%2Fa.example%2Fv%3Fitag%3D18%26signature%3Dx&itag=18";
        let formats = parse_fmt_stream_map(&format!("{entry},{entry}"));
        assert_eq!(formats.len(), 1);
    }

    #[test]
    fn an_entry_without_a_url_is_skipped() {
        assert!(parse_fmt_stream_map("itag=137&s=abc").is_empty());
    }
}
