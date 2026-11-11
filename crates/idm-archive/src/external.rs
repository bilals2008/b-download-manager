//! 7z and RAR through 7-Zip, run as its own program: UnRAR's licence forbids
//! using its code in a GPL program, so nothing is linked here, and 7z's
//! decoder is not built in either (it cost about a megabyte).
//!
//! 7-Zip writes into the staging folder; what it wrote is then checked as
//! our own entries are ([`crate::safe::check_tree`]) before anything is moved
//! into place.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::listing::Entry;
use crate::safe::{self, Writer};
use crate::Error;

/// 7-Zip's program: in its usual install folders on Windows, else `7zz`, `7z`
/// or `7za` on the PATH (Linux and macOS packages name it so).
pub fn find_7zip() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for variable in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Some(dir) = std::env::var_os(variable) {
                candidates.push(PathBuf::from(dir).join("7-Zip").join("7z.exe"));
            }
        }
    }
    let names: &[&str] = if cfg!(windows) {
        &["7z.exe", "7za.exe"]
    } else {
        &["7zz", "7z", "7za"]
    };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in names {
                candidates.push(dir.join(name));
            }
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// Unpack `archive` into the writer's folder.
pub fn extract(
    tool: &Path,
    archive: &Path,
    writer: &mut Writer,
    password: Option<&str>,
) -> Result<(), Error> {
    let mut command = Command::new(tool);
    command
        .arg("x")
        .arg("-y")
        .arg("-bso0")
        .arg("-bsp0")
        // An empty password when none is given: 7-Zip then fails at once on
        // an encrypted archive instead of asking on a console nobody sees.
        .arg(format!("-p{}", password.unwrap_or("")))
        .arg(format!("-o{}", writer.root().display()))
        .arg("--")
        .arg(archive)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // No console window flashing up for it.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let out = command.output()?;
    if !out.status.success() {
        return Err(error_of(&out.stderr, password));
    }
    for (name, why) in safe::check_tree(writer.root(), 64)? {
        writer.skip(&name, why);
    }
    Ok(())
}

/// The names inside a 7z or a RAR, as 7-Zip lists them.
pub fn list(tool: &Path, archive: &Path) -> Result<Vec<Entry>, Error> {
    let mut command = Command::new(tool);
    command
        .arg("l")
        .arg("-slt")
        .arg("-bso0")
        .arg("-bsp0")
        .arg("--")
        .arg(archive)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let out = command.output()?;
    if !out.status.success() {
        return Err(error_of(&out.stderr, None));
    }
    Ok(parse_listing(&String::from_utf8_lossy(&out.stdout)))
}

/// What 7-Zip's `l -slt` output says: the archive's own block, a line of
/// dashes, then one block per entry.
fn parse_listing(output: &str) -> Vec<Entry> {
    let Some(after) = output.split_once("\n----------\n") else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for block in after.1.split("\n\n") {
        let mut path = None;
        let mut size = 0u64;
        let mut is_dir = false;
        for line in block.lines() {
            if let Some(value) = line.strip_prefix("Path = ") {
                path = Some(value.to_owned());
            } else if let Some(value) = line.strip_prefix("Size = ") {
                size = value.trim().parse().unwrap_or(0);
            } else if let Some(value) = line.strip_prefix("Attributes = ") {
                is_dir = value.trim_start().starts_with('D');
            }
        }
        if let Some(path) = path {
            entries.push(Entry {
                path,
                size,
                is_dir,
            });
        }
    }
    entries
}

/// What 7-Zip said went wrong, as one of our errors.
fn error_of(stderr: &[u8], password: Option<&str>) -> Error {
    let text = String::from_utf8_lossy(stderr).to_lowercase();
    if text.contains("wrong password") || text.contains("encrypted") {
        if password.is_some() {
            Error::WrongPassword
        } else {
            Error::NeedsPassword
        }
    } else if text.contains("missing volume") || text.contains("unexpected end of archive") {
        Error::MissingVolume(0)
    } else {
        Error::Damaged(
            text.lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("7-Zip failed")
                .trim()
                .to_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_is_read_as_paths_and_sizes() {
        let output = "\
Listing archive: C:\\tmp\\x.7z

--
Path = C:\\tmp\\x.7z
Type = 7z
Physical Size = 125

----------
Path = inner
Size = 0
Attributes = D

Path = inner\\b.txt
Size = 7
Attributes = A
";
        let entries = parse_listing(output);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "inner");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].path, "inner\\b.txt");
        assert_eq!(entries[1].size, 7);
        assert!(!entries[1].is_dir);
    }
}
