//! The `ffmpeg` fallback joiner.
//!
//! The native muxers write the pairs they understand without a re-encode: an
//! MP4 picture and its MP4 sound, or a WebM picture and its Opus sound. Some
//! sites send a pair the native muxers cannot hold — a WebM picture with an
//! AAC sound, or an MPEG-TS stream — and those are copied into one file by
//! `ffmpeg` instead. Nothing is re-encoded there either: every stream is
//! copied as it arrived.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::mux::{is_cancelled, Container, MuxError};

/// Copy every input's streams into `output` in `container`.
///
/// The first input's picture and the first sound found are taken; a second
/// input is the sound track a site served apart from its picture.
pub fn join(
    exe: &Path,
    inputs: &[PathBuf],
    output: &Path,
    container: Container,
) -> Result<(), MuxError> {
    if inputs.is_empty() {
        return Err(MuxError::Io("there is nothing to join".to_string()));
    }
    if is_cancelled(output) {
        return Err(MuxError::Cancelled);
    }

    let mut command = Command::new(exe);
    command
        .arg("-hide_banner")
        .arg("-nostdin")
        .arg("-loglevel")
        .arg("error")
        .arg("-y");
    for input in inputs {
        command.arg("-i").arg(input);
    }
    if inputs.len() == 1 {
        if container == Container::M4a {
            command.arg("-map").arg("0:a:0");
        } else {
            command.arg("-map").arg("0");
        }
    } else {
        // The pair contract of this application: the first input is the
        // picture, the second is the sound served apart from it. A sound that
        // the picture input also carried is not copied twice.
        command.arg("-map").arg("0:v:0").arg("-map").arg("1:a:0");
    }
    command.arg("-c").arg("copy");
    match container {
        Container::Mp4 => {
            // The index goes to the front, so the file streams and seeks
            // without reading the whole of it first.
            command.arg("-movflags").arg("+faststart");
        }
        Container::M4a => {
            command.arg("-f").arg("ipod");
        }
        Container::Mkv => {
            command.arg("-f").arg("matroska");
        }
        Container::Webm => {
            command.arg("-f").arg("webm");
        }
    }
    command.arg(output);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    // A console window must not flash over the app for a join that takes a
    // second.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let result = command
        .output()
        .map_err(|error| MuxError::Io(format!("ffmpeg could not be run: {error}")))?;

    if is_cancelled(output) {
        let _ = std::fs::remove_file(output);
        return Err(MuxError::Cancelled);
    }
    if !result.status.success() {
        let _ = std::fs::remove_file(output);
        let message = String::from_utf8_lossy(&result.stderr);
        let reason = message
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .map(str::trim)
            .unwrap_or("ffmpeg refused the tracks");
        return Err(MuxError::Io(reason.to_string()));
    }
    Ok(())
}
