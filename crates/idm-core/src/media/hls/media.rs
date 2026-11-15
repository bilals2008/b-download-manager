//! Reading a media playlist: its segments, init segment and byte ranges.

use super::*;

/// Read a media playlist.
pub fn parse_media(text: &str, base_url: &str) -> Result<MediaPlaylist, MediaError> {
    require_header(text)?;
    // A protected playlist is refused before a single byte is fetched.
    if let Some(scheme) = drm::in_hls_playlist(text) {
        return Err(drm::error(scheme));
    }

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
    let mut discontinuities = 0usize;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            let attrs = Attributes::parse(rest);
            let Some(uri) = attrs.get("URI").filter(|uri| !uri.is_empty()) else {
                return Err(MediaError::Malformed(
                    "an init segment has no address".to_string(),
                ));
            };
            let range = match attrs.get("BYTERANGE") {
                Some(value) => {
                    let (length, offset) = parse_byte_range(value).ok_or_else(|| {
                        MediaError::Malformed(
                            "an init segment's byte range could not be read".to_string(),
                        )
                    })?;
                    // The format requires the offset here: the init segment
                    // is the first thing read from its resource, so there is
                    // no previous sub-range it could continue from.
                    let Some(offset) = offset else {
                        return Err(MediaError::Malformed(
                            "an init segment's byte range has no offset".to_string(),
                        ));
                    };
                    Some((offset, span(offset, length)?))
                }
                None => None,
            };
            let url = resolve(base_url, uri)?;
            let map = HlsSegment {
                url,
                range,
                duration: 0.0,
            };
            // A playlist may repeat its map; a *different* one partway
            // through describes bytes the join cannot place, because the
            // segments on each side are initialised differently.
            match &init {
                None => {
                    last_range_end = range.map(|(_, end)| (map.url.clone(), end + 1));
                    init = Some(map);
                }
                Some(first) if first == &map => {}
                Some(_) => {
                    return Err(MediaError::Unsupported(
                        "the playlist changes its init segment partway through, so the segments cannot be joined into one file".to_string(),
                    ));
                }
            }
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
        if line == "#EXT-X-DISCONTINUITY" {
            discontinuities += 1;
            continue;
        }
        if line == "#EXT-X-GAP" {
            // The tag marks a segment the publisher says is missing. Joining
            // around it would produce a file whose timeline claims time it
            // does not hold, so the stream is refused instead.
            return Err(MediaError::Unsupported(
                "the playlist has a gap (EXT-X-GAP) where a segment should be".to_string(),
            ));
        }
        if line.starts_with('#') {
            // `#EXT-X-PROGRAM-DATE-TIME` and the rest do not change which
            // bytes are downloaded.
            continue;
        }

        // A URI line belongs to the `#EXTINF` before it.
        let duration = pending_duration.take().unwrap_or(0.0);
        let url = resolve(base_url, line)?;
        let range = match pending_range.take() {
            Some((length, offset)) => {
                let offset = match offset {
                    Some(offset) => offset,
                    None => {
                        // An omitted offset continues after the previous
                        // sub-range — but only when that sub-range is of the
                        // same resource. Guessing zero would fetch the wrong
                        // bytes from the same address.
                        match last_range_end.as_ref() {
                            Some((last_url, end)) if last_url == &url => *end,
                            _ => return Err(MediaError::Malformed(
                                "a byte range has no offset and does not continue a previous one"
                                    .to_string(),
                            )),
                        }
                    }
                };
                Some((offset, span(offset, length)?))
            }
            None => None,
        };
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
    // A fragmented stream states its own decode times, so a discontinuity
    // resets the clock the join relies on. Plain segments are joined byte for
    // byte and carry their own timestamps, so a note is enough there.
    if discontinuities > 0 && init.is_some() {
        return Err(MediaError::Unsupported(format!(
            "the playlist restarts its timeline {discontinuities} time(s) (EXT-X-DISCONTINUITY), which a fragmented stream cannot be joined across"
        )));
    }

    Ok(MediaPlaylist {
        segments,
        init,
        duration: duration_total,
        ended,
        target_duration,
        version,
        discontinuities,
    })
}
