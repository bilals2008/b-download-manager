//! The destination path of a new download.
//!
//! The add dialog checks the path before it starts anything, so a name the
//! file system cannot hold is a sentence in the dialog rather than the message
//! of a failed download. The same check runs on the way in, because the
//! browser bridge and the command line can reach the engine without the
//! dialog.

use std::path::{Path, PathBuf};

/// The longest name one component of a path may have, with room to spare.
const MAX_NAME: usize = 200;

/// The reserved device names of Windows, which name a device even with an
/// extension after them.
const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// What the dialog needs to know about a destination before it starts.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    /// The folder the file goes into, as the engine will report it.
    pub folder: String,
    /// Whether that folder is there already. A missing one is created.
    pub folder_exists: bool,
    /// Whether a file of that name is already there, and would be replaced.
    pub file_exists: bool,
}

/// Refuse a destination that cannot hold a file.
pub fn validate(dest: &Path) -> Result<(), String> {
    let raw = dest.to_string_lossy();
    if raw.trim().is_empty() {
        return Err("a destination file is required".to_string());
    }
    if raw.ends_with('/') || raw.ends_with('\\') {
        return Err("the destination must name a file, not a folder".to_string());
    }

    // Check the whole path, not just the last part: a separator makes the
    // parts folders, but a colon or a star is as unusable in a folder as in a
    // file name. The drive prefix and a verbatim marker are not part of it.
    let body = raw
        .strip_prefix(r"\\?\")
        .or_else(|| raw.strip_prefix(r"\\"))
        .unwrap_or(&raw);
    let body = match body.as_bytes() {
        [drive, b':', ..] if drive.is_ascii_alphabetic() => &body[2..],
        _ => body,
    };
    if body.contains(|c: char| c.is_control()) {
        return Err("the destination contains a character that cannot be shown".to_string());
    }
    if body.contains(['<', '>', ':', '"', '|', '?', '*']) {
        return Err("the destination cannot contain < > : \" | ? *".to_string());
    }

    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .ok_or("the destination needs a file name")?;

    if name.chars().count() > MAX_NAME {
        return Err(format!(
            "the file name is longer than {MAX_NAME} characters"
        ));
    }
    if name == "." || name == ".." {
        return Err("that is not a file name".to_string());
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err("a file name cannot end with a dot or a space".to_string());
    }

    let stem = name.split('.').next().unwrap_or("").to_ascii_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err(format!("{name} is a name Windows keeps for a device"));
    }

    if dest.is_dir() {
        return Err(format!("{} is a folder", dest.display()));
    }

    let folder = folder_of(dest);
    check_folder(&folder)?;

    Ok(())
}

/// The folder a destination sits in.
pub fn folder_of(dest: &Path) -> PathBuf {
    dest.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The last part of a stored destination, for the task switcher and the tray.
///
/// A destination is a Windows path even when the tests run elsewhere, so both
/// separators split it rather than only the one the host happens to use.
pub fn last_segment(dest: &str) -> Option<String> {
    dest.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// Look at a destination: the folder it goes into, and the file already there.
pub fn inspect(dest: &Path) -> Result<Destination, String> {
    validate(dest)?;
    let folder = folder_of(dest);
    Ok(Destination {
        folder: folder.to_string_lossy().to_string(),
        folder_exists: folder.is_dir(),
        file_exists: dest.is_file(),
    })
}

/// The nearest part of the folder that exists has to be a folder: a file in
/// the middle of the path would make the engine create the rest underneath a
/// file, which fails with a message about the wrong thing.
fn check_folder(folder: &Path) -> Result<(), String> {
    let mut ancestor = folder;
    loop {
        if ancestor.exists() {
            if ancestor.is_dir() {
                return Ok(());
            }
            return Err(format!("{} is a file, not a folder", ancestor.display()));
        }
        match ancestor.parent() {
            Some(parent) => ancestor = parent,
            // A path with no existing part at all is left to the engine: it
            // may be a drive that only exists when it is mounted.
            None => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_segment_splits_both_separators() {
        assert_eq!(
            last_segment(r"C:\Downloads\a.bin").as_deref(),
            Some("a.bin")
        );
        assert_eq!(last_segment("/home/u/a.bin").as_deref(), Some("a.bin"));
        assert_eq!(last_segment("a.bin").as_deref(), Some("a.bin"));
        assert_eq!(last_segment(""), None);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("my-idm-clone-dest-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_plain_destination_is_accepted() {
        let dir = temp_dir("plain");
        let dest = dir.join("film.mp4");
        assert!(validate(&dest).is_ok());
        let seen = inspect(&dest).expect("a destination");
        assert_eq!(seen.folder, dir.to_string_lossy());
        assert!(seen.folder_exists);
        assert!(!seen.file_exists);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_existing_file_is_reported_not_refused() {
        let dir = temp_dir("exists");
        let dest = dir.join("film.mp4");
        std::fs::write(&dest, b"old").expect("write");
        let seen = inspect(&dest).expect("a destination");
        assert!(seen.file_exists, "the dialog asks before replacing it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_instead_of_a_file_is_refused() {
        let dir = temp_dir("folder");
        let error = validate(&dir).expect_err("the folder must be refused");
        assert!(error.contains("folder"), "the error was: {error}");
        assert!(validate(&dir.join("sub").join(" ")).is_err());
        assert!(validate(Path::new("C:\\temp\\")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_with_a_separator_in_it_is_a_folder() {
        let dir = temp_dir("separator");
        assert!(validate(&dir.join("name.zip")).is_ok());
        assert!(validate(Path::new("C:\\temp\\")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_device_name_is_refused() {
        let dir = temp_dir("device");
        for name in ["CON.txt", "lpt9", "nul.tar.gz"] {
            let error = validate(&dir.join(name)).expect_err("a device name");
            assert!(error.contains("device"), "{name}: {error}");
        }
        assert!(validate(&dir.join("console.log")).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_windows_cannot_hold_is_refused() {
        let dir = temp_dir("bad-name");
        for name in ["a:b.zip", "a?b.zip", "a*b.zip", "a|b.zip", "a\"b.zip"] {
            // Compose the path as a string: `join` treats "a:b.zip" as a drive
            // of its own and throws the folder away.
            let dest = PathBuf::from(format!("{}\\{name}", dir.display()));
            assert!(validate(&dest).is_err(), "{name} must be refused");
        }
        assert!(validate(&dir.join("report.pdf.")).is_err());
        assert!(validate(&dir.join("report.pdf ")).is_err());
        assert!(validate(Path::new(r"C:\temp\a:b\film.mp4")).is_err());
        assert!(validate(Path::new(r"\\?\C:\temp\film.mp4")).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_under_a_file_is_refused() {
        let dir = temp_dir("file-in-path");
        let file = dir.join("notes.txt");
        std::fs::write(&file, b"x").expect("write");
        let error =
            validate(&file.join("sub").join("film.mp4")).expect_err("a file cannot be a folder");
        assert!(error.contains("not a folder"), "the error was: {error}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
