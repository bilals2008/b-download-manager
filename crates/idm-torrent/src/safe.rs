//! The guard a torrent name goes through, the same one the archive code uses:
//! a name that would leave the folder the files go into is refused.

use idm_archive::Guard;

/// The name a file may be written under, or nothing when the name would
/// leave the folder.
pub fn safe_path(name: &str) -> Option<String> {
    let guard = Guard::new(64);
    let path = guard.check(name).ok()?;
    let text = path.to_string_lossy().into_owned();
    if text.is_empty() {
        return None;
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_name_is_kept() {
        assert_eq!(safe_path("Big Buck Bunny.mkv").as_deref(), Some("Big Buck Bunny.mkv"));
        assert_eq!(safe_path("Season 1/episode 2.mkv").as_deref(), Some("Season 1\\episode 2.mkv"));
    }

    #[test]
    fn a_name_that_leaves_the_folder_is_refused() {
        assert!(safe_path("../evil.mkv").is_none());
        assert!(safe_path("..\\evil.mkv").is_none());
        assert!(safe_path("a/../../evil.mkv").is_none());
        assert!(safe_path("/etc/passwd").is_none());
        assert!(safe_path("\\Windows\\system32").is_none());
        assert!(safe_path("C:\\Windows").is_none());
        assert!(safe_path("C:Windows").is_none());
        assert!(safe_path("").is_none());
    }
}
