//! The application's own storage for the bytes of unfinished downloads.
//!
//! A download never leaves parts, manifests or join temporaries in the
//! folder the user chose: those live here, inside a folder the application
//! manages, which the operating system keeps out of the user's browsing
//! folders. The layout:
//!
//! ```text
//! <app data>/staging/
//!     downloads/<id>/       one folder per unfinished download:
//!                           its .part ranges and its .idm.json manifest
//!     media/<job>-<name>/   the tracks and join files of a video job
//! ```
//!
//! A download whose destination is on another volume gets a folder on that
//! volume (`<volume>\.bdm-staging\downloads\<id>`), so the finished file can
//! be put in place with a rename instead of a copy across volumes. The
//! volume root is not writable on every machine, so the folder beside the
//! application data is the fallback, and the answer either way is the folder
//! the index records.
//!
//! This is application data, not disposable cache: an active or resumable
//! download must survive a restart and must never be cleared by an OS cache
//! purge, which is why it lives in the data directory and not the cache one.
//! Completed transfers are moved out of here into the destination the user
//! picked; only then does the folder's share of the bytes go.

use std::path::{Path, PathBuf};

/// The folder every temporary of every download lives in.
pub const ROOT_NAME: &str = "staging";
/// Where the parts and manifests of unfinished downloads wait.
pub const DOWNLOADS_NAME: &str = "downloads";
/// Where the tracks of a video job wait to be joined.
pub const MEDIA_NAME: &str = "media";
/// The folder of the same name on another volume.
pub const VOLUME_ROOT_NAME: &str = ".bdm-staging";

/// The staging folder name of the earlier releases, kept so its leftovers
/// are swept once no row still points at it.
pub const LEGACY_ROOT_NAME: &str = "unfinished";

#[derive(Debug, Clone)]
pub struct Staging {
    root: PathBuf,
    downloads: PathBuf,
    media: PathBuf,
    /// The root of the volume the application data is on. A destination on
    /// that volume keeps its temporaries in the folder beside the data, which
    /// the operating system already keeps out of the reader's way.
    home_volume: Option<PathBuf>,
}

impl Staging {
    /// Create the staging tree inside the application's data directory.
    ///
    /// A failure here is a real one — the volume is full or the folder cannot
    /// be written — and the caller must not fall back to the user's download
    /// folder: an unfinished download then has nowhere clean to live, and
    /// saying so beats leaving parts in the user's way.
    pub fn resolve(app_data: &Path) -> Result<Staging, String> {
        let root = app_data.join(ROOT_NAME);
        let downloads = root.join(DOWNLOADS_NAME);
        let media = root.join(MEDIA_NAME);
        for dir in [&root, &downloads, &media] {
            std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        }
        Ok(Staging {
            home_volume: volume_root(app_data),
            root,
            downloads,
            media,
        })
    }

    /// The folder one download keeps its ranges and manifest in, for a
    /// destination at `dest`.
    ///
    /// The download owns this folder: when it finishes or is forgotten, the
    /// folder goes with it. A destination on another volume gets a folder on
    /// that volume, so the finished file is renamed into place rather than
    /// copied across a disk; a volume that cannot hold the folder falls back
    /// to the one beside the application data.
    pub fn download_dir(&self, dest: &Path, id: u64) -> PathBuf {
        self.folder_for(dest, DOWNLOADS_NAME, &id.to_string())
    }

    /// The folder one media job keeps its tracks and join files in.
    pub fn media_dir(&self, dest: &Path, job_id: u64, name: &str) -> PathBuf {
        self.folder_for(dest, MEDIA_NAME, &format!("video-{job_id}-{name}"))
    }

    /// The folder for one job, on the volume of its destination when that
    /// volume can hold it.
    fn folder_for(&self, dest: &Path, kind: &str, name: &str) -> PathBuf {
        if let Some(root) = volume_root(dest) {
            // Another volume than the application's own: a folder there lets
            // the finished file be renamed into place instead of copied
            // across a disk.
            if Some(&root) != self.home_volume.as_ref() {
                let volume = root.join(VOLUME_ROOT_NAME).join(kind).join(name);
                if std::fs::create_dir_all(&volume).is_ok() {
                    hide_folder(&root.join(VOLUME_ROOT_NAME));
                    return volume;
                }
            }
        }
        self.root.join(kind).join(name)
    }

    /// The folder that holds the per-download folders, for the sweep.
    pub fn downloads_root(&self) -> &Path {
        &self.downloads
    }

    /// The folder that holds the media job folders, for the sweep.
    pub fn media_root(&self) -> &Path {
        &self.media
    }

    /// The staging root, for the About page and the log.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// The root of the volume a path is on, when the platform can tell.
///
/// A path without a drive of its own — a relative name, or a folder on a
/// share — has no volume root to offer, and the caller keeps its own folder.
#[cfg(windows)]
fn volume_root(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut root = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Prefix(prefix) => root.push(prefix.as_os_str()),
            Component::RootDir => {
                root.push(std::path::MAIN_SEPARATOR.to_string());
                return Some(root);
            }
            _ => return None,
        }
    }
    None
}

#[cfg(not(windows))]
fn volume_root(_path: &Path) -> Option<PathBuf> {
    None
}

/// Mark a folder invisible to the shell's default view, where the platform
/// has one. A folder that refuses the flag stays visible: it is a folder the
/// application made, and a reader who sees it is only mildly surprised.
pub fn hide_folder(path: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            SetFileAttributesW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN,
        };

        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide.push(0);
        // SAFETY: `wide` is a NUL terminated UTF-16 string that outlives the
        // call, and the two attributes are a valid combination.
        unsafe {
            SetFileAttributesW(
                wide.as_ptr(),
                FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_DIRECTORY,
            )
        };
    }
    #[cfg(not(windows))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bdm-staging-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn the_tree_is_made_under_the_data_directory() {
        let dir = temp_dir("tree");
        let staging = Staging::resolve(&dir).expect("a staging tree");
        assert!(staging.root().exists());
        assert!(staging.downloads_root().exists());
        assert!(staging.media_root().exists());
        assert_eq!(
            staging.download_dir(&dir.join("film.mp4"), 7),
            dir.join("staging").join("downloads").join("7")
        );
        assert!(staging
            .media_dir(&dir.join("film.mp4"), 3, "film")
            .starts_with(dir.join("staging").join("media")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_destination_on_another_volume_gets_a_folder_there() {
        let dir = temp_dir("volume");
        let staging = Staging::resolve(&dir).expect("a staging tree");

        // A destination on the volume of the application data keeps the
        // folder beside it.
        let here = staging.download_dir(&dir.join("film.mp4"), 4);
        assert!(here.starts_with(staging.downloads_root()));

        // A destination on another drive, when that drive is there, gets its
        // own folder so the file can be renamed into place.
        #[cfg(windows)]
        {
            let other = PathBuf::from("Z:\\");
            if other.exists() {
                let on_volume = staging.download_dir(&other.join("film.mp4"), 5);
                assert!(
                    on_volume.starts_with(&other),
                    "the folder is on the volume of the destination: {on_volume:?}"
                );
                let _ = std::fs::remove_dir_all(other.join(VOLUME_ROOT_NAME));
            }
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
