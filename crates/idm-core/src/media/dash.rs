//! MPEG-DASH (`MPD`) parsing.
//!
//! The parser walks `MPD` -> `Period` -> `AdaptationSet` -> `Representation`
//! and expands the segment information into one list of addresses per track:
//! `SegmentTemplate` (`$Number$`, `$Time$`, `$RepresentationID$`,
//! `$Bandwidth$`, including `%0Nd` widths), `SegmentList`, and `SegmentBase`
//! (the whole file as one ranged download). A representation id is kept as
//! text, so YouTube's numeric itags pass through unchanged.
//!
//! Multi-period manifests offer their first period, and the period ends where
//! the next one starts, so no segment is asked for past the stretch it
//! describes. Live (`type="dynamic"`) and protected (`ContentProtection`)
//! manifests are refused with their own sentences: a live stream is not
//! encryption, and a protection system is named when the manifest names one.

use reqwest::Url;

use super::xml::{self, Element};
use super::{
    quality, Format, Manifest, MediaError, MediaKind, Pair, TrackSegment, MAX_DURATION,
    MAX_SEGMENTS,
};

/// A parsed manifest with the segment lists the download needs.
#[derive(Debug, Clone, PartialEq)]
pub struct DashManifest {
    pub duration: Option<f64>,
    pub tracks: Vec<DashTrack>,
    pub manifest: Manifest,
    /// A readable note for a manifest this build only partly offers, such as
    /// a multi-period stream.
    pub notice: Option<String>,
}

/// One representation, with the addresses behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct DashTrack {
    pub format: Format,
    pub init: Option<TrackSegment>,
    pub segments: Vec<TrackSegment>,
    pub language: Option<String>,
}

/// Read an MPD into the visible manifest.
pub fn parse(text: &str, base_url: &str) -> Result<Manifest, MediaError> {
    Ok(parse_full(text, base_url)?.manifest)
}

/// Read an MPD into the visible manifest and the track plans.
pub fn parse_full(text: &str, base_url: &str) -> Result<DashManifest, MediaError> {
    let root = xml::parse(text)?;
    if !root.name.eq_ignore_ascii_case("MPD") {
        return Err(MediaError::Malformed(
            "the document is not an MPD".to_string(),
        ));
    }
    if root
        .attr("type")
        .is_some_and(|kind| kind.eq_ignore_ascii_case("dynamic"))
    {
        return Err(MediaError::Live);
    }
    if root.contains("ContentProtection") {
        return Err(MediaError::protected(protection_scheme(&root)));
    }

    let duration = root
        .attr("mediaPresentationDuration")
        .and_then(parse_duration);
    if duration.is_some_and(|duration| duration > MAX_DURATION) {
        return Err(MediaError::TooLarge("longer than 12 hours".to_string()));
    }

    let periods: Vec<&Element> = root.children_named("Period").collect();
    if periods.is_empty() {
        return Err(MediaError::Malformed(
            "the manifest has no period".to_string(),
        ));
    }
    // Only the first period is offered. A second period is a different
    // stretch of the presentation, and duplicating every track is a V-next
    // feature; the panel says so instead of hiding it.
    let notice = (periods.len() > 1)
        .then(|| "This stream has more than one period; the first one is offered.".to_string());

    let mpd_base = resolve_base(&root, base_url)?;
    let period = periods[0];
    let period_base = resolve_base(period, &mpd_base)?;
    let period_length = first_period_length(period, periods.get(1).copied(), duration);
    // The list describes one period, so the length the panel shows is that
    // period's, not the whole presentation's: a ten-second presentation whose
    // first period is four seconds downloads four seconds.
    let offered = period_length.or(duration);

    let mut tracks: Vec<DashTrack> = Vec::new();
    // Reasons a representation could not be planned. They are only reported
    // when nothing else could be read, so one unusable row does not take the
    // rest of the manifest down with it.
    let mut skipped: Vec<MediaError> = Vec::new();
    // Segment information is inherited down the tree: a period or a set may
    // carry the `SegmentTemplate` every representation under it uses.
    let period_template = Template::from_element(period);
    for (set_index, set) in period.children_named("AdaptationSet").enumerate() {
        if set.contains("ContentProtection") {
            return Err(MediaError::protected(protection_scheme(set)));
        }
        let role = set
            .child("Role")
            .and_then(|role| role.attr("value"))
            .unwrap_or("");
        if role.eq_ignore_ascii_case("trickmode") {
            continue;
        }
        let set_mime = set.attr("mimeType").unwrap_or("");
        let set_content = set.attr("contentType").unwrap_or("");
        let set_kind = media_kind(set_content, set_mime, set.attr("codecs"));
        if role.eq_ignore_ascii_case("alternate") && set_kind != Some(MediaKind::Audio) {
            continue;
        }
        let set_base = resolve_base(set, &period_base)?;
        let set_template =
            Template::merge(period_template.as_ref(), Template::from_element(set).as_ref());

        for (rep_index, rep) in set.children_named("Representation").enumerate() {
            if rep.contains("ContentProtection") {
                return Err(MediaError::protected(protection_scheme(rep)));
            }
            let mime = rep.attr("mimeType").unwrap_or(set_mime);
            let content = rep.attr("contentType").unwrap_or(set_content);
            let Some(kind) = media_kind(content, mime, rep.attr("codecs").or(set.attr("codecs")))
            else {
                continue;
            };
            let rep_base = resolve_base(rep, &set_base)?;
            let template =
                Template::merge(set_template.as_ref(), Template::from_element(rep).as_ref());
            let plan = match segment_plan(
                rep,
                &template,
                &rep_base,
                period_length,
                rep.attr("id").unwrap_or(""),
            ) {
                Ok(plan) => plan,
                Err(reason) => {
                    skipped.push(reason);
                    continue;
                }
            };
            if plan.segments.len() > MAX_SEGMENTS {
                return Err(MediaError::TooLarge(format!(
                    "more than {MAX_SEGMENTS} segments"
                )));
            }
            let id = rep
                .attr("id")
                .map(str::to_string)
                .unwrap_or_else(|| format!("s{set_index}r{rep_index}"));
            let bandwidth = rep
                .attr("bandwidth")
                .and_then(|value| value.parse::<u64>().ok());
            let container = track_container(kind, mime, rep.attr("codecs").or(set.attr("codecs")));
            let mut format = Format::new(id, kind, container);
            format.codec = rep
                .attr("codecs")
                .or(set.attr("codecs"))
                .map(str::to_string);
            format.width = rep.attr("width").or(set.attr("width")).and_then(number_u32);
            format.height = rep
                .attr("height")
                .or(set.attr("height"))
                .and_then(number_u32);
            format.fps = rep
                .attr("frameRate")
                .or(set.attr("frameRate"))
                .and_then(number_f64);
            format.bandwidth = bandwidth;
            format.size = format.estimate_size(offered);
            tracks.push(DashTrack {
                format,
                init: plan.init,
                segments: plan.segments,
                language: rep.attr("lang").or(set.attr("lang")).map(str::to_string),
            });
        }
    }

    if tracks.is_empty() {
        return Err(skipped.into_iter().next().unwrap_or_else(|| {
            MediaError::Malformed("the manifest names no video or audio track".to_string())
        }));
    }

    let manifest = build_manifest(&tracks, offered);
    Ok(DashManifest {
        duration: offered,
        tracks,
        manifest,
        notice,
    })
}

fn build_manifest(tracks: &[DashTrack], duration: Option<f64>) -> Manifest {
    let videos: Vec<&DashTrack> = tracks
        .iter()
        .filter(|track| track.format.kind != MediaKind::Audio)
        .collect();
    let audios: Vec<&DashTrack> = tracks
        .iter()
        .filter(|track| track.format.kind == MediaKind::Audio)
        .collect();

    let mut formats: Vec<Format> = videos.iter().map(|track| track.format.clone()).collect();
    formats.extend(audios.iter().map(|track| track.format.clone()));

    let mut pairs = Vec::new();
    for video in &videos {
        let audio = best_audio(video.language.as_deref(), &audios);
        pairs.push(Pair {
            video: video.format.id.clone(),
            audio: audio.map(|track| track.format.id.clone()),
            label: String::new(),
        });
    }
    quality::finish(&mut formats, &mut pairs, duration);
    Manifest {
        title: String::new(),
        duration,
        formats,
        pairs,
        container_hint: Some("dash".to_string()),
    }
}

/// The audio track that goes with a video: the same language when one is
/// named, otherwise the highest bitrate the manifest states.
fn best_audio<'a>(language: Option<&str>, audios: &[&'a DashTrack]) -> Option<&'a DashTrack> {
    if let Some(language) = language {
        if let Some(track) = audios
            .iter()
            .copied()
            .find(|track| track.language.as_deref() == Some(language))
        {
            return Some(track);
        }
    }
    audios
        .iter()
        .copied()
        .max_by_key(|track| track.format.bandwidth.unwrap_or(0))
        .or_else(|| audios.first().copied())
}

fn media_kind(content: &str, mime: &str, codecs: Option<&str>) -> Option<MediaKind> {
    let content = content.to_ascii_lowercase();
    let mime = mime.to_ascii_lowercase();
    if content == "text"
        || content == "image"
        || mime.starts_with("text/")
        || mime.starts_with("image/")
    {
        return None;
    }
    if content == "video" || mime.starts_with("video/") {
        return Some(MediaKind::Video);
    }
    if content == "audio" || mime.starts_with("audio/") {
        return Some(MediaKind::Audio);
    }
    let codecs = codecs.unwrap_or("").to_ascii_lowercase();
    if codecs.contains("avc")
        || codecs.contains("hev")
        || codecs.contains("hvc")
        || codecs.contains("vp0")
        || codecs.contains("av01")
    {
        return Some(MediaKind::Video);
    }
    if codecs.contains("mp4a")
        || codecs.contains("opus")
        || codecs.contains("vorbis")
        || codecs.contains("ac-3")
        || codecs.contains("ec-3")
    {
        return Some(MediaKind::Audio);
    }
    None
}

fn track_container(kind: MediaKind, mime: &str, codecs: Option<&str>) -> String {
    let mime = mime.to_ascii_lowercase();
    if mime.contains("webm") {
        return "webm".to_string();
    }
    if mime.contains("mp4") {
        return if kind == MediaKind::Audio {
            "m4a".to_string()
        } else {
            "mp4".to_string()
        };
    }
    match kind {
        MediaKind::Audio => quality::audio_container(codecs),
        _ => quality::video_container(codecs),
    }
}

/// The protection system a manifest names, as the panel should read it.
///
/// `ContentProtection` identifies a system by `schemeIdUri`, usually in the
/// UUID form registered with DASH-IF. The well-known ones are named; anything
/// else is shown as the manifest wrote it, bounded so a hostile document
/// cannot push a paragraph into the window.
fn protection_scheme(element: &Element) -> String {
    let mut schemes = Vec::new();
    collect_schemes(element, &mut schemes);
    let named: Vec<String> = schemes.iter().map(|scheme| protection_name(scheme)).collect();
    for known in ["Widevine", "PlayReady", "FairPlay", "Clear Key"] {
        if named.iter().any(|name| name == known) {
            return known.to_string();
        }
    }
    named
        .into_iter()
        .find(|name| !name.is_empty())
        .unwrap_or_default()
}

fn collect_schemes(element: &Element, out: &mut Vec<String>) {
    if element.name.eq_ignore_ascii_case("ContentProtection") {
        let scheme = element
            .attr("schemeIdUri")
            .or_else(|| element.attr("value"))
            .unwrap_or("");
        out.push(scheme.trim().to_ascii_lowercase());
    }
    for child in &element.children {
        collect_schemes(child, out);
    }
}

/// The readable name of one `schemeIdUri`.
fn protection_name(scheme: &str) -> String {
    if scheme.contains("edef8ba9-79d6-4ace-a3c8-27dcd51d21ed") || scheme.contains("widevine") {
        return "Widevine".to_string();
    }
    if scheme.contains("9a04f079-9840-4286-ab92-e65be0885f95") || scheme.contains("playready") {
        return "PlayReady".to_string();
    }
    if scheme.contains("94ce86fb-07ff-4f43-adb8-93d2fa968ca2") || scheme.contains("fairplay") {
        return "FairPlay".to_string();
    }
    if scheme.contains("1077efec-c0b2-4d02-ace3-3c1e52e2fb4b") || scheme.contains("clearkey") {
        return "Clear Key".to_string();
    }
    if scheme.contains("mp4protection") {
        return "MPEG Common Encryption".to_string();
    }
    scheme.chars().take(64).collect()
}

/// How long the first period lasts, in seconds.
///
/// A period ends where its own `@duration` says it does, otherwise where the
/// next period starts, otherwise at the end of the presentation. Reading the
/// whole presentation's length for the first period of a multi-period stream
/// would ask for segments that belong to a different stretch of it.
fn first_period_length(
    period: &Element,
    next: Option<&Element>,
    presentation: Option<f64>,
) -> Option<f64> {
    let start = period.attr("start").and_then(parse_duration).unwrap_or(0.0);
    let end = period
        .attr("duration")
        .and_then(parse_duration)
        .map(|length| start + length)
        .or_else(|| {
            next.and_then(|next| next.attr("start"))
                .and_then(parse_duration)
        })
        .or(presentation);
    end.map(|end| (end - start).max(0.0))
}

/* --------------------------------------------------------------- segments */

struct Plan {
    init: Option<TrackSegment>,
    segments: Vec<TrackSegment>,
}

/// The segment information of one representation, after inheritance.
#[derive(Debug, Clone, Default)]
struct Template {
    media: Option<String>,
    initialization: Option<String>,
    initialization_range: Option<String>,
    start_number: Option<u64>,
    timescale: Option<u64>,
    duration: Option<u64>,
    presentation_time_offset: Option<u64>,
    timeline: Vec<TimelineEntry>,
}

#[derive(Debug, Clone)]
struct TimelineEntry {
    time: Option<u64>,
    duration: u64,
    repeat: i64,
}

impl Template {
    fn from_element(element: &Element) -> Option<Self> {
        let node = element.child("SegmentTemplate")?;
        let mut template = Template {
            media: node.attr("media").map(str::to_string),
            initialization: node.attr("initialization").map(str::to_string),
            initialization_range: node.attr("initializationRange").map(str::to_string),
            start_number: node.attr("startNumber").and_then(number),
            timescale: node.attr("timescale").and_then(number),
            duration: node.attr("duration").and_then(number),
            presentation_time_offset: node.attr("presentationTimeOffset").and_then(number),
            timeline: Vec::new(),
        };
        if let Some(timeline) = node.child("SegmentTimeline") {
            for entry in timeline.children_named("S") {
                // A missing `d` is kept as a zero so the plan can refuse it by
                // name instead of quietly dropping the entry.
                let duration = entry.attr("d").and_then(number).unwrap_or(0);
                template.timeline.push(TimelineEntry {
                    time: entry.attr("t").and_then(number),
                    duration,
                    repeat: entry
                        .attr("r")
                        .and_then(|value| value.parse::<i64>().ok())
                        .unwrap_or(0),
                });
            }
        }
        Some(template)
    }

    fn merge(base: Option<&Template>, over: Option<&Template>) -> Option<Template> {
        match (base, over) {
            (None, None) => None,
            (Some(base), None) => Some(base.clone()),
            (None, Some(over)) => Some(over.clone()),
            (Some(base), Some(over)) => Some(Template {
                media: over.media.clone().or_else(|| base.media.clone()),
                initialization: over
                    .initialization
                    .clone()
                    .or_else(|| base.initialization.clone()),
                initialization_range: over
                    .initialization_range
                    .clone()
                    .or_else(|| base.initialization_range.clone()),
                start_number: over.start_number.or(base.start_number),
                timescale: over.timescale.or(base.timescale),
                duration: over.duration.or(base.duration),
                presentation_time_offset: over
                    .presentation_time_offset
                    .or(base.presentation_time_offset),
                timeline: if over.timeline.is_empty() {
                    base.timeline.clone()
                } else {
                    over.timeline.clone()
                },
            }),
        }
    }
}

fn segment_plan(
    rep: &Element,
    template: &Option<Template>,
    base_url: &str,
    period_length: Option<f64>,
    representation_id: &str,
) -> Result<Plan, MediaError> {
    let bandwidth = rep
        .attr("bandwidth")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);

    // A template only describes the bytes when it names the media; one that
    // only carries timing falls through to the other segment information.
    let media = template.as_ref().and_then(|template| template.media.as_deref());
    if let (Some(template), Some(media)) = (template.as_ref(), media) {
        let init = template.initialization.as_deref().map(|uri| {
            let url = expand(
                uri,
                representation_id,
                bandwidth,
                template.start_number.unwrap_or(1),
                template.presentation_time_offset.unwrap_or(0),
            );
            TrackSegment {
                url: resolve(base_url, &url).unwrap_or(url),
                range: template
                    .initialization_range
                    .as_deref()
                    .and_then(parse_range),
            }
        });

        let start_number = template.start_number.unwrap_or(1);
        let timescale = template.timescale.unwrap_or(1).max(1);
        let pto = template.presentation_time_offset.unwrap_or(0);
        let mut segments = Vec::new();

        // A zero-length entry would repeat forever, and a repeat below -1 is
        // not a value the format allows; both are refusals rather than a
        // quietly shortened segment list.
        for entry in &template.timeline {
            if entry.duration == 0 {
                return Err(MediaError::Malformed(
                    "a timeline entry has no duration".to_string(),
                ));
            }
            if entry.repeat < -1 {
                return Err(MediaError::Malformed(
                    "a timeline repeat is outside the format's range".to_string(),
                ));
            }
        }

        if !template.timeline.is_empty() {
            let mut number = start_number;
            let mut time = 0u64;
            for entry in &template.timeline {
                time = entry.time.unwrap_or(time);
                let repeats = if entry.repeat < 0 {
                    // `r="-1"` repeats until the period ends, so the number
                    // of *additional* segments is the span divided by the
                    // duration, less the one this entry already describes.
                    // Without a period length the repetition is bounded by
                    // the segment cap, never by an open-ended loop.
                    match period_length {
                        Some(length) => {
                            let end = (length * timescale as f64) as u64 + pto;
                            end.saturating_sub(time)
                                .checked_div(entry.duration)
                                .unwrap_or(1)
                                .saturating_sub(1)
                        }
                        None => (MAX_SEGMENTS as u64).saturating_sub(1),
                    }
                } else {
                    entry.repeat as u64
                };
                for _ in 0..=repeats {
                    let url = expand(media, representation_id, bandwidth, number, time);
                    segments.push(TrackSegment {
                        url: resolve(base_url, &url)?,
                        range: None,
                    });
                    number += 1;
                    time = time.saturating_add(entry.duration);
                    if segments.len() > MAX_SEGMENTS {
                        return Err(MediaError::TooLarge(format!(
                            "more than {MAX_SEGMENTS} segments"
                        )));
                    }
                }
            }
        } else {
            let Some(duration) = template.duration.filter(|duration| *duration > 0) else {
                return Err(MediaError::Unsupported(
                    "the segment template names neither a timeline nor a segment duration"
                        .to_string(),
                ));
            };
            let Some(length) = period_length.filter(|length| *length > 0.0) else {
                return Err(MediaError::Unsupported(
                    "the manifest does not say how long the stream is".to_string(),
                ));
            };
            let ticks = length * timescale as f64;
            let count = (ticks / duration as f64).ceil() as u64;
            if count > MAX_SEGMENTS as u64 {
                return Err(MediaError::TooLarge(format!(
                    "more than {MAX_SEGMENTS} segments"
                )));
            }
            for index in 0..count {
                let time = pto.saturating_add(index.saturating_mul(duration));
                let url = expand(
                    media,
                    representation_id,
                    bandwidth,
                    start_number.saturating_add(index),
                    time,
                );
                segments.push(TrackSegment {
                    url: resolve(base_url, &url)?,
                    range: None,
                });
            }
        }

        if segments.is_empty() {
            return Err(MediaError::Unsupported(
                "the segment template expands to no segment".to_string(),
            ));
        }
        return Ok(Plan { init, segments });
    }

    if let Some(list) = rep.child("SegmentList") {
        let init = match list.child("Initialization") {
            Some(init) => Some(TrackSegment {
                url: match init.attr("sourceURL") {
                    Some(url) => resolve(base_url, url)?,
                    None => base_url.to_string(),
                },
                range: init.attr("range").and_then(parse_range),
            }),
            None => None,
        };
        let mut segments = Vec::new();
        for segment in list.children_named("SegmentURL") {
            let url = segment
                .attr("media")
                .map(|url| resolve(base_url, url))
                .transpose()?
                .unwrap_or_else(|| base_url.to_string());
            segments.push(TrackSegment {
                url,
                range: segment.attr("mediaRange").and_then(parse_range),
            });
            if segments.len() > MAX_SEGMENTS {
                return Err(MediaError::TooLarge(format!(
                    "more than {MAX_SEGMENTS} segments"
                )));
            }
        }
        if segments.is_empty() {
            return Err(MediaError::Unsupported(
                "the segment list names no segment".to_string(),
            ));
        }
        return Ok(Plan { init, segments });
    }

    if rep.child("SegmentBase").is_some() {
        // The representation is one file; the engine's range download fetches
        // it. A `sidx` index would only change how it is split, not the bytes.
        return Ok(Plan {
            init: None,
            segments: vec![TrackSegment {
                url: base_url.to_string(),
                range: None,
            }],
        });
    }

    // A representation with a BaseURL and no segment information is one
    // progressive file.
    if rep.child("BaseURL").is_some() {
        return Ok(Plan {
            init: None,
            segments: vec![TrackSegment {
                url: base_url.to_string(),
                range: None,
            }],
        });
    }

    Err(MediaError::Unsupported(
        "the manifest gives no address for the track's bytes".to_string(),
    ))
}

/// Replace the placeholders a template may carry.
fn expand(
    template: &str,
    representation_id: &str,
    bandwidth: u64,
    number: u64,
    time: u64,
) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            out.push(bytes[index] as char);
            index += 1;
            continue;
        }
        let Some(end) = template[index + 1..]
            .find('$')
            .map(|offset| index + 1 + offset)
        else {
            out.push('$');
            index += 1;
            continue;
        };
        let token = &template[index + 1..end];
        if token.is_empty() {
            out.push('$');
            index = end + 1;
            continue;
        }
        let (name, width) = split_format(token);
        let value = match name {
            "RepresentationID" => representation_id.to_string(),
            "Bandwidth" => bandwidth.to_string(),
            "Number" => number.to_string(),
            "Time" => time.to_string(),
            _ => {
                out.push('$');
                out.push_str(token);
                out.push('$');
                index = end + 1;
                continue;
            }
        };
        out.push_str(&format_number(&value, width));
        index = end + 1;
    }
    out
}

fn split_format(token: &str) -> (&str, Option<usize>) {
    match token.split_once('%') {
        Some((name, format)) => {
            let digits: String = format
                .trim_start_matches('0')
                .chars()
                .take_while(|character| character.is_ascii_digit())
                .collect();
            let width = if digits.is_empty() {
                format
                    .chars()
                    .take_while(|character| character.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .ok()
            } else {
                digits.parse().ok()
            };
            (name, width)
        }
        None => (token, None),
    }
}

fn format_number(value: &str, width: Option<usize>) -> String {
    match width {
        Some(width) if value.len() < width => {
            format!("{}{}", "0".repeat(width - value.len()), value)
        }
        _ => value.to_string(),
    }
}

fn resolve(base: &str, uri: &str) -> Result<String, MediaError> {
    let base = Url::parse(base)
        .map_err(|_| MediaError::Malformed("a base address is not usable".to_string()))?;
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

/// The base address of an element: its `BaseURL`, else the one above it.
fn resolve_base(element: &Element, parent: &str) -> Result<String, MediaError> {
    match element
        .child("BaseURL")
        .map(|base| base.text.trim())
        .filter(|base| !base.is_empty())
    {
        Some(base) => resolve(parent, base),
        None => Ok(parent.to_string()),
    }
}

fn parse_range(value: &str) -> Option<(u64, u64)> {
    let (start, end) = value.trim().split_once('-')?;
    let start = start.trim().parse().ok()?;
    let end = end.trim().parse().ok()?;
    (end >= start).then_some((start, end))
}

fn number(value: &str) -> Option<u64> {
    value.trim().parse().ok()
}

fn number_u32(value: &str) -> Option<u32> {
    value.trim().parse().ok()
}

fn number_f64(value: &str) -> Option<f64> {
    if let Some((numerator, denominator)) = value.split_once('/') {
        let numerator: f64 = numerator.trim().parse().ok()?;
        let denominator: f64 = denominator.trim().parse().ok()?;
        return (denominator > 0.0).then_some(numerator / denominator);
    }
    let value: f64 = value.trim().parse().ok()?;
    value.is_finite().then_some(value)
}

/// `PT1H2M3.5S` -> seconds. Years and months are refused: a month has no
/// honest number of seconds.
pub fn parse_duration(value: &str) -> Option<f64> {
    let value = value.trim();
    let rest = value.strip_prefix('P')?;
    if rest.contains('Y')
        || rest.contains('M')
            && !rest
                .split('T')
                .nth(1)
                .is_some_and(|time| time.contains('M'))
    {
        // A month or a year before the `T` is ambiguous; a minute after it is
        // not, and is handled below.
        if rest
            .split('T')
            .next()
            .is_some_and(|date| date.contains('Y') || date.contains('M'))
        {
            return None;
        }
    }
    let mut total = 0.0f64;
    let mut number = String::new();
    let mut in_time = false;
    for character in rest.chars() {
        match character {
            'T' => {
                in_time = true;
                number.clear();
            }
            '0'..='9' | '.' => number.push(character),
            'D' if !in_time => {
                total += number.parse::<f64>().ok()? * 86_400.0;
                number.clear();
            }
            'H' if in_time => {
                total += number.parse::<f64>().ok()? * 3_600.0;
                number.clear();
            }
            'M' if in_time => {
                total += number.parse::<f64>().ok()? * 60.0;
                number.clear();
            }
            'S' if in_time => {
                total += number.parse::<f64>().ok()?;
                number.clear();
            }
            _ => return None,
        }
    }
    if !number.is_empty() {
        return None;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::fixtures;

    const BASE: &str = "https://cdn.example.org/dash/manifest.mpd";

    fn ids(manifest: &DashManifest) -> Vec<&str> {
        manifest
            .tracks
            .iter()
            .map(|track| track.format.id.as_str())
            .collect()
    }

    #[test]
    fn the_fixture_manifest_is_read_with_both_tracks() {
        let parsed = parse_full(fixtures::DASH_MANIFEST, BASE).expect("an MPD");
        assert_eq!(parsed.duration, Some(10.0));
        assert_eq!(ids(&parsed), vec!["1080", "720", "251", "140"]);

        let video = &parsed.tracks[0];
        assert_eq!(video.format.kind, MediaKind::Video);
        assert_eq!(video.format.height, Some(1080));
        assert_eq!(
            video.format.fps,
            Some(30.0),
            "frameRate is inherited from the set"
        );
        assert_eq!(
            video.segments.len(),
            3,
            "ten seconds of four-second segments"
        );
        assert_eq!(
            video.segments[0].url,
            "https://cdn.example.org/dash/v-1080-1.m4s"
        );
        assert_eq!(
            video.init.as_ref().map(|init| init.url.as_str()),
            Some("https://cdn.example.org/dash/v-1080-init.mp4")
        );

        let manifest = &parsed.manifest;
        assert_eq!(manifest.formats.len(), 4);
        assert_eq!(manifest.pairs.len(), 2);
        assert_eq!(manifest.pairs[0].video, "1080");
        assert!(manifest.pairs[0].audio.is_some());
        assert_eq!(
            manifest.formats.last().map(|format| format.kind),
            Some(MediaKind::Audio)
        );
    }

    #[test]
    fn every_template_placeholder_is_expanded() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT8S">
          <Period>
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <SegmentTemplate timescale="1000" duration="4000" startNumber="7"
                media="v-$RepresentationID$-$Bandwidth$-$Number%05d$-$Time$.m4s"
                initialization="init-$RepresentationID$.mp4"/>
              <Representation id="137" bandwidth="4200000" width="1920" height="1080"/>
            </AdaptationSet>
          </Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a template");
        let track = &parsed.tracks[0];
        assert_eq!(
            track.segments[0].url,
            "https://cdn.example.org/dash/v-137-4200000-00007-0.m4s"
        );
        assert_eq!(
            track.segments[1].url,
            "https://cdn.example.org/dash/v-137-4200000-00008-4000.m4s"
        );
        assert_eq!(track.segments.len(), 2);
    }

    #[test]
    fn a_segment_timeline_is_expanded() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT6S">
          <Period>
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <SegmentTemplate timescale="1000" media="s-$Time$-$Number$.m4s" startNumber="1">
                <SegmentTimeline><S t="0" d="2000" r="1"/><S d="2000"/></SegmentTimeline>
              </SegmentTemplate>
              <Representation id="v" bandwidth="1000" width="640" height="360"/>
            </AdaptationSet>
          </Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a timeline");
        let urls: Vec<&str> = parsed.tracks[0]
            .segments
            .iter()
            .map(|segment| segment.url.as_str())
            .collect();
        assert_eq!(urls.len(), 3);
        assert!(urls[0].ends_with("s-0-1.m4s"));
        assert!(urls[1].ends_with("s-2000-2.m4s"));
        assert!(urls[2].ends_with("s-4000-3.m4s"));
    }

    #[test]
    fn a_template_that_would_expand_past_the_cap_is_refused() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT6000S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <SegmentTemplate timescale="1" duration="1" media="s-$Number$.m4s"/>
            <Representation id="v" bandwidth="1000" width="640" height="360"/>
          </AdaptationSet></Period>
        </MPD>"#;
        assert!(matches!(
            parse_full(text, BASE),
            Err(MediaError::TooLarge(_))
        ));
    }

    #[test]
    fn a_segment_list_is_read() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period><AdaptationSet contentType="audio" mimeType="audio/mp4">
            <Representation id="a" bandwidth="128000">
              <SegmentList>
                <Initialization sourceURL="init.mp4" range="0-99"/>
                <SegmentURL media="a1.m4s"/>
                <SegmentURL media="a2.m4s"/>
              </SegmentList>
            </Representation>
          </AdaptationSet></Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a list");
        let track = &parsed.tracks[0];
        assert_eq!(
            track.init.as_ref().map(|init| init.range),
            Some(Some((0, 99)))
        );
        assert_eq!(track.segments.len(), 2);
        assert!(track.segments[0].url.ends_with("a1.m4s"));
    }

    #[test]
    fn a_segment_base_is_one_file() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <Representation id="137" bandwidth="1000" width="640" height="360">
              <BaseURL>whole.mp4</BaseURL>
              <SegmentBase indexRange="100-200"/>
            </Representation>
          </AdaptationSet></Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a base");
        let track = &parsed.tracks[0];
        assert_eq!(track.segments.len(), 1);
        assert_eq!(
            track.segments[0].url,
            "https://cdn.example.org/dash/whole.mp4"
        );
    }

    #[test]
    fn youtube_itag_ids_survive() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT2S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <SegmentTemplate media="v.m4s" initialization="i.mp4" timescale="1000" duration="1000"/>
            <Representation id="299" bandwidth="9000" width="1920" height="1080"/>
          </AdaptationSet></Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("an itag id");
        assert_eq!(parsed.tracks[0].format.id, "299");
    }

    #[test]
    fn a_protected_manifest_is_refused() {
        let text = r#"<MPD type="static"><Period><AdaptationSet contentType="video">
          <ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011"/>
          <Representation id="1"><BaseURL>a.mp4</BaseURL></Representation>
        </AdaptationSet></Period></MPD>"#;
        let error = parse_full(text, BASE).expect_err("protected");
        assert_eq!(
            error,
            MediaError::protected("MPEG Common Encryption"),
            "the protection system is named"
        );
        assert!(error.to_string().contains("protected"));
    }

    #[test]
    fn a_protection_system_is_named_instead_of_one_blanket_sentence() {
        let text = r#"<MPD type="static"><Period><AdaptationSet contentType="video">
          <ContentProtection
            schemeIdUri="urn:uuid:edef8ba9-79d6-4ace-a3c8-27dcd51d21ed"/>
          <Representation id="1"><BaseURL>a.mp4</BaseURL></Representation>
        </AdaptationSet></Period></MPD>"#;
        let error = parse_full(text, BASE).expect_err("widevine");
        assert!(error.to_string().contains("Widevine"), "{error}");

        // A `ContentProtection` on the representation is read the same way.
        let per_rep = r#"<MPD type="static"><Period><AdaptationSet contentType="video">
          <Representation id="1"><BaseURL>a.mp4</BaseURL>
            <ContentProtection schemeIdUri="urn:uuid:9a04f079-9840-4286-ab92-e65be0885f95"/>
          </Representation>
        </AdaptationSet></Period></MPD>"#;
        let error = parse_full(per_rep, BASE).expect_err("playready");
        assert!(error.to_string().contains("PlayReady"), "{error}");
    }

    #[test]
    fn a_dynamic_manifest_is_live() {
        let text = r#"<MPD type="dynamic"><Period><AdaptationSet contentType="video">
          <Representation id="1"><BaseURL>a.mp4</BaseURL></Representation>
        </AdaptationSet></Period></MPD>"#;
        let error = parse_full(text, BASE).expect_err("live");
        assert_eq!(error, MediaError::Live);
        assert!(error.to_string().contains("live"));
        assert!(!error.to_string().contains("protected"));
    }

    #[test]
    fn segment_information_is_inherited_from_the_period() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period>
            <SegmentTemplate timescale="1000" duration="2000" media="v-$Number$.m4s"/>
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <Representation id="137" bandwidth="1000" width="640" height="360"/>
            </AdaptationSet>
          </Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a period-level template");
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].segments.len(), 2);
        assert!(parsed.tracks[0].segments[0].url.ends_with("v-1.m4s"));
    }

    #[test]
    fn the_first_period_stops_where_the_next_one_starts() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT10S">
          <Period>
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <SegmentTemplate timescale="1000" duration="2000" media="v-$Number$.m4s"/>
              <Representation id="1" bandwidth="1000" width="640" height="360"/>
            </AdaptationSet>
          </Period>
          <Period start="PT4S">
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <Representation id="2"><BaseURL>second.mp4</BaseURL></Representation>
            </AdaptationSet>
          </Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("the first period");
        assert_eq!(
            parsed.tracks[0].segments.len(),
            2,
            "four seconds of two-second segments, not the whole ten-second presentation"
        );
        assert_eq!(
            parsed.duration,
            Some(4.0),
            "what the panel promises is what the first period holds"
        );
    }

    #[test]
    fn a_repeat_that_runs_to_the_period_end_stops_there() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT6S">
          <Period>
            <AdaptationSet contentType="video" mimeType="video/mp4">
              <SegmentTemplate timescale="1000" media="s-$Time$.m4s">
                <SegmentTimeline><S t="0" d="2000" r="-1"/></SegmentTimeline>
              </SegmentTemplate>
              <Representation id="1" bandwidth="1000" width="640" height="360"/>
            </AdaptationSet>
          </Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("a repeat");
        assert_eq!(
            parsed.tracks[0].segments.len(),
            3,
            "six seconds of two-second segments"
        );
        assert!(parsed.tracks[0].segments[2].url.ends_with("s-4000.m4s"));
    }

    #[test]
    fn a_presentation_time_offset_shifts_the_first_segment_time() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <SegmentTemplate timescale="1000" duration="2000" presentationTimeOffset="9000"
              media="s-$Number$-$Time$.m4s"/>
            <Representation id="1" bandwidth="1000" width="640" height="360"/>
          </AdaptationSet></Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("an offset");
        let urls: Vec<&str> = parsed.tracks[0]
            .segments
            .iter()
            .map(|segment| segment.url.as_str())
            .collect();
        assert_eq!(urls.len(), 2);
        assert!(urls[0].ends_with("s-1-9000.m4s"), "{}", urls[0]);
        assert!(urls[1].ends_with("s-2-11000.m4s"), "{}", urls[1]);
    }

    #[test]
    fn a_timeline_entry_without_a_duration_is_refused() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT6S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <SegmentTemplate timescale="1000" media="s-$Time$.m4s">
              <SegmentTimeline><S t="0" d="0"/></SegmentTimeline>
            </SegmentTemplate>
            <Representation id="1" bandwidth="1000" width="640" height="360"/>
          </AdaptationSet></Period>
        </MPD>"#;
        let error = parse_full(text, BASE).expect_err("a zero duration");
        assert!(matches!(error, MediaError::Malformed(_)), "{error:?}");
        assert!(error.to_string().contains("duration"), "{error}");
    }

    #[test]
    fn a_track_with_no_address_says_that_instead_of_disappearing() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <Representation id="1" bandwidth="1000" width="640" height="360"/>
          </AdaptationSet></Period>
        </MPD>"#;
        let error = parse_full(text, BASE).expect_err("no address");
        assert!(matches!(error, MediaError::Unsupported(_)), "{error:?}");
        assert!(error.to_string().contains("no address"), "{error}");
    }

    #[test]
    fn a_second_period_is_noted_and_the_first_is_offered() {
        let text = r#"<MPD type="static" mediaPresentationDuration="PT4S">
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <Representation id="1" bandwidth="1000" width="640" height="360">
              <BaseURL>first.mp4</BaseURL><SegmentBase/>
            </Representation>
          </AdaptationSet></Period>
          <Period><AdaptationSet contentType="video" mimeType="video/mp4">
            <Representation id="2"><BaseURL>second.mp4</BaseURL><SegmentBase/></Representation>
          </AdaptationSet></Period>
        </MPD>"#;
        let parsed = parse_full(text, BASE).expect("the first period");
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].format.id, "1");
        assert!(parsed
            .notice
            .as_deref()
            .is_some_and(|notice| notice.contains("more than one period")));
    }

    #[test]
    fn bad_xml_is_refused() {
        assert!(matches!(
            parse_full(fixtures::DASH_BAD, BASE),
            Err(MediaError::Malformed(_))
        ));
    }

    #[test]
    fn a_duration_is_read_in_seconds() {
        assert_eq!(parse_duration("PT10S"), Some(10.0));
        assert_eq!(parse_duration("PT1H2M3.5S"), Some(3723.5));
        assert_eq!(parse_duration("P1DT2H"), Some(93_600.0));
        assert_eq!(parse_duration("P1M"), None, "a month has no honest seconds");
        assert_eq!(parse_duration("nonsense"), None);
    }

    #[tokio::test]
    async fn the_fixture_tracks_download_and_concatenate() {
        use crate::limits::Limits;
        use crate::media::download::{self, TrackOutcome, TrackSource};
        use crate::media::fixtures;
        use crate::media::testserver::TestServer;

        let video_init = fixtures::segment("VINIT");
        let video_1 = fixtures::segment("V1");
        let video_2 = fixtures::segment("V2");
        let video_3 = fixtures::segment("V3");
        let audio_init = fixtures::segment("AINIT");
        let audio_1 = fixtures::segment("A1");
        let audio_2 = fixtures::segment("A2");
        let audio_3 = fixtures::segment("A3");

        let server = TestServer::start_with_files(vec![
            ("/dash/v-1080-init.mp4".to_string(), video_init.clone()),
            ("/dash/v-1080-1.m4s".to_string(), video_1.clone()),
            ("/dash/v-1080-2.m4s".to_string(), video_2.clone()),
            ("/dash/v-1080-3.m4s".to_string(), video_3.clone()),
            ("/dash/a-251-init.mp4".to_string(), audio_init.clone()),
            ("/dash/a-251-1.m4s".to_string(), audio_1.clone()),
            ("/dash/a-251-2.m4s".to_string(), audio_2.clone()),
            ("/dash/a-251-3.m4s".to_string(), audio_3.clone()),
        ]);
        let parsed = parse_full(fixtures::DASH_MANIFEST, &server.url("/dash/manifest.mpd"))
            .expect("the fixture MPD");

        let dir = std::env::temp_dir().join(format!("idm-dash-download-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");

        let video = parsed
            .tracks
            .iter()
            .find(|track| track.format.id == "1080")
            .expect("the 1080 track");
        let outcome = download::download_track(
            Limits::default(),
            TrackSource {
                init: video.init.clone(),
                segments: video.segments.clone(),
                dest: dir.join("video.mp4"),
                work_dir: dir.clone(),
                stem: "video".to_string(),
                connections: 2,
                proxy: None,
                referer: None,
                headers: Vec::new(),
                header_origin: None,
                fingerprint: None,
            },
            None,
            None,
        )
        .await
        .expect("the video track");
        assert_eq!(outcome, TrackOutcome::Done);

        let mut expected = video_init;
        expected.extend(video_1);
        expected.extend(video_2);
        expected.extend(video_3);
        assert_eq!(
            std::fs::read(dir.join("video.mp4")).expect("the video"),
            expected
        );

        let audio = parsed
            .tracks
            .iter()
            .find(|track| track.format.id == "251")
            .expect("the audio track");
        let outcome = download::download_track(
            Limits::default(),
            TrackSource {
                init: audio.init.clone(),
                segments: audio.segments.clone(),
                dest: dir.join("audio.m4a"),
                work_dir: dir.clone(),
                stem: "audio".to_string(),
                connections: 2,
                proxy: None,
                referer: None,
                headers: Vec::new(),
                header_origin: None,
                fingerprint: None,
            },
            None,
            None,
        )
        .await
        .expect("the audio track");
        assert_eq!(outcome, TrackOutcome::Done);

        let mut expected = audio_init;
        expected.extend(audio_1);
        expected.extend(audio_2);
        expected.extend(audio_3);
        assert_eq!(
            std::fs::read(dir.join("audio.m4a")).expect("the audio"),
            expected
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_frame_rate_is_read_as_a_ratio() {
        assert_eq!(number_f64("30"), Some(30.0));
        assert_eq!(number_f64("30000/1001"), Some(29.97002997002997));
        assert_eq!(number_f64("x"), None);
    }
}
