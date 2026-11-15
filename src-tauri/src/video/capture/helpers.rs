use idm_core::media::MediaError;
use reqwest::Url;

use crate::bridge::dest;

use super::{MediaJob, MediaKindHint};

#[derive(Default)]
pub(super) struct YoutubeParams {
    pub(super) itag: Option<u64>,
    pub(super) size: Option<u64>,
    pub(super) mime: Option<String>,
}

/// The itag, size and mime a player put in its own address. The page's text
/// is never used for a label: the itag table decides what the number means.
pub(super) fn youtube_params(url: &str) -> YoutubeParams {
    let Ok(url) = Url::parse(url) else {
        return YoutubeParams::default();
    };
    let mut params = YoutubeParams::default();
    for (name, value) in url.query_pairs() {
        match name.as_ref() {
            "itag" => params.itag = value.parse().ok(),
            "clen" => params.size = value.parse().ok(),
            "mime" => params.mime = Some(value.into_owned()),
            _ => {}
        }
    }
    params
}

/// What a manifest address appears to be, before it is fetched.
pub(crate) fn infer_kind(hint: MediaKindHint, url: &str) -> MediaKindHint {
    if hint != MediaKindHint::Auto {
        return hint;
    }
    let Ok(url) = Url::parse(url) else {
        return MediaKindHint::Auto;
    };
    let path = url.path().to_ascii_lowercase();
    if path.ends_with(".m3u8") || path.ends_with(".m3u") {
        return MediaKindHint::Hls;
    }
    if path.ends_with(".mpd") || path.ends_with(".ism") || path.ends_with(".ismc") {
        return MediaKindHint::Dash;
    }
    if url.query_pairs().any(|(name, _)| name == "itag")
        || path.ends_with("/videoplayback")
        || [
            ".mp4", ".m4v", ".webm", ".mkv", ".mov", ".ts", ".m4s", ".m4a", ".mp3", ".aac",
            ".flac", ".ogg", ".opus",
        ]
        .iter()
        .any(|extension| path.ends_with(extension))
    {
        return MediaKindHint::File;
    }
    MediaKindHint::Auto
}

pub(super) fn looks_like_mpd(text: &str) -> bool {
    text.trim_start().starts_with("<?xml") || text.contains("<MPD")
}

pub(super) fn title_of(job: &MediaJob, final_url: &str) -> String {
    let title = job.title.trim().trim_end_matches(" - YouTube").trim();
    if !title.is_empty() {
        return title.chars().take(200).collect();
    }
    if let Ok(url) = Url::parse(final_url) {
        if let Some(name) = dest::name_from_url(url.as_str()) {
            return name;
        }
        if let Some(host) = url.host_str() {
            return host.to_string();
        }
    }
    "Video".to_string()
}

pub(super) fn clean_title(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(" - YouTube")
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(200)
        .collect()
}

pub(super) fn file_container(url: &str, content_type: Option<&str>) -> String {
    if let Some(content_type) = content_type {
        let content_type = content_type.to_ascii_lowercase();
        if content_type.starts_with("audio/mp4") || content_type.starts_with("audio/aac") {
            return "m4a".to_string();
        }
        if content_type.starts_with("audio/mpeg") {
            return "mp3".to_string();
        }
        if content_type.starts_with("audio/webm") || content_type.starts_with("audio/ogg") {
            return "webm".to_string();
        }
        if content_type.starts_with("video/webm") {
            return "webm".to_string();
        }
        if content_type.starts_with("video/mp4") {
            return "mp4".to_string();
        }
    }
    let path = Url::parse(url)
        .map(|url| url.path().to_ascii_lowercase())
        .unwrap_or_else(|_| url.to_ascii_lowercase());
    for extension in [
        "mp4", "m4v", "webm", "mkv", "mov", "ts", "m4s", "m4a", "mp3", "aac", "flac", "ogg", "opus",
    ] {
        if path.ends_with(&format!(".{extension}")) {
            return match extension {
                "m4v" => "mp4".to_string(),
                "m4s" => "mp4".to_string(),
                other => other.to_string(),
            };
        }
    }
    "mp4".to_string()
}

/// The refusal a manifest address's content type names, if any.
///
/// YouTube's UMP is the player's own streaming protocol rather than a file, so
/// it is reported as a transport with no plain address: calling it DRM would
/// be a lie when the bytes were never inspected. An explicit DRM type is
/// reported as encryption, with the scheme named when the type names one.
pub(super) fn refused_content_type(content_type: &str) -> Option<MediaError> {
    let content_type = content_type.to_ascii_lowercase();
    if content_type.contains("vnd.yt-ump") {
        return Some(MediaError::PlayerTransport(
            "YouTube's UMP protocol".to_string(),
        ));
    }
    if content_type.contains("application/drm") {
        return Some(MediaError::protected(""));
    }
    if content_type.contains("vnd.widevine") {
        return Some(MediaError::protected("Widevine"));
    }
    if content_type.contains("vnd.ms-playready") {
        return Some(MediaError::protected("PlayReady"));
    }
    None
}

pub(super) fn is_audio(container: &str, content_type: Option<&str>) -> bool {
    if let Some(content_type) = content_type {
        if content_type.to_ascii_lowercase().starts_with("audio/") {
            return true;
        }
    }
    matches!(container, "m4a" | "mp3" | "aac" | "flac" | "opus" | "ogg")
}

pub(super) fn is_web_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

