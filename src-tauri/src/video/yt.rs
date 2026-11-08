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
    expiry_from_seconds, origin_of, Authority, Credentials, RequestLease,
    RepresentationFingerprint, Transport, EXPIRY_MARGIN,
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

fn ios_profile(visitor: &str) -> PlayerProfile {
    PlayerProfile {
        label: "iOS",
        user_agent: IOS_UA,
        player_client: json!({
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
        }),
        client_name: Some("5"),
        client_version: IOS_CLIENT_VERSION,
    }
}

fn vr_profile(visitor: &str) -> PlayerProfile {
    PlayerProfile {
        label: "ANDROID_VR",
        user_agent: VR_UA,
        player_client: json!({
            "clientName": "ANDROID_VR",
            "clientVersion": VR_CLIENT_VERSION,
            "deviceMake": "Oculus",
            "deviceModel": "Quest 3",
            "osName": "Android",
            "osVersion": "12L",
            "platform": "MOBILE",
            "visitorData": visitor,
            "hl": "en",
            "gl": "US",
            "utcOffsetMinutes": 0
        }),
        // The VR client is not asked through the named header pair.
        client_name: None,
        client_version: VR_CLIENT_VERSION,
    }
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

/// One remembered answer, so two callers asking at once share the page read.
struct Remembered {
    at: Instant,
    manifest: Manifest,
    source: SourcePlan,
}

fn answers() -> &'static Mutex<HashMap<String, Remembered>> {
    static ANSWERS: OnceLock<Mutex<HashMap<String, Remembered>>> = OnceLock::new();
    ANSWERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Whether any address of a plan is too near its own deadline to start with.
///
/// Only a plain-file plan carries leases; a playlist's segments are re-signed
/// by every read of the playlist, so their age is not this module's to know.
fn expiring(source: &SourcePlan, now: SystemTime) -> bool {
    match source {
        SourcePlan::Files { tracks } => tracks
            .iter()
            .any(|track| track.lease.expiring_at(EXPIRY_MARGIN, now)),
        _ => false,
    }
}

/// The transform programs this run has learned, one per player version.
///
/// A program belongs to the exact player that issued the address, so the key
/// carries both the client and the player's own version. A player that changes
/// therefore invalidates its own entry instead of being applied to output it
/// never produced.
fn programs() -> &'static Mutex<challenge::Cache> {
    static PROGRAMS: OnceLock<Mutex<challenge::Cache>> = OnceLock::new();
    PROGRAMS.get_or_init(|| Mutex::new(challenge::Cache::new()))
}

/// Rewrite an address's throttling parameter with a learned program, if this
/// run has one for that player.
fn rewrite_throttle(identity: &PlayerIdentity, url: &str) -> Option<String> {
    let mut cache = programs().lock().ok()?;
    match challenge::rewrite(&mut cache, identity, url) {
        Ok(rewritten) => rewritten,
        Err(error) => {
            // A program that does not fit this value is not applied. The
            // address is downloaded as it came, slowly but correctly.
            idm_core::log::write(
                "warn",
                &format!("the throttling transform was not applied: {error}"),
            );
            None
        }
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
pub async fn renew(page_url: &str, fallback_title: &str) -> Result<(Manifest, SourcePlan), YtError> {
    let id = video_id(page_url).ok_or(YtError::NotYoutube)?;
    remember(&id, fallback_title, true).await
}

async fn remember(
    id: &str,
    fallback_title: &str,
    fresh: bool,
) -> Result<(Manifest, SourcePlan), YtError> {
    if !fresh {
        if let Ok(answers) = answers().lock() {
            if let Some(answer) = answers.get(id) {
                // Age is only one reason to drop an answer. Every address in it
                // carries its own deadline, and a deadline is what a server
                // actually honours: handing one back that is about to pass
                // would fail the download on its first segment for no reason.
                if answer.at.elapsed() < ANSWER_TTL
                    && !expiring(&answer.source, SystemTime::now())
                {
                    return Ok((answer.manifest.clone(), answer.source.clone()));
                }
            }
        }
    }
    let (manifest, source) = read_page(id, fallback_title).await?;
    if let Ok(mut answers) = answers().lock() {
        answers.retain(|_, answer| answer.at.elapsed() < ANSWER_TTL);
        answers.insert(
            id.to_string(),
            Remembered {
                at: Instant::now(),
                manifest: manifest.clone(),
                source: source.clone(),
            },
        );
    }
    Ok((manifest, source))
}

/// Read the watch page, ask each client, and keep the best answer.
async fn read_page(id: &str, fallback_title: &str) -> Result<(Manifest, SourcePlan), YtError> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| YtError::Page(error.to_string()))?;

    // 1. Read the watch page for the visitor data and the signature timestamp.
    let page = client
        .get(format!("https://www.youtube.com/watch?v={id}&hl=en&gl=US"))
        .header("user-agent", DESKTOP_UA)
        .header("accept-language", "en-US,en;q=0.9")
        .header("cookie", "CONSENT=YES+1")
        .send()
        .await
        .map_err(|error| YtError::Page(error.to_string()))?;
    let html = page
        .text()
        .await
        .map_err(|error| YtError::Page(error.to_string()))?;
    let visitor = text_between(&html, "\"VISITOR_DATA\":\"", '"')
        .or_else(|| text_between(&html, "\"visitorData\":\"", '"'))
        .ok_or(YtError::NoVisitor)?;
    let sts = digits_after(&html, "\"STS\":").unwrap_or(DEFAULT_STS);

    // 2. Ask each client in turn. iOS is first; the VR client answers when iOS
    //    does not, so one client losing its URL rights no longer stops every
    //    video on the page.
    let mut failures: Vec<YtError> = Vec::new();
    let mut fallback: Option<(Manifest, SourcePlan)> = None;
    let mut throttled_note = false;
    for profile in [ios_profile(visitor), vr_profile(visitor)] {
        let data = match ask_player(&client, id, visitor, sts, &profile).await {
            Ok(data) => data,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        if let Err(error) = check_playable(&data, id) {
            // A definite refusal is final: another client will say the same.
            if matches!(
                error,
                YtError::Protected(_) | YtError::Live | YtError::Identity { .. }
            ) {
                return Err(error);
            }
            failures.push(error);
            continue;
        }
        match plan_from(&data, fallback_title, &profile) {
            Ok(planned) => {
                // An answer whose addresses all need a transform this build
                // cannot verify is kept, but not preferred: a throttled
                // address downloads, only slowly.
                if planned.throttled {
                    throttled_note = true;
                    fallback.get_or_insert((planned.manifest, planned.source));
                } else {
                    return Ok((planned.manifest, planned.source));
                }
            }
            Err(error) => failures.push(error),
        }
    }
    if let Some(answer) = fallback {
        if throttled_note {
            idm_core::log::write(
                "info",
                "the player's throttling parameter is present and untransformed; the download may be slow",
            );
        }
        return Ok(answer);
    }
    Err(merge_failures(failures))
}

/// Fold the clients' failures into the one a person reads.
fn merge_failures(failures: Vec<YtError>) -> YtError {
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
fn check_playable(data: &Value, asked: &str) -> Result<(), YtError> {
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
        let protected = data
            .pointer("/streamingData/drmParams")
            .is_some()
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
                    item.get("drmFamilies").is_some()
                        || item.get("licenseInfos").is_some()
                })
            })
}

/// The player answer of one client.
async fn ask_player(
    client: &reqwest::Client,
    id: &str,
    visitor: &str,
    sts: u64,
    profile: &PlayerProfile,
) -> Result<Value, YtError> {
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
            "client": profile.player_client,
            "user": { "enableSafetyMode": false, "lockedSafetyMode": false },
            "request": { "useSsl": true, "internalExperimentFlags": [] }
        }
    });
    let mut request = client
        .post("https://www.youtube.com/youtubei/v1/player?prettyPrint=false&alt=json")
        .header("user-agent", profile.user_agent)
        .header("accept", "*/*")
        .header("accept-language", "*")
        .header("origin", "https://www.youtube.com")
        .header("x-goog-visitor-id", visitor)
        .header("content-type", "application/json");
    if let Some(name) = profile.client_name {
        request = request
            .header("x-youtube-client-name", name)
            .header("x-youtube-client-version", profile.client_version);
    }
    let response = request
        .body(
            serde_json::to_vec(&body)
                .map_err(|error| YtError::Client {
                    client: profile.label.to_string(),
                    reason: error.to_string(),
                })?,
        )
        .send()
        .await
        .map_err(|error| YtError::Transient {
            message: format!("the video could not be asked for: {error}"),
            retry_after: None,
        })?;
    let status = response.status();
    let retry_after = idm_core::http::parse_retry_after(response.headers());
    let text = response.text().await.map_err(|error| YtError::Transient {
        message: format!("the video answer could not be read: {error}"),
        retry_after: None,
    })?;
    if status.as_u16() == 429 || status.is_server_error() {
        return Err(YtError::Transient {
            message: format!("the player answered {}", status.as_u16()),
            retry_after,
        });
    }
    serde_json::from_str(&text).map_err(|error| YtError::Client {
        client: profile.label.to_string(),
        reason: format!("the video answer could not be read: {error}"),
    })
}

/// What one client's answer produced, and whether its addresses need a
/// transform this build cannot verify.
struct Planned {
    manifest: Manifest,
    source: SourcePlan,
    throttled: bool,
}

/// The formats of one player answer, as the manifest and the plans the rest
/// of the media path downloads.
fn plan_from(
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
fn player_version(data: &Value) -> Option<String> {
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
fn codec_of(mime: &str) -> Option<String> {
    let (_, rest) = mime.split_once("codecs=")?;
    let codec = rest.trim().trim_matches('"').split(',').next()?.trim();
    (!codec.is_empty() && codec.len() <= 64).then(|| codec.to_string())
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

    fn answer() -> Value {
        json!({
            "playabilityStatus": { "status": "OK" },
            "videoDetails": { "videoId": "dQw4w9WgXcQ", "title": "A song", "lengthSeconds": "212" },
            "streamingData": {
                "adaptiveFormats": [
                    {
                        "itag": 137,
                        "mimeType": "video/mp4; codecs=\"avc1.640028\"",
                        "bitrate": 4000000,
                        "contentLength": "24000000",
                        "height": "1080",
                        "url": "https://rr1.googlevideo.com/videoplayback?itag=137&expire=1893456000&sig=secret"
                    },
                    {
                        "itag": 140,
                        "mimeType": "audio/mp4; codecs=\"mp4a.40.2\"",
                        "bitrate": 128000,
                        "contentLength": "3400000",
                        "url": "https://rr2.googlevideo.com/videoplayback?itag=140&expire=1893456000&sig=secret"
                    }
                ]
            }
        })
    }

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

    #[test]
    fn a_fallback_client_names_itself() {
        let ios = ios_profile("Cgtabc");
        let vr = vr_profile("Cgtabc");
        assert_eq!(ios.player_client["clientName"], "iOS");
        assert!(ios.client_name.is_some());
        assert_eq!(vr.player_client["clientName"], "ANDROID_VR");
        assert_eq!(vr.player_client["visitorData"], "Cgtabc");
        assert!(vr.client_name.is_none());
    }

    #[test]
    fn an_answer_becomes_leases_not_bare_addresses() {
        let planned = plan_from(&answer(), "Fallback", &ios_profile("Cgtabc")).expect("a plan");
        assert!(!planned.throttled);
        let tracks = match planned.source {
            SourcePlan::Files { tracks } => tracks,
            _ => panic!("files expected"),
        };
        assert_eq!(tracks.len(), 2);
        let video = tracks
            .iter()
            .find(|track| track.format.id == "137")
            .expect("a picture");
        assert_eq!(video.lease.authority, Authority::Native);
        assert_eq!(video.lease.client, "iOS");
        assert_eq!(video.lease.user_agent.as_deref(), Some(IOS_UA));
        assert!(video.lease.expires_at.is_some(), "the expire says when");
        assert_eq!(video.lease.representation.id, "137");
        assert_eq!(video.lease.representation.bytes, Some(24_000_000));
        assert_eq!(video.lease.representation.codec.as_deref(), Some("avc1.640028"));
        // The signed value is not in anything a log would print.
        assert!(!video.lease.describe().contains("secret"));
    }

    #[test]
    fn an_answer_about_another_video_is_refused() {
        let mut data = answer();
        data["videoDetails"]["videoId"] = json!("somethingElse");
        assert_eq!(
            check_playable(&data, "dQw4w9WgXcQ"),
            Err(YtError::Identity {
                asked: "dQw4w9WgXcQ".to_string(),
                answered: "somethingElse".to_string()
            })
        );
    }

    #[test]
    fn a_refusal_keeps_the_players_reason() {
        let data = json!({
            "playabilityStatus": { "status": "UNPLAYABLE", "reason": "This video is private" }
        });
        let error = check_playable(&data, "x").expect_err("a refusal");
        assert!(error.to_string().contains("This video is private"), "{error}");
        assert!(error.to_string().contains("UNPLAYABLE"), "{error}");
    }

    #[test]
    fn a_licence_is_named_as_protection_not_as_a_generic_failure() {
        let mut data = answer();
        data["streamingData"]["drmParams"] = json!("a licence");
        assert!(matches!(
            check_playable(&data, "dQw4w9WgXcQ"),
            Err(YtError::Protected(_))
        ));
    }

    #[test]
    fn sabr_only_is_its_own_answer() {
        let data = json!({
            "playabilityStatus": { "status": "OK" },
            "videoDetails": { "videoId": "x" },
            "streamingData": { "serverAbrStreamingUrl": "https://www.youtube.com/api" }
        });
        assert_eq!(plan_from(&data, "x", &ios_profile("v")).err(), Some(YtError::NoPlainAddress));
    }

    #[test]
    fn a_throttled_answer_is_kept_but_marked() {
        let mut data = answer();
        data["streamingData"]["adaptiveFormats"][0]["url"] = json!(
            "https://rr1.googlevideo.com/videoplayback?itag=137&n=AbC"
        );
        let planned = plan_from(&data, "x", &ios_profile("v")).expect("a plan");
        assert!(planned.throttled);
        let tracks = match planned.source {
            SourcePlan::Files { tracks } => tracks,
            _ => panic!("files expected"),
        };
        assert!(tracks[0].lease.note.is_some());
    }

    #[test]
    fn the_player_script_names_the_identity_a_transform_belongs_to() {
        let mut data = answer();
        data["playerConfig"] = json!({
            "assets": { "js": "/s/player/1a2b3c4d/player_ias.vflset/en_US/base.js" }
        });
        assert_eq!(player_version(&data).as_deref(), Some("1a2b3c4d"));

        data["playerConfig"]["assets"]["js"] = json!("https://example.org/elsewhere.js");
        assert_eq!(player_version(&data), None);
        data["playerConfig"]["assets"]["js"] = json!("/s/player/..%2Fetc/player_ias.js");
        assert_eq!(player_version(&data), None);
        assert_eq!(player_version(&answer()), None);
    }

    #[test]
    fn a_learned_transform_is_applied_only_to_its_own_player() {
        // This client name belongs to this test alone, so a parallel test's
        // program cannot be the one that answers.
        let platform = PlayerIdentity::new("TEST-REWRITE", "1a2b3c4d");
        programs()
            .lock()
            .expect("the cache")
            .put(platform, challenge::Program::parse("reverse").expect("a program"));

        let mut data = answer();
        data["playerConfig"] = json!({
            "assets": { "js": "/s/player/1a2b3c4d/player_ias.vflset/en_US/base.js" }
        });
        data["streamingData"]["adaptiveFormats"][0]["url"] =
            json!("https://rr1.googlevideo.com/videoplayback?itag=137&n=abc");
        let mut profile = ios_profile("v");
        profile.label = "TEST-REWRITE";
        let planned = plan_from(&data, "x", &profile).expect("a plan");
        assert!(!planned.throttled, "the transform removed the throttling");
        let tracks = match planned.source {
            SourcePlan::Files { tracks } => tracks,
            _ => panic!("files expected"),
        };
        assert!(tracks[0].url().contains("n=cba"), "{}", tracks[0].url());
        assert!(tracks[0].url().contains("itag=137"));

        // The same answer from a player this build never learned keeps its
        // address untouched and stays marked as throttled.
        data["playerConfig"]["assets"]["js"] =
            json!("/s/player/9z9z9z9z/player_ias.vflset/en_US/base.js");
        let planned = plan_from(&data, "x", &profile).expect("a plan");
        assert!(planned.throttled);
        let tracks = match planned.source {
            SourcePlan::Files { tracks } => tracks,
            _ => panic!("files expected"),
        };
        assert!(tracks[0].url().contains("n=abc"), "{}", tracks[0].url());
    }

    #[test]
    fn the_codec_is_read_out_of_the_mime_type() {
        assert_eq!(
            codec_of("video/mp4; codecs=\"avc1.640028\"").as_deref(),
            Some("avc1.640028")
        );
        assert_eq!(codec_of("audio/webm").as_deref(), None);
        assert_eq!(codec_of("video/mp4; codecs=\"\"").as_deref(), None);
    }

    #[test]
    fn every_client_failing_names_every_client() {
        let error = merge_failures(vec![
            YtError::Client {
                client: "iOS".to_string(),
                reason: "refused".to_string(),
            },
            YtError::NoStreams,
        ]);
        let text = error.to_string();
        assert!(text.contains("refused"), "{text}");
        assert!(text.contains("no downloadable stream"), "{text}");
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
                    tracks.iter().all(|t| t.url().starts_with("http")),
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
                .filter(|t| {
                    t.format.kind == MediaKind::Video && t.format.size.unwrap_or(0) > 20_000_000
                })
                .min_by_key(|t| t.format.size.unwrap_or(u64::MAX))
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
            segments: vec![idm_core::media::TrackSegment {
                url: track.url().to_string(),
                range: None,
            }],
            dest: dest.clone(),
            work_dir: dir.clone(),
            stem: "video".to_string(),
            connections: 8,
            proxy: None,
            referer: Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string()),
            headers: track
                .lease
                .user_agent
                .iter()
                .map(|user_agent| ("user-agent".to_string(), (*user_agent).to_string()))
                .collect(),
            header_origin: None,
            fingerprint: Some(track.lease.representation.key()),
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
        eprintln!(
            "outcome {:?} bytes {} in {:?}",
            outcome,
            size,
            start.elapsed()
        );
        assert!(size > 1_000_000, "a whole track");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = manifest;
    }
}

#[cfg(test)]
mod live_fallback {
    use super::*;
    use idm_core::media::MediaKind;

    #[tokio::test]
    #[ignore = "reaches YouTube and downloads"]
    async fn the_vr_client_resolves_and_downloads_a_track() {
        let id = video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ").expect("a video id");
        let client = reqwest::Client::builder().build().expect("client");
        let page = client
            .get(format!("https://www.youtube.com/watch?v={id}&hl=en&gl=US"))
            .header("user-agent", DESKTOP_UA)
            .header("cookie", "CONSENT=YES+1")
            .send()
            .await
            .expect("page");
        let html = page.text().await.expect("html");
        let visitor = text_between(&html, "\"VISITOR_DATA\":\"", '"').expect("visitor");
        let sts = digits_after(&html, "\"STS\":").unwrap_or(DEFAULT_STS);
        let data = ask_player(&client, &id, visitor, sts, &vr_profile(visitor))
            .await
            .expect("the VR answer");
        let planned = plan_from(&data, "Video", &vr_profile(visitor)).expect("a plan");
        assert!(
            !planned.manifest.formats.is_empty(),
            "the VR answer offered formats"
        );
        let track = match planned.source {
            SourcePlan::Files { tracks } => tracks
                .into_iter()
                .filter(|t| t.format.kind == MediaKind::Video)
                .min_by_key(|t| t.format.size.unwrap_or(u64::MAX))
                .filter(|t| t.format.size.unwrap_or(0) > 1_000_000)
                .expect("a video track"),
            _ => panic!("files expected"),
        };
        let dir = std::env::temp_dir().join(format!("bdm-yt-vr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let dest = dir.join("video.track");
        let outcome = idm_core::media::download::download_track(
            idm_core::limits::Limits::default(),
            idm_core::media::download::TrackSource {
                init: None,
                segments: vec![idm_core::media::TrackSegment {
                    url: track.url().to_string(),
                    range: None,
                }],
                dest: dest.clone(),
                work_dir: dir.clone(),
                stem: "video".to_string(),
                connections: 4,
                proxy: None,
                referer: None,
                headers: track
                    .lease
                    .user_agent
                    .iter()
                    .map(|user_agent| ("user-agent".to_string(), (*user_agent).to_string()))
                    .collect(),
                header_origin: None,
                fingerprint: Some(track.lease.representation.key()),
            },
            None,
            None,
        )
        .await;
        let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        eprintln!(
            "vr itag {} size {:?} outcome {:?} bytes {}",
            track.format.id, track.format.size, outcome, size
        );
        assert!(size > 1_000_000, "a whole track");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
