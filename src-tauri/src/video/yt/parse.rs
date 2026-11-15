//! Reading a player answer: playability, the formats it offers, and the manifest and leases they become.

use super::*;
use super::resolver::rewrite_throttle;

/// Fold the clients' failures into the one a person reads.
pub(super) fn merge_failures(failures: Vec<YtError>) -> YtError {
    let mut failures = failures;
    if let Some(index) = failures
        .iter()
        .position(|error| matches!(error, YtError::NoPlainAddress))
    {
        return failures.remove(index);
    }
    match failures.len() {
        0 => YtError::NoStreams,
        1 => failures.remove(0),
        _ => {
            let detail = failures
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            YtError::Client {
                client: "every client".to_string(),
                reason: detail,
            }
        }
    }
}

/// Whether the answer is about this video and may be downloaded at all.
pub(super) fn check_playable(data: &Value, asked: &str) -> Result<(), YtError> {
    let status = data
        .pointer("/playabilityStatus/status")
        .and_then(Value::as_str)
        .unwrap_or("");
    let reason = || {
        data.pointer("/playabilityStatus/reason")
            .and_then(Value::as_str)
            .unwrap_or("the video is not playable")
            .to_string()
    };
    if status == "LIVE_STREAM_OFFLINE" || status == "REAL_TIME" {
        return Err(YtError::Live);
    }
    if !status.is_empty() && status != "OK" {
        let text = reason();
        if text.to_ascii_lowercase().contains("live") {
            return Err(YtError::Live);
        }
        return Err(YtError::Unplayable {
            status: status.to_string(),
            reason: text,
        });
    }
    let answered = data
        .pointer("/videoDetails/videoId")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !answered.is_empty() && answered != asked {
        return Err(YtError::Identity {
            asked: asked.to_string(),
            answered: answered.to_string(),
        });
    }
    if data.get("streamingData").is_none() {
        // A video behind DRM says so here, in the licence parameters.
        let protected = data.pointer("/streamingData/drmParams").is_some()
            || data.get("licenseInfos").is_some();
        let what = reason();
        return Err(if protected {
            YtError::Protected(what)
        } else {
            YtError::NoStreams
        });
    }
    if let Some(streaming) = data.get("streamingData") {
        if has_licence(streaming) {
            return Err(YtError::Protected(reason()));
        }
    }
    Ok(())
}

/// Whether a streaming answer names a licence, which means DRM.
fn has_licence(streaming: &Value) -> bool {
    streaming.get("drmParams").is_some()
        || streaming.get("licenseInfos").is_some()
        || streaming
            .get("adaptiveFormats")
            .and_then(Value::as_array)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item.get("drmFamilies").is_some() || item.get("licenseInfos").is_some()
                })
            })
}
/// The formats of one player answer, as the manifest and the plans the rest
/// of the media path downloads.
pub(super) fn plan_from(
    data: &Value,
    fallback_title: &str,
    profile: &PlayerProfile,
) -> Result<Planned, YtError> {
    let Some(streaming) = data.get("streamingData") else {
        return Err(YtError::NoStreams);
    };
    // The player's own version is the key a transform belongs to. Its script
    // path names it; when the answer does not, the client version stands in,
    // which is conservative: a new version is a new key.
    let identity = PlayerIdentity::new(
        profile.label,
        player_version(data).unwrap_or_else(|| profile.client_version.to_string()),
    );

    let mut raw: Vec<&Value> = Vec::new();
    for key in ["formats", "adaptiveFormats"] {
        if let Some(items) = streaming.get(key).and_then(Value::as_array) {
            raw.extend(items.iter());
        }
    }

    let mut formats = Vec::new();
    let mut tracks = Vec::new();
    let mut throttled = false;
    for (index, item) in raw.iter().enumerate() {
        let url = item.get("url").and_then(Value::as_str).unwrap_or("");
        if !url.starts_with("http") {
            continue;
        }
        let itag = item.get("itag").and_then(Value::as_u64).unwrap_or(0);
        let mime = item.get("mimeType").and_then(Value::as_str).unwrap_or("");
        let described = youtube::describe(itag);
        let kind = described
            .map(|entry| entry.kind)
            .unwrap_or_else(|| kind_of(mime, item));
        let container = described
            .map(|entry| entry.container.to_string())
            .unwrap_or_else(|| container_of(mime));
        let id = if itag > 0 {
            itag.to_string()
        } else {
            format!("format-{index}")
        };
        let mut format = Format::new(id.clone(), kind, container.clone());
        format.height = described
            .and_then(|entry| entry.height)
            .or_else(|| item.get("height").and_then(Value::as_u64).map(|h| h as u32));
        format.fps = described.and_then(|entry| entry.fps);
        format.bandwidth = item.get("bitrate").and_then(Value::as_u64);
        format.size = item
            .get("contentLength")
            .and_then(Value::as_str)
            .and_then(|size| size.parse().ok());
        format.codec = codec_of(mime);
        formats.push(format.clone());

        let needs_transform = challenge::throttled(url);
        // A learned program for this exact player turns the throttling
        // parameter into one the server answers at full speed. Without one the
        // address is kept verbatim: a plausible, wrong value is worse than a
        // slow download.
        let mut translated = false;
        let address = match rewrite_throttle(&identity, url) {
            Some(rewritten) if needs_transform => {
                translated = true;
                rewritten
            }
            _ => url.to_string(),
        };
        // A rewritten value still carries the parameter, so the presence of
        // `n` alone no longer says the address is untransformed.
        let still_throttled = needs_transform && !translated;
        throttled |= still_throttled;
        let query = Url::parse(&address).ok();
        let expires_at = query.as_ref().and_then(|query| {
            expiry_from_seconds(
                query
                    .query_pairs()
                    .find(|(name, _)| name == "expire")
                    .map(|(_, value)| value.to_string())
                    .as_deref(),
            )
        });
        let representation = RepresentationFingerprint {
            id,
            container,
            codec: format.codec.clone(),
            bytes: format.size,
        };
        let mut lease = RequestLease::new(
            address.clone(),
            Authority::Native,
            profile.label,
            Some(profile.client_version.to_string()),
            Some(profile.user_agent.to_string()),
            Credentials::at(origin_of(&address)),
            Transport::Progressive,
            representation,
            expires_at,
        );
        if still_throttled {
            lease = lease.with_note(Some(
                "the throttling parameter is present and untransformed".to_string(),
            ));
        } else if needs_transform {
            lease = lease.with_note(Some(format!(
                "the throttling parameter was rewritten for player {}",
                identity.player_version
            )));
        }
        tracks.push(FileTrack { format, lease });
    }
    if tracks.is_empty() {
        // The player only offered its own streaming protocol: there is no
        // file to fetch, and calling it DRM would be a lie.
        return Err(if streaming.get("serverAbrStreamingUrl").is_some() {
            YtError::NoPlainAddress
        } else {
            YtError::NoStreams
        });
    }

    let title = data
        .pointer("/videoDetails/title")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| fallback_title.to_string());
    let duration = data
        .pointer("/videoDetails/lengthSeconds")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<f64>().ok());

    let mut pairs = quality::pair_file_formats(&formats);
    quality::finish(&mut formats, &mut pairs, duration);
    let manifest = Manifest {
        title,
        duration,
        formats,
        pairs,
        container_hint: Some("youtube".to_string()),
        subtitles: Vec::new(),
    };
    Ok(Planned {
        manifest,
        source: SourcePlan::Files { tracks },
        throttled,
    })
}

/// The player script's own name, out of `playerConfig.assets.js`.
///
/// The path is `/s/player/<name>/player_ias.vflset/<language>/base.js`, and
/// `<name>` is the identity a transform belongs to. A value that is not a
/// plain path segment is refused rather than used as a cache key.
pub(super) fn player_version(data: &Value) -> Option<String> {
    let path = data
        .pointer("/playerConfig/assets/js")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    // `s`, `player`, then the name.
    while let Some(segment) = segments.next() {
        if segment != "s" {
            continue;
        }
        if segments.next() != Some("player") {
            continue;
        }
        let name = segments.next()?;
        if name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Some(name.to_string());
        }
        return None;
    }
    None
}

fn kind_of(mime: &str, item: &Value) -> MediaKind {
    if mime.starts_with("audio/") {
        return MediaKind::Audio;
    }
    if item.get("audioQuality").is_some() && item.get("height").is_none() {
        return MediaKind::Audio;
    }
    MediaKind::Video
}

fn container_of(mime: &str) -> String {
    let mime = mime.to_ascii_lowercase();
    if mime.contains("audio/mp4") || mime.contains("audio/m4a") || mime.contains("audio/aac") {
        "m4a".to_string()
    } else if mime.contains("webm") {
        "webm".to_string()
    } else {
        "mp4".to_string()
    }
}

/// The codec out of `video/mp4; codecs="avc1.640028"`.
pub(super) fn codec_of(mime: &str) -> Option<String> {
    let (_, rest) = mime.split_once("codecs=")?;
    let codec = rest.trim().trim_matches('"').split(',').next()?.trim();
    (!codec.is_empty() && codec.len() <= 64).then(|| codec.to_string())
}

pub(super) fn text_between<'a>(text: &'a str, marker: &str, end: char) -> Option<&'a str> {
    let at = text.find(marker)? + marker.len();
    let rest = &text[at..];
    let end_at = rest.find(end)?;
    Some(&rest[..end_at])
}

pub(super) fn digits_after(text: &str, marker: &str) -> Option<u64> {
    let at = text.find(marker)? + marker.len();
    let rest = &text[at..];
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .map(|end| &rest[..end])
        .unwrap_or(rest);
    digits.parse().ok()
}
