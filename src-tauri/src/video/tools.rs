//! Finding the external tools the media path may run.
//!
//! Two tools exist, and both are found the same way: an environment override
//! first (the tests and a power user), then beside the application, then on
//! `PATH`. A release build ships `ffmpeg` as a Tauri sidecar, which Tauri
//! places beside the executable; nothing has to be installed for the app to
//! join tracks.

use std::path::{Path, PathBuf};

/// An external tool this layer knows how to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// The remuxer and the playback check.
    Ffmpeg,
    /// The optional last resort for a signed address that expired.
    YtDlp,
}

impl Tool {
    fn stem(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "ffmpeg",
            Tool::YtDlp => "yt-dlp",
        }
    }

    /// The variable a test or a power user can point at a different build.
    fn override_variable(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "BDM_FFMPEG",
            Tool::YtDlp => "BDM_YTDLP",
        }
    }

    fn names(self) -> &'static [&'static str] {
        match self {
            Tool::Ffmpeg => &["ffmpeg.exe", "ffmpeg", "ffmpeg.cmd", "ffmpeg.bat"],
            Tool::YtDlp => &["yt-dlp.exe", "yt-dlp", "yt-dlp.cmd", "yt-dlp.bat"],
        }
    }

    /// Whether this tool can be run right now.
    pub fn available(self) -> bool {
        self.locate().is_some()
    }

    /// The tool's path: the override, beside the application, or on `PATH`.
    pub fn locate(self) -> Option<PathBuf> {
        if let Some(value) = std::env::var_os(self.override_variable()) {
            let path = PathBuf::from(value);
            if path.is_file() {
                return Some(path);
            }
        }

        for folder in beside_the_app() {
            if let Some(found) = in_folder(&folder, self) {
                return Some(found);
            }
        }

        let path = std::env::var_os("PATH")?;
        for folder in std::env::split_paths(&path) {
            let candidate = folder.join(self.stem());
            if candidate.is_file() {
                return Some(candidate);
            }
            #[cfg(windows)]
            {
                let candidate = folder.join(format!("{}.exe", self.stem()));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }
}

/// One lock for every test that points a tool at a stand-in. Tests share a
/// process, so a fake `BDM_FFMPEG` must not reach another module's join or
/// playback check.
#[cfg(test)]
pub fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The folders a bundled tool can live in, nearest first.
fn beside_the_app() -> Vec<PathBuf> {
    let Ok(exe) = std::env::current_exe() else {
        return Vec::new();
    };
    let Some(folder) = exe.parent() else {
        return Vec::new();
    };
    let mut folders = vec![folder.to_path_buf()];
    // A macOS app bundle keeps sidecars in Contents/MacOS and resources in
    // Contents/Resources; both are checked so either layout works.
    folders.push(folder.join("binaries"));
    if let Some(contents) = folder.parent() {
        folders.push(contents.join("Resources"));
    }
    folders
}

fn in_folder(folder: &Path, tool: Tool) -> Option<PathBuf> {
    for name in tool.names() {
        let path = folder.join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    // A Tauri sidecar keeps its target triple in the file name when it ships
    // as an `externalBin` without being renamed (a portable or development
    // build), so the prefix is matched too.
    let prefix = format!("{}-", tool.stem());
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let name = name.to_ascii_lowercase();
        if name.starts_with(&prefix) && !name.ends_with(".zip") && !name.ends_with(".txt") {
            return Some(path);
        }
    }
    None
}

/// A `Command` that runs a tool, including a Windows `.cmd` script (the shape
/// the tests use for a stand-in).
pub fn command_for(tool: &Path) -> std::process::Command {
    #[cfg(windows)]
    {
        let script = tool
            .extension()
            .map(|extension| {
                let extension = extension.to_string_lossy().to_ascii_lowercase();
                extension == "cmd" || extension == "bat"
            })
            .unwrap_or(false);
        if script {
            let mut command = std::process::Command::new("cmd");
            command.arg("/C").arg(tool);
            return command;
        }
    }
    std::process::Command::new(tool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_override_wins_and_a_missing_override_is_ignored() {
        let dir = std::env::temp_dir().join(format!("bdm-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test folder");
        let fake = dir.join(if cfg!(windows) {
            "ffmpeg-fake.exe"
        } else {
            "ffmpeg-fake"
        });
        std::fs::write(&fake, b"not really").expect("the fake");

        std::env::set_var("BDM_FFMPEG", &fake);
        assert_eq!(Tool::Ffmpeg.locate(), Some(fake.clone()));
        std::env::remove_var("BDM_FFMPEG");

        std::env::set_var("BDM_FFMPEG", dir.join("nothing-here"));
        assert_ne!(
            Tool::Ffmpeg.locate(),
            Some(dir.join("nothing-here")),
            "a missing override falls through to the rest of the search"
        );
        std::env::remove_var("BDM_FFMPEG");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_two_tools_do_not_find_each_other() {
        assert_eq!(Tool::Ffmpeg.stem(), "ffmpeg");
        assert_eq!(Tool::YtDlp.stem(), "yt-dlp");
        assert_eq!(Tool::Ffmpeg.override_variable(), "BDM_FFMPEG");
        assert_eq!(Tool::YtDlp.override_variable(), "BDM_YTDLP");
    }
}
