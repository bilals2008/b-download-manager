//! The media layer of the desktop application.
//!
//! The extension hands over a manifest, the app fetches and parses it through
//! `idm_core::media`, the quality panel shows what was found, and a chosen pair
//! becomes one or two ordinary track downloads plus a remux. Parsing stays in
//! the engine crate; this module only owns the window, the bridge handlers and
//! the HTTP requests the parser needs.

pub(crate) mod capture;
pub(crate) mod challenge;
pub(crate) mod download;
pub(crate) mod ffmpeg;
pub(crate) mod lease;
pub(crate) mod mp4;
pub(crate) mod mux;
pub(crate) mod policy;
mod probe;
pub(crate) mod tools;
pub(crate) mod verify;
pub(crate) mod webm;
pub(crate) mod window;
pub(crate) mod yt;
pub(crate) mod ytdlp;

use serde::{Deserialize, Serialize};

use idm_core::media::download::TrackControl;
use idm_core::media::{hls, Format, Manifest, Pair};

/// Where a media job is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaState {
    Parsing,
    Ready,
    Failed,
}

/// What the extension said the address is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKindHint {
    #[default]
    Auto,
    Hls,
    Dash,
    File,
    /// A YouTube watch page the app resolves through the iOS player client.
    Youtube,
}

/// The parsed form a job keeps for the download that follows.
#[derive(Debug, Clone)]
pub enum SourcePlan {
    Hls {
        /// `None` when the address was already a media playlist.
        master: Option<hls::Master>,
        media: Option<hls::MediaPlaylist>,
        /// The address the bytes came from; relative playlist addresses
        /// resolve against it.
        base: String,
    },
    Dash {
        tracks: Vec<idm_core::media::dash::DashTrack>,
        notice: Option<String>,
    },
    /// Plain media files, one track per format. YouTube's per-itag URLs and a
    /// progressive file are both this shape; several candidates from one page
    /// are merged into one list so a video can be paired with its audio.
    Files { tracks: Vec<FileTrack> },
}

/// One plain media file: its format and the lease that serves it.
///
/// The address, the client identity it was signed for, its expiry and the
/// representation it carries all live in [`lease::RequestLease`], because a
/// bare URL cannot answer any of the questions a retry has to ask.
#[derive(Clone)]
pub struct FileTrack {
    pub format: Format,
    pub lease: lease::RequestLease,
}

impl FileTrack {
    pub fn url(&self) -> &str {
        &self.lease.url
    }
}

impl std::fmt::Debug for FileTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileTrack")
            .field("format", &self.format)
            .field("lease", &self.lease.describe())
            .finish()
    }
}

/// One manifest the extension asked about.
#[derive(Clone)]
pub struct MediaJob {
    pub id: u64,
    pub state: MediaState,
    pub error: Option<String>,
    /// A note about something the manifest offered only partly (a second DASH
    /// period, for example), shown under the title.
    pub notice: Option<String>,
    pub page_url: String,
    /// Whether the browser extension draws the quality list itself. The app
    /// must not open its window for an inline job.
    pub inline: bool,
    /// The folder the tracks are assembled in.
    pub work_dir: Option<String>,
    pub manifest_url: String,
    pub title: String,
    pub referer: String,
    pub kind: MediaKindHint,
    /// The headers that may be sent right now (already origin-scoped). They
    /// are dropped when the job ends and never written to disk.
    pub headers: Vec<(String, String)>,
    /// Every header the extension offered, kept in memory so consent can turn
    /// them on without asking the page again.
    pub offered_headers: Vec<(String, String)>,
    /// The origin those headers belong to.
    pub header_origin: String,
    /// Whether a cookie or an authorization value was offered.
    pub cookies_offered: bool,
    /// Whether the one-time consent has been given.
    pub cookie_consent: bool,
    /// The quality the panel should open on, from this host's last choice.
    pub preferred: Option<String>,
    pub manifest: Option<Manifest>,
    pub source: Option<SourcePlan>,
    /// Where the finished file is being written, once a download starts.
    pub dest: Option<String>,
    pub downloading: bool,
    pub done: bool,
    /// What the download is doing now: `Video`, `Audio`, `Saved`, ...
    pub stage: Option<String>,
    pub progress: idm_core::media::download::TrackProgress,
    /// The pair and folder the last download started with, so Resume continues
    /// the same file from the bytes on disk after Stop.
    pub request: Option<capture::DownloadRequest>,
    /// The delivery the last download was assembled with.
    pub delivery: Option<download::Delivery>,
    /// How much of the whole job is done, `0.0..=1.0`.
    ///
    /// A job with a separate sound track reports each track on its own, so the
    /// byte totals of the two do not line up: the picture reaches its own total
    /// before the sound has started. This fraction counts the finished tracks
    /// and the part of the running one, so the percentage never walks backwards.
    pub fraction: f64,
    pub control: Option<std::sync::Arc<TrackControl>>,
}

impl MediaJob {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u64,
        page_url: String,
        manifest_url: String,
        kind: MediaKindHint,
        title: String,
        referer: String,
        headers: Vec<(String, String)>,
        preferred: Option<String>,
    ) -> Self {
        Self {
            id,
            state: MediaState::Parsing,
            error: None,
            notice: None,
            page_url,
            inline: false,
            work_dir: None,
            manifest_url,
            title,
            referer,
            kind,
            headers,
            offered_headers: Vec::new(),
            header_origin: String::new(),
            cookies_offered: false,
            cookie_consent: false,
            preferred,
            manifest: None,
            source: None,
            dest: None,
            downloading: false,
            done: false,
            stage: None,
            progress: Default::default(),
            fraction: 0.0,
            request: None,
            delivery: None,
            control: None,
        }
    }

    /// The origin the forwarded headers belong to, when there is one.
    pub fn header_origin(&self) -> Option<&str> {
        (!self.header_origin.is_empty()).then_some(self.header_origin.as_str())
    }

    /// The answer the bridge and the panel read.
    pub fn view(&self) -> MediaJobView {
        let manifest = self.manifest.clone().unwrap_or_else(empty_manifest);
        MediaJobView {
            job: self.id,
            state: self.state,
            error: self.error.clone(),
            notice: self.notice.clone(),
            page_url: self.page_url.clone(),
            title: manifest.title.clone(),
            duration: manifest.duration,
            formats: manifest.formats.clone(),
            pairs: manifest.pairs.clone(),
            preferred: self.preferred.clone(),
            cookies_offered: self.cookies_offered,
            cookie_consent: self.cookie_consent,
            downloading: self.downloading,
            done: self.done,
            dest: self.dest.clone(),
            stage: self.stage.clone(),
            downloaded: self.progress.downloaded,
            total: self.progress.total,
            segments_done: self.progress.segments_done,
            segments_total: self.progress.segments_total,
            fraction: self.fraction.clamp(0.0, 1.0),
            can_resume: self.state == MediaState::Ready
                && self.request.is_some()
                && !self.downloading
                && !self.done,
            resume_video: self.request.as_ref().map(|request| request.video.clone()),
            resume_audio: self
                .request
                .as_ref()
                .and_then(|request| request.audio.clone()),
        }
    }
}

/// The JSON shape of a job: the contract's fields, plus what the panel needs
/// while the download runs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaJobView {
    pub job: u64,
    pub state: MediaState,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub page_url: String,
    pub title: String,
    pub duration: Option<f64>,
    pub formats: Vec<Format>,
    pub pairs: Vec<Pair>,
    pub preferred: Option<String>,
    pub cookies_offered: bool,
    pub cookie_consent: bool,
    pub downloading: bool,
    pub done: bool,
    pub dest: Option<String>,
    pub stage: Option<String>,
    pub downloaded: u64,
    pub total: u64,
    pub segments_done: usize,
    pub segments_total: usize,
    /// How much of the whole job is done, `0.0..=1.0`, across every track.
    pub fraction: f64,
    /// A stopped or failed download can continue from the bytes on disk.
    pub can_resume: bool,
    /// The picture and sound the resumable download was started with.
    pub resume_video: Option<String>,
    pub resume_audio: Option<String>,
}

fn empty_manifest() -> Manifest {
    Manifest {
        title: String::new(),
        duration: None,
        formats: Vec::new(),
        pairs: Vec::new(),
        container_hint: None,
    }
}

/// The host a job's quality choice belongs to: the page when it named one,
/// otherwise the manifest itself.
pub(crate) fn host_of(page_url: &str, manifest_url: &str) -> String {
    let url = if page_url.trim().is_empty() {
        manifest_url
    } else {
        page_url
    };
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_job_is_parsing_and_says_nothing_yet() {
        let job = MediaJob::new(
            7,
            "https://example.org/watch".to_string(),
            "https://cdn.example.org/master.m3u8".to_string(),
            MediaKindHint::Auto,
            "Film".to_string(),
            "https://example.org/watch".to_string(),
            Vec::new(),
            None,
        );
        let view = job.view();
        assert_eq!(view.job, 7);
        assert_eq!(view.state, MediaState::Parsing);
        assert!(view.formats.is_empty());
        assert!(view.error.is_none());
        assert!(view.preferred.is_none());
        assert!(!view.cookies_offered);
    }

    #[test]
    fn a_job_never_echoes_a_header_value() {
        let mut job = MediaJob::new(
            9,
            "https://example.org/watch".to_string(),
            "https://cdn.example.org/master.m3u8".to_string(),
            MediaKindHint::Auto,
            "Film".to_string(),
            "https://example.org/watch".to_string(),
            vec![("cookie".to_string(), "session=secret".to_string())],
            None,
        );
        job.cookies_offered = true;
        job.cookie_consent = true;
        let json = serde_json::to_string(&job.view()).expect("a view");
        assert!(!json.contains("session=secret"));
        assert!(job.view().cookies_offered);
        assert!(job.view().cookie_consent);
    }

    #[test]
    fn the_quality_choice_belongs_to_the_page_host() {
        assert_eq!(
            host_of(
                "https://www.example.org/watch",
                "https://cdn.example.net/a.m3u8"
            ),
            "www.example.org"
        );
        assert_eq!(
            host_of("", "https://cdn.example.net/a.m3u8"),
            "cdn.example.net"
        );
        assert_eq!(host_of("not a url", "also not"), "");
    }
}
