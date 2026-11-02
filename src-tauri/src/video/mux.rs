//! Remuxing tracks into one file.
//!
//! **The one function the whole application calls is [`join`].** It copies the
//! bytes of the tracks into a container; it never re-encodes. The `ffmpeg`
//! sidecar ships with the application, so this works on a stock install; a
//! future native writer may replace the body of [`join`] without touching a
//! caller.
//!
//! The output is written to a temporary file beside the destination, then put
//! in place with [`idm_core::disk::replace_file`]. A cancelled or failed join
//! therefore leaves either the old file or no file, never half of one. The
//! tracks are the caller's to delete, and only after this function returned
//! successfully **and** the file was checked by [`super::verify`].
//!
//! `ffmpeg` is located by [`super::tools`]: `BDM_FFMPEG` (a test/override
//! hook), a sidecar beside the executable, then `PATH`. It is run with
//! `-c copy` and, for the MP4 family, `+faststart`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use idm_core::disk;
use idm_core::log;
use idm_core::media::{Format, MediaKind};

use super::tools::{self, Tool};

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

    /// The `-f` name of the muxer. `ipod` is the MP4 muxer's audio profile: it
    /// writes the `M4A ` brand a `.m4a` file should carry.
    fn format(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::M4a => "ipod",
            Container::Mkv => "matroska",
            Container::Webm => "webm",
        }
    }

    fn faststart(self) -> bool {
        matches!(self, Container::Mp4 | Container::M4a)
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
    /// No `ffmpeg` beside the application or on the path.
    NotFound,
    /// `ffmpeg` ran and refused. The exit code is the only detail logged.
    Failed(Option<i32>),
    /// `ffmpeg` said it was done and wrote nothing usable.
    Empty,
    TimedOut,
    Cancelled,
    Io(String),
}

impl std::fmt::Display for MuxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MuxError::NotFound => write!(
                f,
                "the tracks could not be joined because the video tool is missing from this install"
            ),
            MuxError::Failed(code) => match code {
                Some(code) => write!(
                    f,
                    "the tracks could not be joined (the video tool exited with {code})"
                ),
                None => write!(f, "the tracks could not be joined"),
            },
            MuxError::Empty => write!(f, "the tracks were joined but the file is empty"),
            MuxError::TimedOut => write!(f, "the join took too long and was stopped"),
            MuxError::Cancelled => write!(f, "the join was stopped"),
            MuxError::Io(message) => write!(f, "the join could not be written: {message}"),
        }
    }
}

impl std::error::Error for MuxError {}

/// How long one join may take. A copy of even a very large file is minutes,
/// not hours; a hung `ffmpeg` must not hold a download forever.
pub const TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// The outputs whose join was stopped. Scoped to the destination, so
/// cancelling one video never kills another's join.
static CANCEL: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// One join at a time; a second join waits rather than running beside it.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Stop the join for this destination, if one is running.
pub fn cancel(output: &Path) {
    let mut cancelled = CANCEL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !cancelled.iter().any(|path| path == output) {
        cancelled.push(output.to_path_buf());
    }
}

fn is_cancelled(output: &Path) -> bool {
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
/// This is the only remux entry point in the application. An empty input list
/// is refused; the caller never has nothing to join. A join that produced no
/// usable file is an error, so the caller never puts a zero-byte file in
/// place.
pub fn join(inputs: &[PathBuf], output: &Path, container: Container) -> Result<(), MuxError> {
    if inputs.is_empty() {
        return Err(MuxError::Io("there is nothing to join".to_string()));
    }
    for input in inputs {
        if !input.is_file() {
            return Err(MuxError::Io(format!(
                "a track is missing: {}",
                input.display()
            )));
        }
    }
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // A flag left by a cancel that arrived before this join is stale.
    clear_cancel(output);
    let _cancel_flag = CancelFlag(output);

    let ffmpeg = Tool::Ffmpeg.locate().ok_or(MuxError::NotFound)?;
    let temporary = temporary_path(output);
    let mut command = build_command(&ffmpeg, inputs, &temporary, container);
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|error| MuxError::Io(format!("the video tool could not be started: {error}")))?;

    // The tool's complaints are the only way to know why a join was refused;
    // they are read on a thread so a noisy run cannot fill the pipe and stall.
    let complaints = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let mut limited = stderr.take(64 * 1024);
            let _ = limited.read_to_string(&mut text);
            text
        })
    });

    let outcome = wait(&mut child, output, started, &temporary);
    if let Some(handle) = complaints {
        let text = handle.join().unwrap_or_default();
        let text = text.trim();
        if !text.is_empty() {
            let last = text
                .lines()
                .rev()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or(text);
            log::write("warn", &format!("ffmpeg: {}", take(last, 300)));
        }
    }
    outcome
}

/// Wait for the child, honouring cancel and the timeout.
fn wait(
    child: &mut std::process::Child,
    output: &Path,
    started: Instant,
    temporary: &Path,
) -> Result<(), MuxError> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let _ = std::fs::remove_file(temporary);
                    let code = status.code();
                    log::write(
                        "warn",
                        &format!("the video tool refused to join the tracks (exit {code:?})"),
                    );
                    return Err(MuxError::Failed(code));
                }
                let size = std::fs::metadata(temporary)
                    .map(|meta| meta.len())
                    .unwrap_or(0);
                if size == 0 {
                    let _ = std::fs::remove_file(temporary);
                    log::write("warn", "the video tool wrote no file");
                    return Err(MuxError::Empty);
                }
                if let Err(error) = disk::replace_file(temporary, output) {
                    let _ = std::fs::remove_file(temporary);
                    return Err(MuxError::Io(error.to_string()));
                }
                log::write(
                    "info",
                    &format!("joined {size} bytes into {}", output.display()),
                );
                return Ok(());
            }
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(temporary);
                return Err(MuxError::Io(error.to_string()));
            }
        }

        if is_cancelled(output) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(temporary);
            log::write("info", "the join was stopped");
            return Err(MuxError::Cancelled);
        }
        if started.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(temporary);
            log::write("warn", "the join timed out and was stopped");
            return Err(MuxError::TimedOut);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
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

fn temporary_path(output: &Path) -> PathBuf {
    let name = output
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "video".to_string());
    output.with_file_name(format!(".{name}.idmmux"))
}

fn take(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// The command, with `-c copy` and no input on stdin.
///
/// The bitstream is copied as it is. `ffmpeg` inserts the one filter a copy
/// genuinely needs (`aac_adtstoasc` for ADTS AAC into MP4, which is what every
/// TS-to-MP4 HLS join is), so no filter is named here and none can be applied
/// to a stream that does not take it.
fn build_command(
    ffmpeg: &Path,
    inputs: &[PathBuf],
    output: &Path,
    container: Container,
) -> Command {
    let mut command = tools::command_for(ffmpeg);

    command.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
    for input in inputs {
        command.arg("-i").arg(input);
    }

    if inputs.len() == 1 {
        command.args(["-map", "0:v:0?", "-map", "0:a:0?"]);
    } else {
        // Several tracks: the first input is the picture, every later one is
        // sound (the two-track case the media path produces).
        command.args(["-map", "0:v:0?"]);
        for index in 0..inputs.len() {
            command.args(["-map", &format!("{index}:a:0?")]);
        }
    }

    command.args(["-c", "copy"]);
    if container.faststart() {
        command.args(["-movflags", "+faststart"]);
    }
    command
        .args(["-f", container.format()])
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    command
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests point `BDM_FFMPEG` at a stand-in, and the module's cancel
    /// flag is shared too. Only the Windows tests run a stand-in executable,
    /// so the guard exists under the same cfg and does not read as dead code
    /// on Linux.
    #[cfg(windows)]
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        super::super::tools::test_guard()
    }

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
        assert_eq!(Container::Webm.format(), "webm");
    }

    #[test]
    fn the_command_copies_and_only_mp4_gets_faststart() {
        let input = PathBuf::from("video.track");
        let output = PathBuf::from("out");
        let mp4 = build_command(
            Path::new("ffmpeg"),
            std::slice::from_ref(&input),
            &output,
            Container::Mp4,
        );
        let args: Vec<String> = mp4
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();
        assert!(args.windows(2).any(|pair| pair == ["-c", "copy"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-movflags", "+faststart"]));
        assert!(args.windows(2).any(|pair| pair == ["-f", "mp4"]));
        assert!(args.windows(2).any(|pair| pair == ["-map", "0:v:0?"]));
        assert!(args.windows(2).any(|pair| pair == ["-map", "0:a:0?"]));

        let audio = PathBuf::from("audio.track");
        let mkv = build_command(
            Path::new("ffmpeg"),
            &[input, audio],
            &output,
            Container::Mkv,
        );
        let args: Vec<String> = mkv
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();
        assert!(args.windows(2).any(|pair| pair == ["-f", "matroska"]));
        assert!(!args.iter().any(|arg| arg == "-movflags"));
        assert!(args.windows(2).any(|pair| pair == ["-map", "1:a:0?"]));

        let sound = build_command(
            Path::new("ffmpeg"),
            &[PathBuf::from("audio.track")],
            &output,
            Container::M4a,
        );
        let args: Vec<String> = sound
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();
        assert!(args.windows(2).any(|pair| pair == ["-f", "ipod"]));
    }

    #[test]
    fn a_missing_track_is_refused_before_the_tool_runs() {
        let output = std::env::temp_dir().join(format!("bdm-mux-missing-{}", std::process::id()));
        let error = join(
            &[PathBuf::from("there-is-no-such-track.track")],
            &output,
            Container::Mp4,
        )
        .expect_err("a missing track");
        assert!(matches!(error, MuxError::Io(_)), "{error}");
        assert!(!output.exists());
    }

    #[cfg(windows)]
    fn fake_ffmpeg(folder: &Path, body: &str) -> PathBuf {
        let path = folder.join("ffmpeg.cmd");
        std::fs::write(&path, format!("@echo off\r\n{body}\r\n")).expect("the fake");
        path
    }

    #[cfg(windows)]
    #[test]
    fn a_join_writes_the_file_through_a_temporary() {
        let _guard = lock();
        let dir = std::env::temp_dir().join(format!("bdm-mux-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        let fake = fake_ffmpeg(
            &dir,
            "set \"last=\"\r\nfor %%a in (%*) do set \"last=%%a\"\r\n> \"%last%\" echo MUXED",
        );
        std::env::set_var("BDM_FFMPEG", &fake);

        let input = dir.join("video.track");
        std::fs::write(&input, b"track").expect("the track");
        let output = dir.join("film.mp4");
        join(&[input], &output, Container::Mp4).expect("a join");
        assert_eq!(std::fs::read(&output).expect("the file"), b"MUXED\r\n");
        assert!(!temporary_path(&output).exists());

        std::env::remove_var("BDM_FFMPEG");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_matroska_join_writes_the_file_too() {
        let _guard = lock();
        let dir = std::env::temp_dir().join(format!("bdm-mux-mkv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        let fake = fake_ffmpeg(
            &dir,
            "set \"last=\"\r\nfor %%a in (%*) do set \"last=%%a\"\r\n> \"%last%\" echo MKV",
        );
        std::env::set_var("BDM_FFMPEG", &fake);

        let video = dir.join("video.track");
        let audio = dir.join("audio.track");
        std::fs::write(&video, b"v").expect("the video");
        std::fs::write(&audio, b"a").expect("the audio");
        let output = dir.join("film.mkv");
        join(&[video, audio], &output, Container::Mkv).expect("a join");
        assert_eq!(std::fs::read(&output).expect("the file"), b"MKV\r\n");

        std::env::remove_var("BDM_FFMPEG");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_failed_join_leaves_no_file_and_no_temporary() {
        let _guard = lock();
        let dir = std::env::temp_dir().join(format!("bdm-mux-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        let fake = fake_ffmpeg(&dir, "echo an error 1>&2\r\nexit /b 3");
        std::env::set_var("BDM_FFMPEG", &fake);

        let input = dir.join("video.track");
        std::fs::write(&input, b"track").expect("the track");
        let output = dir.join("film.mp4");
        let error = join(&[input], &output, Container::Mp4).expect_err("a refusing ffmpeg");
        assert_eq!(error, MuxError::Failed(Some(3)));
        assert!(!output.exists());
        assert!(!temporary_path(&output).exists());

        std::env::remove_var("BDM_FFMPEG");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_tool_that_writes_nothing_is_an_error_not_an_empty_file() {
        let _guard = lock();
        let dir = std::env::temp_dir().join(format!("bdm-mux-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        let fake = fake_ffmpeg(
            &dir,
            "set \"last=\"\r\nfor %%a in (%*) do set \"last=%%a\"\r\nbreak > \"%last%\"",
        );
        std::env::set_var("BDM_FFMPEG", &fake);

        let input = dir.join("video.track");
        std::fs::write(&input, b"track").expect("the track");
        let output = dir.join("film.mp4");
        let error = join(&[input], &output, Container::Mp4).expect_err("an empty join");
        assert_eq!(error, MuxError::Empty);
        assert!(!output.exists());
        assert!(!temporary_path(&output).exists());

        std::env::remove_var("BDM_FFMPEG");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_cancelled_join_kills_the_child_and_removes_the_temporary() {
        let _guard = lock();
        let dir = std::env::temp_dir().join(format!("bdm-mux-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        let fake = fake_ffmpeg(
            &dir,
            "ping -n 20 127.0.0.1 >nul\r\nset \"last=\"\r\nfor %%a in (%*) do set \"last=%%a\"\r\n> \"%last%\" echo MUXED",
        );
        std::env::set_var("BDM_FFMPEG", &fake);

        let input = dir.join("video.track");
        std::fs::write(&input, b"track").expect("the track");
        let output = dir.join("film.mp4");
        let output_for_thread = output.clone();
        let input_for_thread = input.clone();
        let handle = std::thread::spawn(move || {
            join(&[input_for_thread], &output_for_thread, Container::Mp4)
        });
        std::thread::sleep(Duration::from_millis(300));
        cancel(&output);
        let error = handle
            .join()
            .expect("the join thread")
            .expect_err("cancelled");
        assert_eq!(error, MuxError::Cancelled);
        assert!(!output.exists());
        assert!(!temporary_path(&output).exists());

        std::env::remove_var("BDM_FFMPEG");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
