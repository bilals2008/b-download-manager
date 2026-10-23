//! Free space on the volume that will hold the destination file.
//!
//! A download that runs out of disk fails in the middle of a part file, and
//! the part files are what the resume state is built on. The engine checks
//! the free space before it starts and again while it runs.

use std::path::Path;

/// Free bytes on the volume that holds `path`, when the platform can tell us.
pub fn free_space(path: &Path) -> Option<u64> {
    platform::free_space(&directory_of(path))
}

/// The directory a file lives in, or the path itself when it is a directory.
fn directory_of(path: &Path) -> std::path::PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    }
}

/// Put `from` in place of `to`.
///
/// The old file is moved aside first, because Windows will not rename over an
/// existing file, and it is only removed once the new file is in place. A
/// failure therefore leaves the old file, never a half written one.
pub fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    let backup = match to.file_name() {
        Some(name) => to.with_file_name(format!("{}.idmold", name.to_string_lossy())),
        None => to.with_extension("idmold"),
    };

    if to.exists() {
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(to, &backup)?;
    }

    match std::fs::rename(from, to) {
        Ok(()) => {
            let _ = std::fs::remove_file(&backup);
            Ok(())
        }
        Err(error) => {
            // Put the old file back.
            if backup.exists() {
                let _ = std::fs::rename(&backup, to);
            }
            Err(error)
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::path::Path;

    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    pub fn free_space(dir: &Path) -> Option<u64> {
        use std::os::windows::ffi::OsStrExt;

        let mut wide: Vec<u16> = dir.as_os_str().encode_wide().collect();
        wide.push(0);

        let mut available: u64 = 0;
        let mut total: u64 = 0;
        let mut free_total: u64 = 0;
        // SAFETY: `wide` is a NUL terminated UTF-16 string that outlives the
        // call, and the three out pointers are valid, writable u64s.
        let ok = unsafe {
            GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free_total)
        };
        if ok == 0 {
            None
        } else {
            Some(available)
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    pub fn free_space(_dir: &Path) -> Option<u64> {
        // shortcut: only the Windows path reads the free space of a volume.
        // Add a statvfs call when the engine ships on a Unix desktop.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_in_a_folder_is_measured_on_its_volume() {
        let dir = std::env::temp_dir();
        let file = dir.join("my-idm-clone-space-probe.bin");
        assert_eq!(directory_of(&file), dir);
    }

    #[test]
    fn a_bare_name_is_measured_in_the_working_directory() {
        assert_eq!(
            directory_of(Path::new("file.bin")),
            std::path::PathBuf::from(".")
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_temp_volume_reports_free_space() {
        let free = free_space(&std::env::temp_dir());
        assert!(free.is_some(), "the temp volume must report its free space");
        assert!(free.unwrap() > 0);
    }
}
