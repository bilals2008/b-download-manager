//! The caption renditions a manifest offers.
//!
//! A manifest names its captions in its own way: an HLS master lists them as
//! `#EXT-X-MEDIA` renditions, a DASH manifest as an adaptation set of text.
//! Both become the same shape here, so the panel can show them as rows of
//! their own and the downloader can go and fetch the one that is chosen.

use serde::{Deserialize, Serialize};

use super::{Format, Manifest, MediaKind, SubtitleFormat};

/// The caption codec of a container that keeps the text as it is.
///
/// An MPEG-TS file has no caption codec of its own, and a container nobody
/// knows is not a reason to convert the text either.
pub const TEXT_AS_IS: &str = "text";

/// One caption rendition a manifest offered.
///
/// The renditions a manifest names become rows of their own in the quality
/// panel, so the language and the default flag travel with the row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleRendition {
    /// The manifest's own name for the rendition.
    pub id: String,
    /// The name the panel shows.
    pub name: String,
    /// The language the rendition is in, when the manifest says.
    pub language: Option<String>,
    /// Where the bytes are.
    pub url: String,
    /// The format the bytes are in (`vtt`, `srt`, `ttml`).
    pub container: String,
    /// Whether the manifest marks this one as the default.
    pub is_default: bool,
}

impl SubtitleRendition {
    /// The id the quality row carries.
    ///
    /// The row has no language field of its own, so the language travels in
    /// the id and in the label.
    pub fn row_id(&self) -> String {
        let language = self.language.as_deref().unwrap_or("und");
        let mut id = format!("{}-{language}", self.id);
        if self.is_default {
            id.push_str("-default");
        }
        id
    }

    /// The text the quality row shows.
    pub fn row_label(&self) -> String {
        match (&self.language, self.is_default) {
            (Some(language), true) => format!("{} ({language}, default)", self.name),
            (Some(language), false) => format!("{} ({language})", self.name),
            (None, true) => format!("{} (default)", self.name),
            (None, false) => self.name.clone(),
        }
    }

    /// The quality row this rendition becomes.
    pub fn format(&self) -> Format {
        let mut format = Format::new(self.row_id(), MediaKind::Subtitle, &self.container);
        format.label = self.row_label();
        format
    }
}

/// A manifest and the caption renditions it offered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleManifest {
    /// The rows the quality panel shows.
    pub manifest: Manifest,
    /// The caption renditions behind the caption rows.
    pub renditions: Vec<SubtitleRendition>,
}

/// Add the caption rows of these renditions to a manifest.
pub fn add_formats(manifest: &mut Manifest, renditions: &[SubtitleRendition]) {
    for rendition in renditions {
        manifest.formats.push(rendition.format());
    }
}

/// The caption codec a container wants.
///
/// An MP4 wants `mov_text`, which is SubRip's text; a Matroska file wants
/// `S_TEXT/UTF8`, which is WebVTT's text. Any other container keeps the text
/// as it is, so nothing is converted for it.
pub fn container_codec(container: &str) -> &str {
    match container.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
        "mp4" | "m4a" | "m4v" | "m4s" | "mov" => "mov_text",
        "mkv" | "webm" => "S_TEXT/UTF8",
        _ => TEXT_AS_IS,
    }
}

/// The format a caption file must be written in for a container.
///
/// An MP4 wants `mov_text`, which is SubRip's text; a Matroska file wants
/// `S_TEXT/UTF8`, which is WebVTT's text.
pub fn format_for_container(container: &str) -> SubtitleFormat {
    match container.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
        "mp4" | "m4a" | "m4v" | "m4s" | "mov" => SubtitleFormat::Srt,
        _ => SubtitleFormat::Vtt,
    }
}

/// The caption file a rendition needs for a container, written out.
///
/// A WebVTT rendition for an MP4 is written as SubRip, because that is the
/// text `mov_text` holds. A file that holds no cue is left as it is rather
/// than replaced by an empty one.
pub fn render_for_container(text: &str, from: SubtitleFormat, container: &str) -> String {
    let to = format_for_container(container);
    if to == from {
        return text.to_string();
    }
    super::convert(text, from, to).unwrap_or_else(|_| text.to_string())
}

/// The caption format an address names.
///
/// A file that names no format is read as WebVTT, because that is what a
/// site serves when it has nothing else to offer.
pub fn container_of(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let extension = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match extension.as_str() {
        "srt" | "subrip" => "srt".to_string(),
        "ttml" | "dfxp" | "xml" => "ttml".to_string(),
        _ => "vtt".to_string(),
    }
}
