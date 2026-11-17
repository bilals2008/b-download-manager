//! Choosing the sound that goes with a video, and naming the file it lands in.
//!
//! The reference product asks its downloader for a track with a selector
//! (`b`, `ba`, `mp3:320`) and lets ffmpeg convert the sound afterwards. What
//! is kept here is the part that can be decided without either: what each
//! selector means, which track it picks out of a manifest, and the container
//! the file is saved in when it is saved at all.
//!
//! One of the choices needs a word of warning rather than a row of its own:
//! a FLAC file is lossless as a format, but the sound it holds is only as
//! lossless as the source it was made from. The choice says so instead of
//! promising something the bytes cannot keep.

use serde::{Deserialize, Serialize};

use super::{Format, MediaKind, Pair};

/// The MP3 bitrates the panel offers, best first.
///
/// The list is the reference product's: a rate nobody asked for is not
/// offered, because every rate is a promise about the size of the file.
pub const MP3_RATES: [u32; 4] = [320, 256, 192, 128];

/// The note a FLAC choice carries.
///
/// The container keeps everything it is given, so a lossy source becomes a
/// lossy file with a lossless name. Saying this once, in the place where the
/// choice is made, is better than a footnote nobody reads.
pub const LOSSY_SOURCE: &str = "the source may be lossy, so this file is not truly lossless";

/// How good an MP3 is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mp3Quality {
    /// The best variable bitrate (`mp3:0`): the encoder picks the rate per
    /// part of the sound, so the file is as good as the source allows.
    Variable,
    /// A constant rate (`mp3:320` and the others): the same rate everywhere.
    Constant(u32),
}

/// What an audio selector asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioSelector {
    /// `b`: the best single file, the picture and its sound together.
    Best,
    /// `ba`: the sound as the site has it, not converted.
    Source,
    /// `mp3:`: the best sound written as MP3.
    Mp3(Mp3Quality),
    /// `opus:`: the best sound written as Opus.
    Opus,
    /// `flac:`: the best sound written as FLAC.
    Flac,
}

impl AudioSelector {
    /// The selector the reference product writes for this choice.
    ///
    /// The text is built rather than looked up, so a rate that was never
    /// offered still writes itself out: the selector is the caller's
    /// business, this module only has to name it.
    pub fn as_str(self) -> String {
        match self {
            AudioSelector::Best => "b".to_string(),
            AudioSelector::Source => "ba".to_string(),
            AudioSelector::Mp3(Mp3Quality::Variable) => "mp3:0".to_string(),
            AudioSelector::Mp3(Mp3Quality::Constant(kbps)) => format!("mp3:{kbps}"),
            AudioSelector::Opus => "opus:".to_string(),
            AudioSelector::Flac => "flac:".to_string(),
        }
    }

    /// Read a selector the reference product writes.
    ///
    /// A selector that is not one of the offered ones is refused with a
    /// sentence that says which ones there are, because a typo in a setting
    /// should not be answered with silence.
    pub fn parse(selector: &str) -> Result<Self, AudioError> {
        let selector = selector.trim().to_ascii_lowercase();
        match selector.as_str() {
            "b" => return Ok(AudioSelector::Best),
            "ba" => return Ok(AudioSelector::Source),
            // A bare name is the best sound in that container, the same way
            // `opus` and `flac` are read without their colon.
            "mp3" => return Ok(AudioSelector::Mp3(Mp3Quality::Variable)),
            "opus" | "opus:" => return Ok(AudioSelector::Opus),
            "flac" | "flac:" => return Ok(AudioSelector::Flac),
            _ => {}
        }
        if let Some(rate) = selector.strip_prefix("mp3:") {
            if rate == "0" {
                return Ok(AudioSelector::Mp3(Mp3Quality::Variable));
            }
            match rate.parse::<u32>() {
                Ok(kbps) if MP3_RATES.contains(&kbps) => {
                    return Ok(AudioSelector::Mp3(Mp3Quality::Constant(kbps)))
                }
                Ok(kbps) => {
                    return Err(AudioError::unknown(
                        &selector,
                        format!("MP3 at {kbps} kbps is not one of the offered rates"),
                    ))
                }
                Err(_) => {
                    return Err(AudioError::unknown(
                        &selector,
                        "the MP3 rate must be a number of kbps",
                    ))
                }
            }
        }
        Err(AudioError::unknown(
            &selector,
            "it is not a selector this program knows",
        ))
    }

    /// The container this choice is saved in, when it is converted.
    ///
    /// `Best` and `Source` save what the site gave, so they name no
    /// container of their own.
    pub fn container(self) -> Option<&'static str> {
        match self {
            AudioSelector::Mp3(_) => Some("mp3"),
            AudioSelector::Opus => Some("opus"),
            AudioSelector::Flac => Some("flac"),
            AudioSelector::Best | AudioSelector::Source => None,
        }
    }
    /// The constant rate this choice is written at, when it has one.
    ///
    /// `None` for the choices that are not written at a fixed rate: a
    /// variable MP3, an Opus and a FLAC are as big as the sound needs them to
    /// be, and a number would be a guess.
    pub fn bitrate_kbps(self) -> Option<u32> {
        match self {
            AudioSelector::Mp3(Mp3Quality::Constant(kbps)) => Some(kbps),
            _ => None,
        }
    }

    /// The extension the output file is written with.
    ///
    /// A conversion names its own extension. The original track keeps the
    /// container it came in, so its extension is read from that track with
    /// [`AudioSelector::extension_for`].
    pub fn extension(self) -> &'static str {
        match self {
            AudioSelector::Mp3(_) => "mp3",
            AudioSelector::Opus => "opus",
            AudioSelector::Flac => "flac",
            AudioSelector::Best | AudioSelector::Source => "",
        }
    }

    /// The extension the output file is written with, for a track that came
    /// in a container of its own.
    ///
    /// The original track is written out as it was downloaded, so the file
    /// keeps the container it came in (`m4a`, `webm`, `mp4`).
    pub fn extension_for(self, container: &str) -> String {
        match self {
            AudioSelector::Best | AudioSelector::Source => {
                let container = container
                    .trim()
                    .trim_start_matches('.')
                    .to_ascii_lowercase();
                match container.as_str() {
                    "m4a" | "aac" | "mp4" | "m4v" | "m4s" | "webm" | "opus" | "ogg" | "mp3"
                    | "flac" | "wav" | "ts" => container,
                    _ => "m4a".to_string(),
                }
            }
            _ => self.extension().to_string(),
        }
    }

    /// The FFmpeg codec the conversion uses, when a conversion happens.
    ///
    /// `None` for the original track: no conversion happens at all, so no
    /// codec is named and FFmpeg is never called.
    pub fn ffmpeg_codec(self) -> Option<&'static str> {
        match self {
            AudioSelector::Mp3(_) => Some("libmp3lame"),
            AudioSelector::Opus => Some("libopus"),
            AudioSelector::Flac => Some("flac"),
            AudioSelector::Best | AudioSelector::Source => None,
        }
    }

    /// Whether the choice converts the sound at all.
    pub fn is_conversion(self) -> bool {
        self.container().is_some()
    }
}

/// Why an audio choice could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// The selector is not one this program knows.
    UnknownSelector {
        /// What was written.
        selector: String,
        /// Why it was refused.
        detail: String,
    },
    /// The selector is understood, but this video has nothing to satisfy it.
    NotAvailable {
        /// The selector that was asked for.
        selector: AudioSelector,
        /// What the video lacks.
        reason: String,
    },
}

impl AudioError {
    fn unknown(selector: &str, detail: impl Into<String>) -> Self {
        AudioError::UnknownSelector {
            selector: selector.to_string(),
            detail: detail.into(),
        }
    }

    fn not_available(selector: AudioSelector, reason: &str) -> Self {
        AudioError::NotAvailable {
            selector,
            reason: reason.to_string(),
        }
    }
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::UnknownSelector { selector, detail } => write!(
                f,
                "unknown audio selector \"{selector}\": {detail}; the offered ones are b, ba, mp3:0, mp3:320, mp3:256, mp3:192, mp3:128, opus:, flac:"
            ),
            AudioError::NotAvailable { reason, .. } => {
                write!(f, "this choice is not available: {reason}")
            }
        }
    }
}

impl std::error::Error for AudioError {}

/// One sound the video can be listened to in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioPreference {
    /// What the choice asks the downloader for.
    pub selector: AudioSelector,
    /// The row the panel shows.
    pub label: String,
    /// The rate the file is written at, when it is written at a fixed one.
    pub bitrate_kbps: Option<u32>,
    /// The container the file is saved in.
    pub container: String,
    /// The note the panel shows with the row, when there is one to show.
    pub caveat: Option<String>,
}

/// The sounds a video offers, best first.
///
/// The original sound comes first because it is the one the video was made
/// with; the converted ones follow in the order the reference product
/// offers them, from the MP3 that keeps the most to the FLAC that keeps
/// everything the source had.
pub fn preferences(formats: &[Format]) -> Vec<AudioPreference> {
    let Some(source) = best_audio_format(formats) else {
        return Vec::new();
    };

    let mut out = vec![AudioPreference {
        selector: AudioSelector::Source,
        label: format!("Audio only original {}", source.container.to_uppercase()),
        bitrate_kbps: source.bandwidth.map(|bits| ((bits + 500) / 1000) as u32),
        container: source.container.clone(),
        caveat: None,
    }];

    out.push(conversion_preference(AudioSelector::Mp3(
        Mp3Quality::Variable,
    )));
    for kbps in MP3_RATES {
        out.push(conversion_preference(AudioSelector::Mp3(
            Mp3Quality::Constant(kbps),
        )));
    }
    out.push(conversion_preference(AudioSelector::Opus));
    out.push(conversion_preference(AudioSelector::Flac));

    out
}

/// The row of a choice that converts the sound.
fn conversion_preference(selector: AudioSelector) -> AudioPreference {
    let container = selector.container().unwrap_or("").to_string();
    let label = match selector {
        AudioSelector::Mp3(Mp3Quality::Variable) => "Audio only MP3 VBR".to_string(),
        AudioSelector::Mp3(Mp3Quality::Constant(kbps)) => {
            format!("Audio only MP3 {kbps} kbps")
        }
        AudioSelector::Opus => "Audio only OPUS".to_string(),
        AudioSelector::Flac => "Audio only FLAC".to_string(),
        AudioSelector::Best | AudioSelector::Source => String::new(),
    };
    AudioPreference {
        selector,
        label,
        bitrate_kbps: selector.bitrate_kbps(),
        container,
        caveat: match selector {
            AudioSelector::Flac => Some(LOSSY_SOURCE.to_string()),
            _ => None,
        },
    }
}

/// The track a choice ends up downloading.
///
/// The picture is `None` for a choice that keeps only the sound, and the
/// sound is `None` for a file that carries its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chosen {
    /// The picture track, when the choice keeps one.
    pub video: Option<String>,
    /// The sound track, when the choice is not a file with its own sound.
    pub audio: Option<String>,
    /// The container the file is saved in.
    pub container: String,
    /// The row the panel shows.
    pub label: String,
    /// The note the panel shows with the row, when there is one to show.
    pub caveat: Option<String>,
}

/// Pick the track a selector asks for.
///
/// The pairs are the manifest's own: a picture and the sound that goes with
/// it. A choice that is not on offer is refused with a plain sentence
/// rather than a fallback, because a file that is not what was asked for is
/// worse than no file.
pub fn choose(
    pairs: &[Pair],
    formats: &[Format],
    selector: &AudioSelector,
) -> Result<Chosen, AudioError> {
    match selector {
        AudioSelector::Best => {
            let mut best: Option<&Format> = None;
            for pair in pairs {
                let Some(video) = formats.iter().find(|format| format.id == pair.video) else {
                    continue;
                };
                if video.kind != MediaKind::Muxed {
                    continue;
                }
                let better = match best {
                    None => true,
                    Some(current) => is_better_video(video, current),
                };
                if better {
                    best = Some(video);
                }
            }
            let Some(video) = best else {
                return Err(AudioError::not_available(
                    *selector,
                    "no single file carries both the picture and the sound",
                ));
            };
            Ok(Chosen {
                video: Some(video.id.clone()),
                audio: None,
                container: video.container.clone(),
                label: video.label.clone(),
                caveat: None,
            })
        }
        AudioSelector::Source => {
            let audio = best_paired_audio(pairs, formats).ok_or_else(|| {
                AudioError::not_available(*selector, "this video has no sound track of its own")
            })?;
            Ok(Chosen {
                video: None,
                audio: Some(audio.id.clone()),
                container: audio.container.clone(),
                label: format!("Audio only original {}", audio.container.to_uppercase()),
                caveat: None,
            })
        }
        _ => {
            let audio = best_paired_audio(pairs, formats).ok_or_else(|| {
                AudioError::not_available(*selector, "this video has no sound track of its own")
            })?;
            let container = selector.container().unwrap_or("").to_string();
            let preference = conversion_preference(*selector);
            Ok(Chosen {
                video: None,
                audio: Some(audio.id.clone()),
                container,
                label: preference.label,
                caveat: preference.caveat,
            })
        }
    }
}

/// The best sound track of the manifest, whether it is paired or not.
fn best_audio_format(formats: &[Format]) -> Option<&Format> {
    formats
        .iter()
        .filter(|format| format.kind == MediaKind::Audio)
        .max_by(|a, b| a.bandwidth.cmp(&b.bandwidth).then(a.size.cmp(&b.size)))
}

/// The best sound track that a pair in the manifest names.
fn best_paired_audio<'a>(pairs: &[Pair], formats: &'a [Format]) -> Option<&'a Format> {
    pairs
        .iter()
        .filter_map(|pair| pair.audio.as_ref())
        .filter_map(|id| formats.iter().find(|format| &format.id == id))
        .max_by(|a, b| a.bandwidth.cmp(&b.bandwidth).then(a.size.cmp(&b.size)))
}

/// Whether one picture track is better than another.
fn is_better_video(candidate: &Format, current: &Format) -> bool {
    candidate
        .height
        .cmp(&current.height)
        .then(candidate.bandwidth.cmp(&current.bandwidth))
        .then(candidate.size.cmp(&current.size))
        .then_with(|| candidate.id.cmp(&current.id))
        .is_gt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(id: &str, height: u32) -> Format {
        let mut format = Format::new(id, MediaKind::Video, "mp4");
        format.height = Some(height);
        format.bandwidth = Some(2_000_000);
        format.label = format!("{height}p");
        format
    }

    fn muxed(id: &str, height: u32) -> Format {
        let mut format = Format::new(id, MediaKind::Muxed, "mp4");
        format.height = Some(height);
        format.bandwidth = Some(3_000_000);
        format.label = format!("{height}p");
        format
    }

    fn audio(id: &str, container: &str, bandwidth: u64) -> Format {
        let mut format = Format::new(id, MediaKind::Audio, container);
        format.bandwidth = Some(bandwidth);
        format
    }

    fn pair(video: &str, audio: Option<&str>) -> Pair {
        Pair {
            video: video.to_string(),
            audio: audio.map(str::to_string),
            label: String::new(),
        }
    }

    #[test]
    fn the_selectors_the_panel_offers_are_read() {
        assert_eq!(AudioSelector::parse("b").expect("b"), AudioSelector::Best);
        assert_eq!(
            AudioSelector::parse("ba").expect("ba"),
            AudioSelector::Source
        );
        assert_eq!(
            AudioSelector::parse("mp3:320").expect("mp3:320"),
            AudioSelector::Mp3(Mp3Quality::Constant(320))
        );
        assert_eq!(
            AudioSelector::parse("mp3:0").expect("mp3:0"),
            AudioSelector::Mp3(Mp3Quality::Variable)
        );
        assert_eq!(
            AudioSelector::parse("mp3").expect("mp3"),
            AudioSelector::Mp3(Mp3Quality::Variable),
            "the panel sends the bare name"
        );
        assert_eq!(
            AudioSelector::parse("opus:").expect("opus:"),
            AudioSelector::Opus
        );
        assert_eq!(
            AudioSelector::parse("flac:").expect("flac:"),
            AudioSelector::Flac
        );
    }

    #[test]
    fn a_selector_written_the_way_the_reference_writes_it_comes_back_unchanged() {
        for selector in [
            AudioSelector::Best,
            AudioSelector::Source,
            AudioSelector::Mp3(Mp3Quality::Variable),
            AudioSelector::Mp3(Mp3Quality::Constant(320)),
            AudioSelector::Opus,
            AudioSelector::Flac,
        ] {
            assert_eq!(
                AudioSelector::parse(&selector.as_str()).expect("round trip"),
                selector
            );
        }
    }

    #[test]
    fn an_unknown_selector_says_which_ones_there_are() {
        let error = AudioSelector::parse("mp3:100").expect_err("not offered");
        assert!(matches!(error, AudioError::UnknownSelector { .. }));
        let text = error.to_string();
        assert!(text.contains("mp3:100"));
        assert!(text.contains("mp3:320"));

        let error = AudioSelector::parse("wav").expect_err("not a selector");
        assert!(error.to_string().contains("wav"));
    }

    #[test]
    fn the_best_single_file_is_the_one_that_carries_its_own_sound() {
        let formats = vec![
            video("137", 1080),
            muxed("22", 720),
            muxed("18", 360),
            audio("140", "m4a", 128_000),
        ];
        let pairs = vec![pair("137", Some("140")), pair("22", None), pair("18", None)];
        let chosen = choose(&pairs, &formats, &AudioSelector::Best).expect("a single file");
        assert_eq!(chosen.video.as_deref(), Some("22"));
        assert_eq!(chosen.audio, None);
        assert_eq!(chosen.container, "mp4");
    }

    #[test]
    fn a_video_without_a_single_file_says_it_plainly() {
        let formats = vec![video("137", 1080), audio("140", "m4a", 128_000)];
        let pairs = vec![pair("137", Some("140"))];
        let error = choose(&pairs, &formats, &AudioSelector::Best).expect_err("no single file");
        let text = error.to_string();
        assert!(text.contains("not available"));
        assert!(text.contains("picture"));
    }

    #[test]
    fn the_source_sound_is_the_best_audio_track() {
        let formats = vec![
            video("137", 1080),
            audio("140", "m4a", 128_000),
            audio("251", "webm", 160_000),
        ];
        let pairs = vec![pair("137", Some("140"))];
        let chosen = choose(&pairs, &formats, &AudioSelector::Source).expect("the sound");
        assert_eq!(chosen.audio.as_deref(), Some("140"));
        assert_eq!(chosen.video, None);
        assert_eq!(chosen.container, "m4a");
    }

    #[test]
    fn a_video_without_a_sound_track_says_it_plainly() {
        let formats = vec![muxed("22", 720)];
        let pairs = vec![pair("22", None)];
        let error = choose(&pairs, &formats, &AudioSelector::Source).expect_err("no sound");
        assert!(error.to_string().contains("sound track"));
    }

    #[test]
    fn a_conversion_is_saved_in_the_container_it_names() {
        let formats = vec![video("137", 1080), audio("140", "m4a", 128_000)];
        let pairs = vec![pair("137", Some("140"))];
        let chosen = choose(
            &pairs,
            &formats,
            &AudioSelector::Mp3(Mp3Quality::Constant(320)),
        )
        .expect("an mp3");
        assert_eq!(chosen.container, "mp3");
        assert_eq!(chosen.audio.as_deref(), Some("140"));
        assert!(chosen.label.contains("320 kbps"));
    }

    #[test]
    fn the_flac_choice_says_the_source_may_be_lossy() {
        let formats = vec![video("137", 1080), audio("140", "m4a", 128_000)];
        let pairs = vec![pair("137", Some("140"))];
        let chosen = choose(&pairs, &formats, &AudioSelector::Flac).expect("a flac");
        assert_eq!(chosen.container, "flac");
        let caveat = chosen.caveat.expect("the caveat");
        assert!(caveat.contains("lossy"));
        assert!(caveat.contains("source"));
    }

    #[test]
    fn the_sounds_are_offered_best_first() {
        let formats = vec![video("137", 1080), audio("140", "m4a", 128_000)];
        let offered = preferences(&formats);
        let selectors: Vec<AudioSelector> = offered.iter().map(|row| row.selector).collect();
        assert_eq!(selectors[0], AudioSelector::Source);
        assert_eq!(selectors[1], AudioSelector::Mp3(Mp3Quality::Variable));
        assert_eq!(selectors[2], AudioSelector::Mp3(Mp3Quality::Constant(320)));
        assert_eq!(selectors[5], AudioSelector::Mp3(Mp3Quality::Constant(128)));
        assert_eq!(selectors[6], AudioSelector::Opus);
        assert_eq!(selectors[7], AudioSelector::Flac);
    }

    #[test]
    fn every_sound_says_the_container_it_is_saved_in_and_its_rate() {
        let formats = vec![video("137", 1080), audio("140", "m4a", 128_000)];
        let offered = preferences(&formats);
        assert_eq!(offered[0].container, "m4a");
        assert_eq!(offered[0].bitrate_kbps, Some(128));
        assert_eq!(offered[1].container, "mp3");
        assert_eq!(offered[1].bitrate_kbps, None, "a variable rate is no rate");
        assert_eq!(offered[2].container, "mp3");
        assert_eq!(offered[2].bitrate_kbps, Some(320));
        assert_eq!(offered[7].container, "flac");
        assert!(offered[7].caveat.is_some());
    }

    #[test]
    fn a_video_without_a_sound_track_offers_no_sounds() {
        assert!(preferences(&[video("137", 1080)]).is_empty());
    }

    #[test]
    fn a_video_without_a_sound_track_offers_no_conversion() {
        let offered = preferences(&[video("137", 1080)]);
        assert!(
            offered.iter().all(|row| !row.selector.is_conversion()),
            "a conversion needs a sound to convert"
        );
    }

    #[test]
    fn every_output_names_the_file_it_is_written_as() {
        assert_eq!(
            AudioSelector::Mp3(Mp3Quality::Constant(320)).extension(),
            "mp3"
        );
        assert_eq!(AudioSelector::Opus.extension(), "opus");
        assert_eq!(AudioSelector::Flac.extension(), "flac");
    }

    #[test]
    fn the_original_track_keeps_the_container_it_came_in() {
        assert_eq!(AudioSelector::Source.extension_for("m4a"), "m4a");
        assert_eq!(AudioSelector::Source.extension_for("webm"), "webm");
        assert_eq!(AudioSelector::Source.extension_for("mp4"), "mp4");
        assert_eq!(
            AudioSelector::Source.extension_for("something-else"),
            "m4a",
            "a container nobody knows is not a file name"
        );
    }

    #[test]
    fn the_original_track_is_not_converted_at_all() {
        assert_eq!(AudioSelector::Source.ffmpeg_codec(), None);
        assert_eq!(AudioSelector::Best.ffmpeg_codec(), None);
        assert_eq!(
            AudioSelector::Mp3(Mp3Quality::Variable).ffmpeg_codec(),
            Some("libmp3lame")
        );
        assert_eq!(AudioSelector::Opus.ffmpeg_codec(), Some("libopus"));
        assert_eq!(AudioSelector::Flac.ffmpeg_codec(), Some("flac"));
    }
}
