//! YouTube resolution through its InnerTube clients.
//!
//! The web player streams through SABR/UMP over POST and no longer hands over a
//! plain file address; the legacy `get_video_info` endpoint is gone (`410`).
//! The clients that still return a signed, direct `url` for every format are
//! asked here — no JavaScript player, no external tool — and their formats
//! become the same `Files` plan the rest of the media path already downloads
//! and muxes. iOS is the primary client; the ANDROID_VR client follows it, so
//! one client losing its URL rights does not stop every video.
//!
//! Each request needs the page's `visitorData` and `signatureTimestamp`
//! (`STS`), both read from the watch page. The formats the clients return are
//! ordinary GET addresses; the app downloads them like any file.
//!
//! An answer is only accepted when it is an answer *about this video* and the
//! addresses in it can be used as they are. Every accepted address becomes a
//! [`RequestLease`]: the client profile it was signed for, the representation
//! behind it, and the deadline its own `expire` parameter states. A client
//! whose addresses all need a throttling transform this build cannot verify is
//! not chosen while another client offers usable ones.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use idm_core::media::{quality, youtube, Format, Manifest, MediaKind};
use reqwest::Url;
use serde_json::{json, Value};

use super::challenge::{self, PlayerIdentity};
use super::lease::{
    expiry_from_seconds, origin_of, Authority, Credentials, RepresentationFingerprint,
    RequestLease, Transport, EXPIRY_MARGIN,
};
use super::{FileTrack, SourcePlan};

/// The iOS client the player response is asked from.
const IOS_CLIENT_VERSION: &str = "20.11.6";
const IOS_UA: &str = "com.google.ios.youtube/20.11.6 (iPhone10,4; U; CPU iOS 16_7_7 like Mac OS X)";
/// The user agent the watch page is read with.
const DESKTOP_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
/// The signature timestamp used when the page does not state one.
const DEFAULT_STS: u64 = 20_759;

/// The ANDROID_VR client, the one that answers without a proof-of-origin
/// token when the iOS client does not.
const VR_CLIENT_VERSION: &str = "1.60.19";
const VR_UA: &str =
    "com.google.android.apps.youtube.vr.oculus/1.60.19 (Linux; U; Android 12L; Quest 3 Build/SQ3A.220605.009.A1) gzip";

/// How long an answer is reused before the page is asked again. A job that
/// begins long after its page was read resolves first; two refreshes that
/// arrive together share one answer instead of asking twice.
const ANSWER_TTL: Duration = Duration::from_secs(45);

/// Why a page could not be resolved, in a form the caller can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YtError {
    /// The address is not a watch page this module can resolve.
    NotYoutube,
    /// The watch page itself could not be read.
    Page(String),
    /// The page carried no visitor identity.
    NoVisitor,
    /// One client did not answer usefully. Several are recorded before the
    /// resolver gives up, so the message names every one that was tried.
    Client { client: String, reason: String },
    /// The player said why it would not serve the video.
    Unplayable { status: String, reason: String },
    /// The answer was about a different video than the one asked for.
    Identity { asked: String, answered: String },
    /// The video needs a licence (DRM). Never downloaded, never decrypted.
    Protected(String),
    /// A live broadcast. Recording live is not in this feature.
    Live,
    /// The only transport on offer (SABR/UMP) has no plain address.
    NoPlainAddress,
    /// Every client answered, and none offered a usable address.
    NoStreams,
    /// A failure that asking again may fix, with the wait the server named.
    Transient {
        message: String,
        retry_after: Option<Duration>,
    },
}

impl std::fmt::Display for YtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            YtError::NotYoutube => write!(f, "the page does not name a YouTube video"),
            YtError::Page(what) => write!(f, "the video page could not be read: {what}"),
            YtError::NoVisitor => write!(f, "the video page did not name a visitor"),
            YtError::Client { client, reason } => {
                write!(f, "the video could not be asked for: {reason} ({client})")
            }
            YtError::Unplayable { status, reason } => {
                write!(f, "the video could not be asked for: {reason} ({status})")
            }
            YtError::Identity { asked, answered } => write!(
                f,
                "the player answered about a different video ({answered}, not {asked})"
            ),
            YtError::Protected(what) => {
                write!(f, "this video is protected (DRM), so it cannot be downloaded: {what}")
            }
            YtError::Live => write!(
                f,
                "this is a live stream; recording live streams is not supported"
            ),
            YtError::NoPlainAddress => write!(
                f,
                "this video is only offered through the player's streaming protocol, which has no downloadable address"
            ),
            YtError::NoStreams => write!(f, "the video offered no downloadable stream"),
            YtError::Transient {
                message,
                retry_after,
            } => match retry_after {
                Some(wait) => write!(f, "{message} (try again in {}s)", wait.as_secs()),
                None => f.write_str(message),
            },
        }
    }
}

impl std::error::Error for YtError {}

/// One player client. They share the call and the answer's shape; only the
/// identity they present differs, so a client that loses its URL rights can
/// be replaced by the next one.
struct PlayerProfile {
    /// What the log calls this client.
    label: &'static str,
    user_agent: &'static str,
    player_client: serde_json::Value,
    /// `x-youtube-client-name`. Some clients are asked without it.
    client_name: Option<&'static str>,
    client_version: &'static str,
}

/// What one client's answer produced, and whether its addresses need a
/// transform this build cannot verify.
struct Planned {
    manifest: Manifest,
    source: SourcePlan,
    throttled: bool,
}

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
///
/// The page is read at most once per [`ANSWER_TTL`] for one video, so a
/// download that starts right after a preview and a refresh that arrives at
/// the same moment do not read it twice.
pub async fn resolve(
    page_url: &str,
    fallback_title: &str,
) -> Result<(Manifest, SourcePlan), YtError> {
    let id = video_id(page_url).ok_or(YtError::NotYoutube)?;
    remember(&id, fallback_title, false).await
}

/// Resolve a page for an address that stopped working.
///
/// A refresh never reuses an answer: the whole point is that the previous one
/// is no longer accepted. The caller compares the new tracks with the old
/// before it reuses a single byte.
pub async fn renew(
    page_url: &str,
    fallback_title: &str,
) -> Result<(Manifest, SourcePlan), YtError> {
    let id = video_id(page_url).ok_or(YtError::NotYoutube)?;
    remember(&id, fallback_title, true).await
}

mod client;
mod parse;
mod resolver;
pub(crate) mod transcript;

use resolver::remember;

#[cfg(test)]
mod live;
#[cfg(test)]
mod live_download;
#[cfg(test)]
mod live_fallback;
#[cfg(test)]
mod tests;
