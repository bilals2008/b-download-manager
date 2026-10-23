//! File naming for a download the browser handed over.
//!
//! The extension sends a name it took from the link, from the browser's
//! download item, or from the page. None of those is trusted: the name is
//! reduced to a bare file name, so it cannot leave the download folder, and it
//! is made unique, so a second hand-over never overwrites the first file.

use std::path::{Path, PathBuf};

/// The longest name Windows accepts for one component, with a little room.
const MAX_NAME: usize = 180;

/// The reserved device names of Windows, which are not files even with an
/// extension after them.
const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// A file name that is safe on every platform, or `None` when nothing is left.
pub fn sanitize(name: &str) -> Option<String> {
    // A browser may hand over a full path. Only its last segment is a name,
    // and taking it before anything else is replaced keeps a traversal out of
    // the download folder.
    let last = name
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(name);

    let mut cleaned: String = last
        .chars()
        .filter(|character| !character.is_control())
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            other => other,
        })
        .collect();

    cleaned = cleaned.trim().trim_end_matches(['.', ' ']).to_string();

    if cleaned.chars().count() > MAX_NAME {
        cleaned = truncate(&cleaned, MAX_NAME);
    }

    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return None;
    }

    let stem = cleaned.split('.').next().unwrap_or("").to_ascii_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        cleaned = format!("_{cleaned}");
    }

    Some(cleaned)
}

/// The name the last segment of a URL path gives, percent escapes decoded.
pub fn name_from_url(url: &str) -> Option<String> {
    let without_query = url
        .split_once('#')
        .map(|(head, _)| head)
        .unwrap_or(url)
        .split_once('?')
        .map(|(head, _)| head)
        .unwrap_or(url);
    let last = without_query.rsplit('/').find(|part| !part.is_empty())?;
    sanitize(&percent_decode(last))
}

/// A destination in `folder` that no file takes yet.
///
/// The second download of the same name becomes `name (1).ext`, which is what
/// the browser and IDM both do, so the user can tell the two apart on disk.
pub fn unique(folder: &Path, name: &str) -> PathBuf {
    let name = sanitize(name).unwrap_or_else(|| "download".to_string());
    let candidate = folder.join(&name);
    if !candidate.exists() {
        return candidate;
    }

    let (stem, extension) = split_extension(&name);
    for index in 1..1000 {
        let next = folder.join(format!("{stem} ({index}){extension}"));
        if !next.exists() {
            return next;
        }
    }

    folder.join(format!("{stem} ({}){extension}", now_millis()))
}

/// The stem and the extension, dot included. A leading dot is part of the
/// name, not an extension.
fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(index) if index > 0 => (&name[..index], &name[index..]),
        _ => (name, ""),
    }
}

fn truncate(name: &str, limit: usize) -> String {
    let (stem, extension) = split_extension(name);
    if extension.len() + 1 >= limit {
        return name.chars().take(limit).collect();
    }
    let room = limit - extension.len();
    let short: String = stem.chars().take(room).collect();
    format!("{short}{extension}")
}

/// Decode the percent escapes of a URL segment.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_name_is_kept() {
        assert_eq!(sanitize("holiday.mp4").as_deref(), Some("holiday.mp4"));
    }

    #[test]
    fn a_path_cannot_escape_the_folder() {
        assert_eq!(
            sanitize("..\\..\\Windows\\System32\\evil.exe").as_deref(),
            Some("evil.exe")
        );
        assert_eq!(sanitize("../../etc/passwd").as_deref(), Some("passwd"));
    }

    #[test]
    fn the_characters_windows_refuses_become_underscores() {
        assert_eq!(
            sanitize("a<b>c:d\"e|f?g*h.mp4").as_deref(),
            Some("a_b_c_d_e_f_g_h.mp4")
        );
    }

    #[test]
    fn trailing_dots_and_spaces_are_dropped() {
        assert_eq!(sanitize("report.pdf . . ").as_deref(), Some("report.pdf"));
        assert_eq!(sanitize("   ").as_deref(), None);
        assert_eq!(sanitize(".").as_deref(), None);
        assert_eq!(sanitize("..").as_deref(), None);
    }

    #[test]
    fn a_device_name_is_prefixed() {
        assert_eq!(sanitize("CON.txt").as_deref(), Some("_CON.txt"));
        assert_eq!(sanitize("lpt9").as_deref(), Some("_lpt9"));
        assert_eq!(sanitize("console.log").as_deref(), Some("console.log"));
    }

    #[test]
    fn a_long_name_keeps_its_extension() {
        let long = format!("{}.mp4", "a".repeat(400));
        let cleaned = sanitize(&long).expect("a name");
        assert!(cleaned.chars().count() <= MAX_NAME);
        assert!(cleaned.ends_with(".mp4"));
    }

    #[test]
    fn the_name_of_a_link_is_its_last_segment() {
        assert_eq!(
            name_from_url("https://example.org/media/My%20Film.mp4?sig=1#t=2").as_deref(),
            Some("My Film.mp4")
        );
        assert_eq!(
            name_from_url("https://example.org/").as_deref(),
            Some("example.org")
        );
        assert_eq!(
            name_from_url("https://example.org").as_deref(),
            Some("example.org")
        );
    }

    #[test]
    fn a_second_file_of_the_same_name_gets_a_number() {
        let folder = std::env::temp_dir().join("bdm-bridge-dest-test");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder");

        let first = unique(&folder, "film.mp4");
        assert_eq!(first.file_name().unwrap(), "film.mp4");
        std::fs::write(&first, b"x").expect("a file");

        let second = unique(&folder, "film.mp4");
        assert_eq!(second.file_name().unwrap(), "film (1).mp4");

        let dotfile = unique(&folder, ".gitignore");
        assert_eq!(dotfile.file_name().unwrap(), ".gitignore");

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_name_without_an_extension_is_split_at_nothing() {
        assert_eq!(split_extension("readme"), ("readme", ""));
        assert_eq!(split_extension("archive.tar.gz"), ("archive.tar", ".gz"));
        assert_eq!(split_extension(".env"), (".env", ""));
    }
}
