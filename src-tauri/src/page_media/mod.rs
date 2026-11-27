//! What a page offers to download, read from its markup.
//!
//! Add URL asks this before anything starts: the addresses a page names in a
//! `<video>`, a `<source>`, an `og:video` or an ordinary link become the short
//! list the reader picks from, and the page says whether a player stands on it
//! even when the address of what it plays is only in a script.
//!
//! Only markup is read, and only an address with a media extension is kept as
//! a file; a playlist is a video the quality panel reads, never a file the
//! hand-over saves. Pictures are the gallery's question, and a player built
//! entirely at run time is the video helper's; both are answered elsewhere.
//! Nothing here downloads, and nothing here needs the network.

use std::collections::HashSet;

use reqwest::Url;
use serde::Serialize;

use crate::bridge::dest;
use crate::grabber::extract;

/// The most files one page may offer. A machine-generated page can name a
/// thousand; the rest are left alone and the answer says so.
pub const MAX_MEDIA: usize = 60;

/// One media address a page named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundMedia {
    /// The absolute address of the file.
    pub url: String,
    /// The name the address gives, cleaned for every system.
    pub name: String,
    /// `video` or `audio`.
    pub kind: &'static str,
}

/// What one page holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMedia {
    /// The address the markup came from, after any redirect.
    pub page: String,
    /// Whether the page reads as one a video player resolves, so the quality
    /// panel is worth offering even when no file address is in the markup.
    pub video: bool,
    /// The media the markup names, videos first, then sound.
    pub files: Vec<FoundMedia>,
    /// Whether more were found than [`MAX_MEDIA`].
    pub truncated: bool,
}

/// Read the media out of a page. Nothing is downloaded.
pub fn inspect(html: &str, page_url: &str) -> PageMedia {
    let base = Url::parse(page_url.trim()).ok();
    let mut files: Vec<FoundMedia> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut truncated = false;
    // A playlist is what a player plays, not a file to save: it goes to the
    // quality panel, which reads it, and it still marks the page as a video.
    let mut playlist = is_manifest(html);
    if let Some(base) = base.as_ref() {
        for link in extract::links(html, base) {
            playlist |= is_playlist(&link.url);
            push(&mut files, &mut seen, &mut truncated, link.url);
        }
        for url in extract::meta_media(html, base) {
            playlist |= is_playlist(&url);
            push(&mut files, &mut seen, &mut truncated, url);
        }
    }
    // Videos first, then sound: the list reads as the decision, and the
    // page's own order is not one.
    files.sort_by_key(|file| file.kind != "video");

    let video = playlist
        || base
            .as_ref()
            .is_some_and(|base| crate::video::yt::is_youtube_page(base.as_str()))
        || files.iter().any(|file| file.kind == "video")
        || extract::plays_video(html);

    PageMedia {
        page: page_url.trim().to_string(),
        video,
        files,
        truncated,
    }
}

/// Add one address to the list, unless it names nothing this list carries.
fn push(
    files: &mut Vec<FoundMedia>,
    seen: &mut HashSet<String>,
    truncated: &mut bool,
    url: Url,
) {
    let Some((kind, extension)) = classify(&url) else {
        return;
    };
    if !seen.insert(url.as_str().to_string()) {
        return;
    }
    if files.len() == MAX_MEDIA {
        *truncated = true;
        return;
    }
    files.push(found(kind, url, extension));
}

fn found(kind: &'static str, url: Url, extension: &'static str) -> FoundMedia {
    let name = dest::name_from_url(url.as_str())
        .unwrap_or_else(|| format!("media.{extension}"));
    FoundMedia {
        url: url.to_string(),
        name,
        kind,
    }
}

/// What an address names, and the extension it named it with.
fn classify(url: &Url) -> Option<(&'static str, &'static str)> {
    let extension = extension(url)?.to_ascii_lowercase();
    let (kind, extension): (&'static str, &'static str) = match extension.as_str() {
        "mp4" => ("video", "mp4"),
        "m4v" => ("video", "m4v"),
        "webm" => ("video", "webm"),
        "mkv" => ("video", "mkv"),
        "mov" => ("video", "mov"),
        "ts" => ("video", "ts"),
        "m4s" => ("video", "m4s"),
        "m4a" => ("audio", "m4a"),
        "mp3" => ("audio", "mp3"),
        "aac" => ("audio", "aac"),
        "flac" => ("audio", "flac"),
        "ogg" => ("audio", "ogg"),
        "oga" => ("audio", "oga"),
        "opus" => ("audio", "opus"),
        "wav" => ("audio", "wav"),
        _ => return None,
    };
    Some((kind, extension))
}

/// The lowercase extension the last segment of the address carries.
fn extension(url: &Url) -> Option<&str> {
    let segment = url.path_segments()?.next_back()?;
    let (_, extension) = segment.rsplit_once('.')?;
    Some(extension)
}

/// Whether the address names a playlist the quality panel reads.
fn is_playlist(url: &Url) -> bool {
    matches!(
        extension(url).map(|extension| extension.to_ascii_lowercase()),
        Some(extension)
            if matches!(extension.as_str(), "m3u8" | "m3u" | "mpd" | "ism" | "ismc")
    )
}

/// Whether the text itself is a playlist, as a pasted address gives.
fn is_manifest(text: &str) -> bool {
    let head = text.trim_start();
    head.starts_with("#EXTM3U")
        || ((head.starts_with("<?xml") || head.starts_with("<MPD")) && head.contains("<MPD"))
}

#[cfg(test)]
mod tests;
