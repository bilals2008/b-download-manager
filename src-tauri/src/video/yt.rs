//! YouTube resolution through its own iOS InnerTube client.
//!
//! The web player streams through SABR/UMP over POST and no longer hands over a
//! plain file address; the legacy `get_video_info` endpoint is gone (`410`). The
//! iOS client's `player` call still returns a signed, direct `url` for every
//! format, so this module makes that call itself — no JavaScript player, no
//! external tool — and turns the formats into the same `Files` plan the rest of
//! the media path already downloads and muxes.
//!
//! The request needs the page's `visitorData` and `signatureTimestamp` (`STS`),
//! both read from the watch page. The formats it returns are ordinary GET
//! addresses; the app downloads them like any file.

use idm_core::media::{quality, youtube, Format, Manifest, MediaKind};
use reqwest::Url;
use serde_json::{json, Value};

use super::{FileTrack, SourcePlan};

/// The iOS client the player response is asked from.
const IOS_CLIENT_VERSION: &str = "20.11.6";
const IOS_UA: &str = "com.google.ios.youtube/20.11.6 (iPhone10,4; U; CPU iOS 16_7_7 like Mac OS X)";
/// The user agent the watch page is read with.
const DESKTOP_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
/// The signature timestamp used when the page does not state one.
const DEFAULT_STS: u64 = 20_759;

/// Whether a page address is a YouTube watch page this module can resolve.
pub fn is_youtube_page(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    let youtube = host == "youtu.be"
        || host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "m.youtube.com";
    youtube && video_id(url.as_str()).is_some()
}

/// The video id in a watch, short, or embed address.
pub fn video_id(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    let youtube = host == "youtu.be"
        || host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "m.youtube.com";
    if !youtube {
        return None;
    }
    if let Some((_, value)) = url.query_pairs().find(|(name, _)| name == "v") {
        let value = value.trim().to_string();
        if !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Some(value);
        }
    }
    let segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    if host == "youtu.be" {
        return segments.first().map(|s| s.to_string());
    }
    match segments.first().copied() {
        Some("shorts") | Some("embed") | Some("live") => segments.get(1).map(|s| s.to_string()),
        _ => None,
    }
}

/// Resolve a watch page into a manifest and its downloadable tracks.
pub async fn resolve(
    page_url: &str,
    fallback_title: &str,
) -> Result<(Manifest, SourcePlan), String> {
    let id =
        video_id(page_url).ok_or_else(|| "the page does not name a YouTube video".to_string())?;
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| format!("the video page could not be prepared: {error}"))?;

    // 1. Read the watch page for the visitor data and the signature timestamp.
    let page = client
        .get(format!("https://www.youtube.com/watch?v={id}&hl=en&gl=US"))
        .header("user-agent", DESKTOP_UA)
        .header("accept-language", "en-US,en;q=0.9")
        .header("cookie", "CONSENT=YES+1")
        .send()
        .await
        .map_err(|error| format!("the video page could not be read: {error}"))?;
    let html = page
        .text()
        .await
        .map_err(|error| format!("the video page could not be read: {error}"))?;
    let visitor = text_between(&html, "\"VISITOR_DATA\":\"", '"')
        .or_else(|| text_between(&html, "\"visitorData\":\"", '"'))
        .ok_or_else(|| "the video page did not name a visitor".to_string())?;
    let sts = digits_after(&html, "\"STS\":").unwrap_or(DEFAULT_STS);

    // 2. Ask the iOS client for the player response.
    let body = json!({
        "videoId": id,
        "racyCheckOk": true,
        "contentCheckOk": true,
        "playbackContext": {
            "contentPlaybackContext": {
                "vis": 0,
                "splay": false,
                "lactMilliseconds": "-1",
                "signatureTimestamp": sts
            }
        },
        "context": {
            "client": {
                "hl": "en",
                "gl": "US",
                "screenDensityFloat": 1,
                "screenHeightPoints": 1440,
                "screenPixelDensity": 1,
                "screenWidthPoints": 2560,
                "visitorData": visitor,
                "clientName": "iOS",
                "clientVersion": IOS_CLIENT_VERSION,
                "osName": "iOS",
                "osVersion": "16.7.7.20H330",
                "userAgent": IOS_UA,
                "platform": "MOBILE",
                "clientFormFactor": "UNKNOWN_FORM_FACTOR",
                "userInterfaceTheme": "USER_INTERFACE_THEME_LIGHT",
                "deviceMake": "Apple",
                "deviceModel": "iPhone10,4",
                "memoryTotalKbytes": "8000000"
            },
            "user": { "enableSafetyMode": false, "lockedSafetyMode": false },
            "request": { "useSsl": true, "internalExperimentFlags": [] }
        }
    });
    let response = client
        .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false&alt=json")
        .header("user-agent", IOS_UA)
        .header("accept", "*/*")
        .header("accept-language", "*")
        .header("origin", "https://www.youtube.com")
        .header("x-goog-visitor-id", visitor)
        .header("x-youtube-client-name", "5")
        .header("x-youtube-client-version", IOS_CLIENT_VERSION)
        .header("content-type", "application/json")
        .body(serde_json::to_vec(&body).map_err(|error| error.to_string())?)
        .send()
        .await
        .map_err(|error| format!("the video could not be asked for: {error}"))?;
    let text = response
        .text()
        .await
        .map_err(|error| format!("the video answer could not be read: {error}"))?;
    let data: Value = serde_json::from_str(&text)
        .map_err(|error| format!("the video answer could not be read: {error}"))?;

    let status = data
        .pointer("/playabilityStatus/status")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !status.is_empty() && status != "OK" {
        let reason = data
            .pointer("/playabilityStatus/reason")
            .and_then(Value::as_str)
            .unwrap_or("the video is not playable");
        return Err(reason.to_string());
    }

    let Some(streaming) = data.get("streamingData") else {
        return Err("the video returned no streams".to_string());
    };

    let mut raw: Vec<&Value> = Vec::new();
    for key in ["formats", "adaptiveFormats"] {
        if let Some(items) = streaming.get(key).and_then(Value::as_array) {
            raw.extend(items.iter());
        }
    }

    let mut formats = Vec::new();
    let mut tracks = Vec::new();
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
        let mut format = Format::new(id, kind, container);
        format.height = described
            .and_then(|entry| entry.height)
            .or_else(|| item.get("height").and_then(Value::as_u64).map(|h| h as u32));
        format.fps = described.and_then(|entry| entry.fps);
        format.bandwidth = item.get("bitrate").and_then(Value::as_u64);
        format.size = item
            .get("contentLength")
            .and_then(Value::as_str)
            .and_then(|size| size.parse().ok());
        formats.push(format.clone());
        tracks.push(FileTrack {
            format,
            url: url.to_string(),
        });
    }
    if tracks.is_empty() {
        return Err("the video offered no downloadable stream".to_string());
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
    };
    Ok((manifest, SourcePlan::Files { tracks }))
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

fn text_between<'a>(text: &'a str, marker: &str, end: char) -> Option<&'a str> {
    let at = text.find(marker)? + marker.len();
    let rest = &text[at..];
    let end_at = rest.find(end)?;
    Some(&rest[..end_at])
}

fn digits_after(text: &str, marker: &str) -> Option<u64> {
    let at = text.find(marker)? + marker.len();
    let rest = &text[at..];
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .map(|end| &rest[..end])
        .unwrap_or(rest);
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watch_page_names_its_video() {
        assert_eq!(
            video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            video_id("https://youtu.be/dQw4w9WgXcQ").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert!(video_id("https://example.org/watch?v=x").is_none());
    }

    #[test]
    fn a_non_youtube_page_is_not_resolved() {
        assert!(is_youtube_page(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(!is_youtube_page("https://example.org/watch?v=dQw4w9WgXcQ"));
        assert!(!is_youtube_page("https://www.youtube.com/"));
    }

    #[test]
    fn a_page_value_is_read_without_a_regex() {
        let html = r#"{"VISITOR_DATA":"Cgtabc","STS":20759}"#;
        assert_eq!(
            text_between(html, "\"VISITOR_DATA\":\"", '"'),
            Some("Cgtabc")
        );
        assert_eq!(digits_after(html, "\"STS\":"), Some(20759));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test]
    #[ignore = "reaches YouTube"]
    async fn a_live_page_resolves_to_direct_tracks() {
        let (manifest, source) = resolve("https://www.youtube.com/watch?v=dQw4w9WgXcQ", "Video")
            .await
            .expect("a resolution");
        assert!(!manifest.formats.is_empty(), "the page offered formats");
        match source {
            SourcePlan::Files { tracks } => {
                assert!(
                    tracks.iter().any(|t| t.format.kind == MediaKind::Video),
                    "a picture"
                );
                assert!(
                    tracks.iter().all(|t| t.url.starts_with("http")),
                    "direct urls"
                );
            }
            _ => panic!("files expected"),
        }
    }
}


#[cfg(test)]
mod live_download {
    use super::*;
    #[tokio::test]
    #[ignore = "reaches YouTube and downloads"]
    async fn a_live_video_track_downloads_whole() {
        let (manifest, source) = resolve("https://www.youtube.com/watch?v=dQw4w9WgXcQ", "Video")
            .await
            .expect("resolve");
        let track = match source {
            SourcePlan::Files { tracks } => tracks
                .into_iter()
                .filter(|t| t.format.kind == MediaKind::Video && t.format.size.unwrap_or(0) > 20_000_000)
                .min_by_key(|t| t.format.size.unwrap_or(u64::MAX))
                .or_else(|| None)
                .expect("a video track"),
            _ => panic!("files expected"),
        };
        eprintln!("itag {} size {:?}", track.format.id, track.format.size);
        let dir = std::env::temp_dir().join(format!("bdm-yt-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let dest = dir.join("video.track");
        let src = idm_core::media::download::TrackSource {
            init: None,
            segments: vec![idm_core::media::TrackSegment { url: track.url.clone(), range: None }],
            dest: dest.clone(),
            work_dir: dir.clone(),
            stem: "video".to_string(),
            connections: 8,
            proxy: None,
            referer: None,
            headers: Vec::new(),
            header_origin: None,
        };
        let start = std::time::Instant::now();
        let outcome = idm_core::media::download::download_track(
            idm_core::limits::Limits::default(),
            src,
            None,
            None,
        )
        .await
        .expect("download");
        let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        eprintln!("outcome {:?} bytes {} in {:?}", outcome, size, start.elapsed());
        assert!(size > 1_000_000, "a whole track");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = manifest;
    }
}
