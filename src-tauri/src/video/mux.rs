//! Remuxing tracks into one file.
//!
//! **The one function the whole application calls is [`join`].** It copies the
//! bytes of the tracks into a container; it never re-encodes. MP4 and M4A are
//! written natively by [`super::mp4`]; a Matroska/WebM join is refused with one
//! sentence rather than silently concatenating bytes.
//!
//! The output is written to a join temporary inside the application's staging
//! folder — never beside the destination, where a half joined file would sit in
//! the user's way — then put in place with `idm_core::disk::replace_file`. A
//! failed join therefore leaves either the old file or no file, never half of
//! one.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use idm_core::media::{Format, MediaKind};

/// The container the joined file is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Mp4,
    /// Sound alone in an MP4 container, which is what an `.m4a` file is.
    M4a,
    Mkv,
    Webm,
}

impl Container {
    pub fn extension(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::M4a => "m4a",
            Container::Mkv => "mkv",
            Container::Webm => "webm",
        }
    }

    /// The container a finished file with this name should be in.
    pub fn from_extension(extension: &str) -> Option<Container> {
        match extension.to_ascii_lowercase().as_str() {
            "mp4" => Some(Container::Mp4),
            "m4a" => Some(Container::M4a),
            "mkv" => Some(Container::Mkv),
            "webm" => Some(Container::Webm),
            _ => None,
        }
    }

    /// The container for a sound-only download, from what the manifest said.
    pub fn for_audio(format: &Format) -> Container {
        let opus = format.container == "webm" || codec_has(format, &["opus", "vorbis"]);
        if opus {
            Container::Webm
        } else {
            Container::M4a
        }
    }
}

/// Why a join did not produce the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MuxError {
    /// The join was stopped.
    Cancelled,
    /// The tracks could not be joined.
    Io(String),
}

impl std::fmt::Display for MuxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MuxError::Cancelled => write!(f, "the join was stopped"),
            MuxError::Io(message) => write!(f, "the tracks could not be joined: {message}"),
        }
    }
}

impl std::error::Error for MuxError {}

/// The outputs whose join was stopped. Scoped to the destination, so
/// cancelling one video never kills another's join.
static CANCEL: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Stop the join for this destination, if one is running.
pub fn cancel(output: &Path) {
    let mut cancelled = CANCEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !cancelled.iter().any(|path| path == output) {
        cancelled.push(output.to_path_buf());
    }
}

/// Whether a join for this destination has been asked to stop.
pub(crate) fn is_cancelled(output: &Path) -> bool {
    CANCEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .any(|path| path == output)
}

fn clear_cancel(output: &Path) {
    CANCEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(|path| path != output);
}

/// A join that ends for any reason leaves no cancel flag behind.
struct CancelFlag<'a>(&'a Path);

impl Drop for CancelFlag<'_> {
    fn drop(&mut self) {
        clear_cancel(self.0);
    }
}

/// Join the tracks in order into `output` in `container`. **No re-encode.**
///
/// The native muxers write the pairs they know. When they refuse the tracks —
/// a WebM picture with an AAC sound, or a TS stream — `ffmpeg` copies them
/// into the same container instead. An empty input list is refused; the caller
/// never has nothing to join.
pub fn join(
    inputs: &[PathBuf],
    output: &Path,
    work_dir: &Path,
    container: Container,
    ffmpeg: Option<&Path>,
) -> Result<(), MuxError> {
    if inputs.is_empty() {
        return Err(MuxError::Io("there is nothing to join".to_string()));
    }
    // A flag left by a cancel that arrived before this join is stale.
    clear_cancel(output);
    let _cancel_flag = CancelFlag(output);
    let native = match container {
        Container::Mp4 | Container::M4a => super::mp4::join(inputs, output, work_dir, container),
        Container::Mkv | Container::Webm => super::webm::join(inputs, output, work_dir, container),
    };
    match native {
        Ok(()) => Ok(()),
        Err(MuxError::Cancelled) => Err(MuxError::Cancelled),
        Err(MuxError::Io(native_error)) => match ffmpeg {
            Some(exe) => super::ffmpeg::join(exe, inputs, output, container).map_err(|error| {
                // The native joiner's reason is the precise one for bytes it
                // knows; ffmpeg's is the one that names what it could not copy.
                match error {
                    MuxError::Cancelled => MuxError::Cancelled,
                    MuxError::Io(text) => MuxError::Io(format!("{native_error}; ffmpeg: {text}")),
                }
            }),
            None => Err(MuxError::Io(format!(
                "{native_error}; install the video helpers to join this pair"
            ))),
        },
    }
}

/// Join the tracks with `ffmpeg` even when the native muxers accept them.
///
/// The delivery step calls this when the native joiner produced a file whose
/// index does not describe its own bytes: the bytes are readable, the index is
/// not, and `ffmpeg` writes a fresh one while copying the streams.
pub fn join_ffmpeg(
    inputs: &[PathBuf],
    output: &Path,
    container: Container,
    ffmpeg: &Path,
) -> Result<(), MuxError> {
    if inputs.is_empty() {
        return Err(MuxError::Io("there is nothing to join".to_string()));
    }
    // A flag left by a cancel that arrived before this join is stale.
    clear_cancel(output);
    let _cancel_flag = CancelFlag(output);
    super::ffmpeg::join(ffmpeg, inputs, output, container)
}

/// The container the research's table gives this pair of tracks.
///
/// | video | audio | output |
/// |---|---|---|
/// | `avc1`, `hev1` | `mp4a`, `ac-3`, `ec-3` | MP4 |
/// | `vp9`, `av01` | `opus`, `vorbis` | MKV |
/// | TS input | AAC | MP4 |
///
/// An unknown codec falls back to the container the manifest carried: MP4 for
/// MP4/TS, MKV for WebM/MKV. The output is never a guess about the codec; it is
/// the container that can hold what was parsed.
pub fn choose_container(video: Option<&Format>, audio: Option<&Format>) -> Container {
    let video = video.filter(|format| format.kind != MediaKind::Audio);
    let audio = audio.filter(|format| format.kind == MediaKind::Audio);

    let audio_webm = audio
        .is_some_and(|audio| audio.container == "webm" || codec_has(audio, &["opus", "vorbis"]));
    if audio_webm {
        return Container::Mkv;
    }

    let Some(video) = video else {
        return match audio {
            Some(audio) => Container::for_audio(audio),
            None => Container::Mp4,
        };
    };

    if video.container == "webm" || video.container == "mkv" {
        return Container::Mkv;
    }
    if codec_has(video, &["vp09", "vp9"]) {
        return Container::Mkv;
    }
    if codec_has(video, &["av01"]) {
        // AV1 is legal in both; without an Opus track, MP4 keeps the common
        // case simple. An Opus track was handled above.
        return Container::Mp4;
    }
    // avc1, hev1/hvc1, an unknown codec, or a TS stream: MP4 holds the first
    // three and is the research's answer for the fourth.
    Container::Mp4
}

fn codec_has(format: &Format, names: &[&str]) -> bool {
    let codec = format.codec.as_deref().unwrap_or("").to_ascii_lowercase();
    names.iter().any(|name| codec.contains(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(kind: MediaKind, container: &str, codec: &str) -> Format {
        let mut format = Format::new("id", kind, container);
        format.codec = Some(codec.to_string());
        format
    }

    #[test]
    fn the_container_table_is_the_research_table() {
        let avc = format(MediaKind::Video, "mp4", "avc1.640028");
        let mp4a = format(MediaKind::Audio, "m4a", "mp4a.40.2");
        assert_eq!(choose_container(Some(&avc), Some(&mp4a)), Container::Mp4);

        let hevc = format(MediaKind::Video, "mp4", "hev1.1.6.L93");
        let ec3 = format(MediaKind::Audio, "m4a", "ec-3");
        assert_eq!(choose_container(Some(&hevc), Some(&ec3)), Container::Mp4);

        let vp9 = format(MediaKind::Video, "webm", "vp09.00.10.08");
        let opus = format(MediaKind::Audio, "webm", "opus");
        assert_eq!(choose_container(Some(&vp9), Some(&opus)), Container::Mkv);

        let av1 = format(MediaKind::Video, "mp4", "av01.0.08M.08");
        assert_eq!(choose_container(Some(&av1), Some(&opus)), Container::Mkv);
        assert_eq!(choose_container(Some(&av1), Some(&mp4a)), Container::Mp4);

        let ts = format(MediaKind::Muxed, "ts", "");
        assert_eq!(choose_container(Some(&ts), None), Container::Mp4);
    }

    #[test]
    fn sound_alone_gets_the_container_its_codec_fits() {
        assert_eq!(
            Container::for_audio(&format(MediaKind::Audio, "m4a", "mp4a.40.2")),
            Container::M4a
        );
        assert_eq!(
            Container::for_audio(&format(MediaKind::Audio, "webm", "opus")),
            Container::Webm
        );
        assert_eq!(Container::M4a.extension(), "m4a");
        assert_eq!(Container::Webm.extension(), "webm");
    }

    #[test]
    fn a_missing_track_is_refused() {
        let output = std::env::temp_dir().join(format!("bdm-mux-missing-{}", std::process::id()));
        let work =
            std::env::temp_dir().join(format!("bdm-mux-missing-work-{}", std::process::id()));
        let error = join(
            &[PathBuf::from("there-is-no-such-track.track")],
            &output,
            &work,
            Container::Mp4,
            None,
        )
        .expect_err("a missing track");
        assert!(matches!(error, MuxError::Io(_)), "{error}");
        assert!(!output.exists());
    }

    /// The real `ffmpeg`, when this machine has one. The test skips without it;
    /// the join itself is what is being checked, not the presence of a tool.
    fn ffmpeg() -> Option<PathBuf> {
        std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|_| PathBuf::from("ffmpeg"))
            .or_else(|| {
                std::process::Command::new("ffmpeg.exe")
                    .arg("-version")
                    .output()
                    .ok()
                    .filter(|output| output.status.success())
                    .map(|_| PathBuf::from("ffmpeg.exe"))
            })
    }

    #[test]
    fn ffmpeg_joins_a_pair_the_native_muxer_cannot_read() {
        let Some(ffmpeg) = ffmpeg() else { return };
        use std::process::Command;

        let dir = std::env::temp_dir().join(format!("bdm-mux-ts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        let video = dir.join("video.track");
        let audio = dir.join("audio.track");

        // A TS picture, which the native MP4 muxer refuses, and an AAC sound
        // in an MP4, its own file.
        let made_video = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x120:rate=10",
                "-t",
                "0.4",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-f",
                "mpegts",
                "-y",
            ])
            .arg(&video)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        // A build without the encoder cannot make the fixture; it is not the
        // joiner that failed.
        if !made_video {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let made_audio = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=44100",
                "-t",
                "0.4",
                "-c:a",
                "aac",
                "-f",
                "mp4",
                "-y",
            ])
            .arg(&audio)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !made_audio {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }

        let out = dir.join("final.mp4");
        join(
            &[video, audio],
            &out,
            &dir,
            Container::Mp4,
            Some(ffmpeg.as_path()),
        )
        .expect("ffmpeg joins the pair");

        let info = super::super::verify::inspect(&out).expect("the joined file");
        assert!(info.video_codec.is_some(), "the picture is there");
        assert!(info.audio_codec.is_some(), "the sound is there");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
