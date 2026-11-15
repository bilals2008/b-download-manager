//! Expanding the segment information of a representation into addresses.

use super::*;

pub(super) fn segment_plan(
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
    let media = template
        .as_ref()
        .and_then(|template| template.media.as_deref());
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

fn parse_range(value: &str) -> Option<(u64, u64)> {
    let (start, end) = value.trim().split_once('-')?;
    let start = start.trim().parse().ok()?;
    let end = end.trim().parse().ok()?;
    (end >= start).then_some((start, end))
}
