//! Resolving a page through the optional `yt-dlp` helper.
//!
//! The helper is asked for the page's metadata (`-J`) and nothing else: it
//! names every format the site offers and hands back direct file addresses.
//! The bytes are fetched by this app's own engine, so multi-connection speed,
//! resume and the native muxer all still apply; the helper only answers "what
//! is on this page and where".
//!
//! When the helper is missing, or it fails, the in-house resolver
//! ([`super::yt`]) is asked instead. That keeps a fresh install able to read a
//! YouTube page, and makes the helper an upgrade rather than a requirement.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use tauri::AppHandle;

use idm_core::media::{quality, Format, Manifest, MediaKind};
use reqwest::Url;
use serde::Deserialize;

use super::lease::{
    self, Authority, Credentials, RepresentationFingerprint, RequestLease, Transport,
};
use super::{FileTrack, SourcePlan};

/// How long the helper may spend reading one page.
const TIMEOUT: Duration = Duration::from_secs(90);

/// Whether a helper is installed and may be run.
pub fn available(app: &AppHandle) -> bool {
    super::tools::installed(app).is_some()
}

/// Resolve a page for the first time; the cached in-house answer is used only
/// when the helper is not there.
pub async fn resolve_page(
    app: &AppHandle,
    page_url: &str,
    fallback_title: &str,
) -> Result<(Manifest, SourcePlan), String> {
    if available(app) {
        match resolve(app, page_url, fallback_title).await {
            Ok(answer) => return Ok(answer),
            Err(error) => {
                idm_core::log::write(
                    "warn",
                    &format!("the video helper could not read the page: {error}"),
                );
            }
        }
    }
    super::yt::resolve(page_url, fallback_title)
        .await
        .map_err(|error| error.to_string())
}

/// Resolve a page again because the previous addresses were refused. A helper
/// always asks the page afresh; the in-house resolver is asked to skip its
/// cache, since a cached answer is exactly what stopped working.
pub async fn renew_page(
    app: &AppHandle,
    page_url: &str,
    fallback_title: &str,
) -> Result<(Manifest, SourcePlan), String> {
    if available(app) {
        match resolve(app, page_url, fallback_title).await {
            Ok(answer) => return Ok(answer),
            Err(error) => {
                idm_core::log::write(
                    "warn",
                    &format!("the video helper could not read the page again: {error}"),
                );
            }
        }
    }
    super::yt::renew(page_url, fallback_title)
        .await
        .map_err(|error| error.to_string())
}

/// Ask the helper for one page.
pub async fn resolve(
    app: &AppHandle,
    page_url: &str,
    fallback_title: &str,
) -> Result<(Manifest, SourcePlan), String> {
    let helper = super::tools::installed(app)
        .ok_or_else(|| "the video helper is not installed".to_string())?;
    let answer = ask(app, &helper, page_url).await?;
    parse(&answer, fallback_title)
}

async fn ask(app: &AppHandle, helper: &Path, page_url: &str) -> Result<String, String> {
    let mut command = tokio::process::Command::new(helper);
    command
        .arg("-J")
        .arg("--no-playlist")
        .arg("--no-warnings")
        .arg("--ignore-config")
        .arg("--no-progress");
    // The app's own downloads already go through this proxy; the helper must
    // reach the site the same way.
    if let Some(proxy) = super::tools::proxy(app) {
        command.arg("--proxy").arg(proxy);
    }
    command
        .arg(page_url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // A console window must not flash over the app for a page that resolves in
    // under a second.
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let output = tokio::time::timeout(TIMEOUT, command.output())
        .await
        .map_err(|_| "the video helper took too long to read the page".to_string())?
        .map_err(|error| format!("the video helper could not be started: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .map(str::trim)
            .unwrap_or("");
        return Err(if reason.is_empty() {
            format!("the video helper answered {}", output.status)
        } else {
            format!("the video helper failed: {reason}")
        });
    }
    String::from_utf8(output.stdout)
        .map_err(|_| "the video helper's answer was not text".to_string())
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    formats: Vec<RawFormat>,
}

#[derive(Deserialize)]
struct RawFormat {
    #[serde(default)]
    format_id: String,
    #[serde(default)]
    ext: Option<String>,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    vcodec: Option<String>,
    #[serde(default)]
    acodec: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    fps: Option<f64>,
    /// Kilobits per second, as the helper prints it.
    #[serde(default)]
    tbr: Option<f64>,
    #[serde(default)]
    filesize: Option<u64>,
    #[serde(default)]
    filesize_approx: Option<u64>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    has_drm: bool,
    #[serde(default)]
    http_headers: HashMap<String, String>,
}

/// One format of the answer, before the audio duplicates are dropped.
struct Candidate {
    format: Format,
    url: String,
    user_agent: Option<String>,
    expires_at: Option<std::time::SystemTime>,
    /// The exact size only; an approximation must not enter the resume guard,
    /// because two reads of the same format may differ by a few bytes.
    bytes: Option<u64>,
}

/// Turn one helper answer into the manifest and the leases behind it.
fn parse(answer: &str, fallback_title: &str) -> Result<(Manifest, SourcePlan), String> {
    let answer: Answer = serde_json::from_str(answer)
        .map_err(|error| format!("the video helper's answer could not be read: {error}"))?;

    let mut candidates: Vec<Candidate> = Vec::new();
    for raw in &answer.formats {
        if raw.has_drm || raw.format_id.ends_with("-drc") {
            continue;
        }
        // Only a plain `https` address is a file; `m3u8_native` and `mhtml`
        // formats are a playlist and a storyboard, which this path does not
        // download.
        if raw.protocol.as_deref() != Some("https") {
            continue;
        }
        let Some(url) = raw.url.as_deref().filter(|url| url.starts_with("https://")) else {
            continue;
        };
        let container = match raw.ext.as_deref() {
            Some("mp4") => "mp4",
            Some("webm") => "webm",
            Some("m4a") => "m4a",
            _ => continue,
        };
        let video_codec = codec(raw.vcodec.as_deref());
        let audio_codec = codec(raw.acodec.as_deref());
        let kind = match (video_codec.as_deref(), audio_codec.as_deref(), raw.height) {
            (Some(_), Some(_), _) => MediaKind::Muxed,
            (Some(_), None, _) | (None, None, Some(_)) => MediaKind::Video,
            (None, Some(_), _) => MediaKind::Audio,
            // Neither codec nor a picture: an unknown stream, not a file this
            // path knows how to use.
            (None, None, None) => continue,
        };

        let codec = match kind {
            MediaKind::Audio => audio_codec,
            _ => video_codec,
        };
        let mut format = Format::new(raw.format_id.clone(), kind, container);
        format.codec = codec;
        format.width = raw.width;
        format.height = raw.height;
        format.fps = raw.fps;
        format.bandwidth = raw
            .tbr
            .filter(|rate| *rate > 0.0)
            .map(|rate| (rate * 1000.0).round() as u64);
        format.size = raw.filesize.or(raw.filesize_approx);

        let expires_at = Url::parse(url).ok().and_then(|parsed| {
            lease::expiry_from_seconds(
                parsed
                    .query_pairs()
                    .find(|(name, _)| name == "expire")
                    .map(|(_, value)| value.to_string())
                    .as_deref(),
            )
        });
        candidates.push(Candidate {
            format,
            url: url.to_string(),
            user_agent: header(&raw.http_headers, "user-agent"),
            expires_at,
            bytes: raw.filesize,
        });
    }

    // The panel pairs every picture with the first sound of its container, so
    // only the best sound of each container is kept; the rest would be rows
    // nobody can choose.
    let mut best_audio: HashMap<String, usize> = HashMap::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let format = &candidate.format;
        if format.kind != MediaKind::Audio {
            continue;
        }
        let better = best_audio
            .get(&format.container)
            .map(|best| {
                candidates[*best].format.bandwidth.unwrap_or(0) < format.bandwidth.unwrap_or(0)
            })
            .unwrap_or(true);
        if better {
            best_audio.insert(format.container.clone(), index);
        }
    }

    let mut formats: Vec<Format> = Vec::new();
    let mut tracks: Vec<FileTrack> = Vec::new();
    for (index, candidate) in candidates.into_iter().enumerate() {
        let Candidate {
            format,
            url,
            user_agent,
            expires_at,
            bytes,
        } = candidate;
        if format.kind == MediaKind::Audio && best_audio.get(&format.container) != Some(&index) {
            continue;
        }
        let representation = RepresentationFingerprint {
            id: format.id.clone(),
            container: format.container.clone(),
            codec: format.codec.clone(),
            bytes,
        };
        let lease = RequestLease::new(
            &url,
            Authority::Native,
            "yt-dlp",
            None,
            user_agent,
            Credentials::at(lease::origin_of(&url)),
            Transport::Progressive,
            representation,
            expires_at,
        );
        formats.push(format.clone());
        tracks.push(FileTrack { format, lease });
    }

    if tracks.is_empty() {
        return Err("the video helper found no file this app can download".to_string());
    }

    let title = answer
        .title
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| fallback_title.to_string());
    let duration = answer.duration.filter(|seconds| *seconds > 0.0);

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

/// A codec the helper named, or `None` for `"none"` and a missing value.
fn codec(codec: Option<&str>) -> Option<String> {
    codec
        .filter(|codec| !codec.is_empty() && *codec != "none")
        .map(str::to_string)
}

fn header(headers: &HashMap<String, String>, wanted: &str) -> Option<String> {
    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
        .map(|(_, value)| value.clone())
}

#[cfg(test)]
mod tests {
    use super::parse;
    use idm_core::media::MediaKind;

    const ANSWER: &str = r#"{
        "title": "A film",
        "duration": 100.0,
        "formats": [
            {"format_id": "sb0", "ext": "mhtml", "protocol": "mhtml", "url": "https://i.ytimg.com/sb/x.jpg"},
            {"format_id": "233", "ext": "mp4", "protocol": "m3u8_native", "vcodec": "none", "url": "https://manifest.googlevideo.com/x.m3u8"},
            {"format_id": "137", "ext": "mp4", "protocol": "https", "vcodec": "avc1.640028", "acodec": "none", "width": 1920, "height": 1080, "fps": 30, "tbr": 4000.0, "filesize": 50000000, "url": "https://rr1.googlevideo.com/videoplayback?itag=137&expire=1893456000", "http_headers": {"User-Agent": "Mozilla/5.0"}},
            {"format_id": "140", "ext": "m4a", "protocol": "https", "vcodec": "none", "acodec": "mp4a.40.2", "tbr": 128.0, "url": "https://rr1.googlevideo.com/videoplayback?itag=140"},
            {"format_id": "140-drc", "ext": "m4a", "protocol": "https", "vcodec": "none", "acodec": "mp4a.40.2", "tbr": 129.0, "url": "https://rr1.googlevideo.com/videoplayback?itag=140"},
            {"format_id": "251", "ext": "webm", "protocol": "https", "vcodec": "none", "acodec": "opus", "tbr": 160.0, "url": "https://rr1.googlevideo.com/videoplayback?itag=251"},
            {"format_id": "18", "ext": "mp4", "protocol": "https", "vcodec": "avc1.42001E", "acodec": "mp4a.40.2", "width": 640, "height": 360, "tbr": 360.0, "filesize_approx": 4500000, "url": "https://rr1.googlevideo.com/videoplayback?itag=18"}
        ]
    }"#;

    #[test]
    fn only_plain_files_become_rows_and_tracks() {
        let (manifest, source) = parse(ANSWER, "fallback").expect("an answer");
        assert_eq!(manifest.title, "A film");
        assert_eq!(manifest.duration, Some(100.0));
        assert_eq!(
            manifest.formats.len(),
            4,
            "storyboard, playlist and DRC skipped"
        );
        assert_eq!(manifest.formats[0].kind, MediaKind::Video);
        assert_eq!(manifest.formats[0].bandwidth, Some(4_000_000));
        assert_eq!(manifest.formats[0].size, Some(50_000_000));
        let audio = manifest
            .formats
            .iter()
            .find(|format| format.kind == MediaKind::Audio)
            .expect("a sound");
        assert_eq!(audio.id, "140", "the best m4a kept, the DRC copy skipped");
        let muxed = manifest
            .formats
            .iter()
            .find(|format| format.kind == MediaKind::Muxed)
            .expect("a muxed file");
        assert_eq!(muxed.id, "18");

        let super::SourcePlan::Files { tracks } = source else {
            panic!("one file per row");
        };
        assert_eq!(tracks.len(), 4);
        assert_eq!(tracks[0].lease.client, "yt-dlp");
        assert!(tracks[0].lease.expires_at.is_some());
        assert_eq!(tracks[0].lease.user_agent.as_deref(), Some("Mozilla/5.0"));

        let video_pair = manifest
            .pairs
            .iter()
            .find(|pair| pair.video == "137")
            .expect("the picture is paired");
        assert_eq!(video_pair.audio.as_deref(), Some("140"));
        assert_eq!(video_pair.label, "1080p");
    }

    #[test]
    fn an_answer_with_no_plain_file_says_so() {
        let only_playlist = r#"{"formats":[{"format_id":"233","ext":"mp4","protocol":"m3u8_native","url":"https://x/y.m3u8"}]}"#;
        assert!(parse(only_playlist, "fallback").is_err());
    }
}
