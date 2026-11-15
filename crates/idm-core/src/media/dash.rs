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
    drm, quality, subtitles, Format, Manifest, MediaError, MediaKind, Pair, TrackSegment,
    MAX_DURATION, MAX_SEGMENTS,
};

/// A parsed manifest with the segment lists the download needs.
#[derive(Debug, Clone, PartialEq)]
pub struct DashManifest {
    pub duration: Option<f64>,
    pub tracks: Vec<DashTrack>,
    pub manifest: Manifest,
    /// The caption renditions the manifest offered, each on its own row.
    pub subtitles: Vec<subtitles::SubtitleRendition>,
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

/// Read an MPD into the visible manifest and the caption renditions it
/// offered.
pub fn parse_with_subtitles(
    text: &str,
    base_url: &str,
) -> Result<subtitles::SubtitleManifest, MediaError> {
    let parsed = parse_full(text, base_url)?;
    Ok(subtitles::SubtitleManifest {
        manifest: parsed.manifest,
        renditions: parsed.subtitles,
    })
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
    if let Some(scheme) = drm::in_dash_element(&root) {
        return Err(drm::error(scheme));
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
    // The caption renditions the manifest offered.
    let mut renditions: Vec<subtitles::SubtitleRendition> = Vec::new();
    // Segment information is inherited down the tree: a period or a set may
    // carry the `SegmentTemplate` every representation under it uses.
    let period_template = Template::from_element(period);
    for (set_index, set) in period.children_named("AdaptationSet").enumerate() {
        let role = set
            .child("Role")
            .and_then(|role| role.attr("value"))
            .unwrap_or("");
        if role.eq_ignore_ascii_case("trickmode") {
            continue;
        }
        let set_mime = set.attr("mimeType").unwrap_or("");
        let set_content = set.attr("contentType").unwrap_or("");
        // A caption set carries text, not a track to download on its own, so
        // it becomes a row of its own instead of a picture or a sound.
        if is_subtitle_set(set_content, set_mime) {
            renditions.extend(subtitle_renditions(set, &period_base, renditions.len())?);
            continue;
        }
        let set_kind = media_kind(set_content, set_mime, set.attr("codecs"));
        if role.eq_ignore_ascii_case("alternate") && set_kind != Some(MediaKind::Audio) {
            continue;
        }
        let set_base = resolve_base(set, &period_base)?;
        let set_template = Template::merge(
            period_template.as_ref(),
            Template::from_element(set).as_ref(),
        );

        for (rep_index, rep) in set.children_named("Representation").enumerate() {
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

    let mut manifest = build_manifest(&tracks, offered);
    subtitles::add_formats(&mut manifest, &renditions);
    manifest.subtitles = renditions.clone();
    Ok(DashManifest {
        duration: offered,
        tracks,
        manifest,
        subtitles: renditions,
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
        subtitles: Vec::new(),
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

/// Whether an adaptation set carries captions rather than a track.
fn is_subtitle_set(content: &str, mime: &str) -> bool {
    let mime = mime.to_ascii_lowercase();
    content.eq_ignore_ascii_case("text")
        || mime.starts_with("text/vtt")
        || mime.starts_with("application/ttml+xml")
}

/// The caption renditions of one adaptation set.
///
/// A representation without an id cannot be told apart from another one, so
/// it is left out rather than shown twice.
fn subtitle_renditions(
    set: &Element,
    period_base: &str,
    start: usize,
) -> Result<Vec<subtitles::SubtitleRendition>, MediaError> {
    let set_mime = set.attr("mimeType").unwrap_or("");
    let set_base = resolve_base(set, period_base)?;
    let mut out = Vec::new();
    for rep in set.children_named("Representation") {
        let Some(rep_id) = rep.attr("id").filter(|id| !id.is_empty()) else {
            continue;
        };
        let mime = rep.attr("mimeType").unwrap_or(set_mime);
        let url = match rep
            .child("BaseURL")
            .map(|base| base.text.trim())
            .filter(|url| !url.is_empty())
        {
            Some(base) => resolve(&set_base, base)?,
            None => String::new(),
        };
        out.push(subtitles::SubtitleRendition {
            id: format!("s{}", start + out.len()),
            name: rep_id.to_string(),
            language: rep.attr("lang").or(set.attr("lang")).map(str::to_string),
            container: subtitle_container(mime),
            url,
            is_default: set
                .attr("default")
                .is_some_and(|value| value.eq_ignore_ascii_case("true")),
        });
    }
    Ok(out)
}

/// The caption format a media type names.
fn subtitle_container(mime: &str) -> String {
    let mime = mime.to_ascii_lowercase();
    if mime.contains("ttml") {
        return "ttml".to_string();
    }
    if mime.contains("mp4") {
        return "mp4".to_string();
    }
    "vtt".to_string()
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

mod segments;

use segments::segment_plan;

#[cfg(test)]
mod tests;
