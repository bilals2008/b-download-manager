//! Enumerating a video's formats with yt-dlp.
//!
//! `yt-dlp --dump-single-json` returns the page's formats, each with its own
//! direct, freshly-signed stream address. The extension's old capture path
//! scraped whichever itag the player happened to request; this asks the
//! extractor for the complete list, so the panel can offer every quality and
//! the audio streams that have to be muxed with them.

use serde::Deserialize;
use tauri::AppHandle;

use crate::media::{run_sidecar, MediaFormat};

/// The slice of `--dump-single-json` this module reads. yt-dlp sends far more,
/// but the fields that are not declared are simply ignored.
#[derive(Deserialize)]
struct Dump {
    formats: Vec<RawFormat>,
}

#[derive(Deserialize)]
struct RawFormat {
    #[serde(default)]
    format_id: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    ext: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    fps: Option<f64>,
    #[serde(default)]
    vcodec: Option<String>,
    #[serde(default)]
    acodec: Option<String>,
    #[serde(default)]
    filesize: Option<u64>,
    #[serde(default)]
    filesize_approx: Option<u64>,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    has_drm: Option<bool>,
    #[serde(default)]
    format_note: Option<String>,
    #[serde(default)]
    resolution: Option<String>,
}

/// Enumerate the downloadable formats of a page.
///
/// Only direct HTTP streams are returned: an HLS or DASH manifest is not a
/// file the engine can save, and a DRM-protected stream has no usable address
/// at all. `proxy` is the user's proxy, passed through so the extractor follows
/// the same route as the downloads.
pub async fn extract(
    app: &AppHandle,
    proxy: Option<&str>,
    url: &str,
) -> Result<Vec<MediaFormat>, String> {
    let address = url.trim();
    if !address.starts_with("http://") && !address.starts_with("https://") {
        return Err("the video address must start with http:// or https://".to_string());
    }

    let mut args = vec![
        "--dump-single-json".to_string(),
        "--no-playlist".to_string(),
        "--no-warnings".to_string(),
        "--socket-timeout".to_string(),
        "20".to_string(),
    ];
    if let Some(proxy) = proxy.map(str::trim).filter(|value| !value.is_empty()) {
        args.push("--proxy".to_string());
        args.push(proxy.to_string());
    }
    args.push(address.to_string());

    let stdout = run_sidecar(app, "yt-dlp", &args).await?;
    let dump: Dump = serde_json::from_str(&stdout)
        .map_err(|error| format!("the extractor answered with unreadable data: {error}"))?;

    let formats: Vec<MediaFormat> = dump.formats.into_iter().filter_map(to_format).collect();
    if formats.is_empty() {
        return Err(
            "no downloadable format was found for this video; it may be protected".to_string(),
        );
    }
    Ok(formats)
}

/// Turn one raw format into a [`MediaFormat`], dropping anything the engine
/// cannot save.
fn to_format(raw: RawFormat) -> Option<MediaFormat> {
    // A protected stream has no address the application may use.
    if raw.has_drm == Some(true) {
        return None;
    }
    // Only a direct HTTP stream is a file. A live manifest ("m3u8", "mpd")
    // would save as a playlist, not as a playable video.
    match raw.protocol.as_deref() {
        Some("http") | Some("https") => {}
        _ => return None,
    }
    let url = raw.url?;

    let vcodec = raw.vcodec.filter(|codec| codec != "none");
    let acodec = raw.acodec.filter(|codec| codec != "none");
    let kind = match (&vcodec, &acodec) {
        (Some(_), None) => "video-only",
        (None, Some(_)) => "audio-only",
        _ => "muxed",
    };

    let resolution = raw
        .format_note
        .filter(|note| !note.is_empty())
        .or_else(|| raw.resolution.filter(|note| !note.is_empty()))
        .unwrap_or_else(|| match raw.height {
            Some(height) => format!("{height}p"),
            None => String::new(),
        });

    Some(MediaFormat {
        format_id: raw.format_id.unwrap_or_default(),
        resolution,
        height: raw.height,
        width: raw.width,
        fps: raw.fps,
        vcodec,
        acodec,
        ext: raw.ext.unwrap_or_else(|| "mp4".to_string()),
        filesize: raw.filesize.or(raw.filesize_approx),
        kind: kind.to_string(),
        url: Some(url),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(json: serde_json::Value) -> RawFormat {
        serde_json::from_value(json).expect("a raw format")
    }

    #[test]
    fn a_direct_video_stream_is_kept_and_kind_is_video_only() {
        let format = to_format(raw(serde_json::json!({
            "format_id": "137",
            "url": "https://googlevideo.example/videoplayback",
            "ext": "mp4",
            "width": 1920,
            "height": 1080,
            "fps": 30.0,
            "vcodec": "avc1.640028",
            "acodec": "none",
            "filesize": 123456,
            "protocol": "https",
            "has_drm": false,
            "format_note": "1080p"
        })))
        .expect("a downloadable stream");

        assert_eq!(format.format_id, "137");
        assert_eq!(format.resolution, "1080p");
        assert_eq!(format.kind, "video-only");
        assert_eq!(format.vcodec.as_deref(), Some("avc1.640028"));
        assert_eq!(format.acodec, None);
        assert_eq!(format.filesize, Some(123456));
    }

    #[test]
    fn an_audio_stream_has_no_picture() {
        let format = to_format(raw(serde_json::json!({
            "format_id": "140",
            "url": "https://googlevideo.example/videoplayback",
            "ext": "m4a",
            "vcodec": "none",
            "acodec": "mp4a.40.2",
            "protocol": "https"
        })))
        .expect("an audio stream");

        assert_eq!(format.kind, "audio-only");
        assert_eq!(format.vcodec, None);
        assert_eq!(format.acodec.as_deref(), Some("mp4a.40.2"));
        assert_eq!(format.resolution, "");
    }

    #[test]
    fn a_protected_stream_is_refused() {
        assert!(to_format(raw(serde_json::json!({
            "format_id": "1",
            "url": "https://example.org/video",
            "protocol": "https",
            "has_drm": true
        })))
        .is_none());
    }

    #[test]
    fn a_manifest_stream_is_refused() {
        assert!(to_format(raw(serde_json::json!({
            "format_id": "hls-1000",
            "url": "https://example.org/index.m3u8",
            "protocol": "m3u8_native"
        })))
        .is_none());
    }

    #[test]
    fn a_stream_without_an_address_is_refused() {
        assert!(to_format(raw(serde_json::json!({
            "format_id": "1",
            "protocol": "https"
        })))
        .is_none());
    }

    #[test]
    fn a_muxed_stream_has_both_tracks() {
        let format = to_format(raw(serde_json::json!({
            "format_id": "18",
            "url": "https://example.org/video.mp4",
            "ext": "mp4",
            "height": 360,
            "vcodec": "avc1.42001E",
            "acodec": "mp4a.40.2",
            "protocol": "https",
            "format_note": "360p"
        })))
        .expect("a muxed stream");

        assert_eq!(format.kind, "muxed");
        assert_eq!(format.vcodec.as_deref(), Some("avc1.42001E"));
        assert_eq!(format.acodec.as_deref(), Some("mp4a.40.2"));
    }

    #[test]
    fn a_resolution_without_a_note_is_derived_from_the_height() {
        let format = to_format(raw(serde_json::json!({
            "format_id": "137",
            "url": "https://example.org/video",
            "ext": "mp4",
            "height": 1080,
            "vcodec": "avc1",
            "acodec": "none",
            "protocol": "https"
        })))
        .expect("a stream");

        assert_eq!(format.resolution, "1080p");
    }
}
