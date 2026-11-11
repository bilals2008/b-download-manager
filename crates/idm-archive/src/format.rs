//! What an archive is, by its name and by the bytes it starts with.
//!
//! The name is what the download was called, the bytes are what was actually
//! downloaded: a `.zip` that turned out to be a `.tar.gz` is still an archive,
//! and it has to be unpacked as what it is.

use std::io::Read;
use std::path::Path;

use crate::volumes::{volume_key, VolumeKind};

/// The archive formats this crate reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// A zip, with Deflate, Deflate64 or WinZip AES inside.
    Zip,
    /// A tar.
    Tar,
    /// A tar in gzip.
    TarGz,
    /// A tar in xz.
    TarXz,
    /// One file in gzip (not a tar).
    Gz,
    /// One file in xz (not a tar).
    Xz,
    /// A 7z, handed to 7-Zip.
    SevenZip,
    /// A RAR, handed to 7-Zip.
    Rar,
}

impl Format {
    /// The format an archive with this name is, when the name says.
    ///
    /// The name of a split archive is the name of its first volume
    /// (`x.7z.001`), so the volume suffix is looked through.
    pub fn detect(name: &str) -> Option<Format> {
        let lower = name.to_ascii_lowercase();
        if let Some(volume) = volume_key(&lower) {
            return match volume.kind {
                VolumeKind::Split => lower
                    .rsplit_once('.')
                    .and_then(|(archive, _)| Self::detect(archive)),
                VolumeKind::RarPart | VolumeKind::RarOld => Some(Format::Rar),
            };
        }
        if lower.ends_with(".zip") || lower.ends_with(".zipx") {
            return Some(Format::Zip);
        }
        if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            return Some(Format::TarGz);
        }
        if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
            return Some(Format::TarXz);
        }
        if lower.ends_with(".tar") {
            return Some(Format::Tar);
        }
        if lower.ends_with(".gz") {
            return Some(Format::Gz);
        }
        if lower.ends_with(".xz") {
            return Some(Format::Xz);
        }
        if lower.ends_with(".7z") {
            return Some(Format::SevenZip);
        }
        if lower.ends_with(".rar") {
            return Some(Format::Rar);
        }
        None
    }

    /// The format of the archive at `path`, by the bytes it starts with.
    ///
    /// The name still decides between a gzip that holds a tar and a gzip that
    /// holds one file: both start the same way.
    pub fn detect_file(path: &Path) -> std::io::Result<Option<Format>> {
        let mut head = [0u8; 512];
        let mut file = std::fs::File::open(path)?;
        let mut read = 0;
        while read < head.len() {
            let got = file.read(&mut head[read..])?;
            if got == 0 {
                break;
            }
            read += got;
        }
        let head = &head[..read];
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        Ok(Self::from_head(head, &name))
    }

    /// The format the first bytes of a file stand for.
    fn from_head(head: &[u8], name: &str) -> Option<Format> {
        let tar_named = |exts: &[&str]| exts.iter().any(|e| name.ends_with(e));
        if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
            Some(Format::Zip)
        } else if head.starts_with(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C]) {
            Some(Format::SevenZip)
        } else if head.starts_with(b"Rar!\x1A\x07") {
            Some(Format::Rar)
        } else if head.starts_with(&[0x1F, 0x8B]) {
            Some(if tar_named(&[".tar.gz", ".tgz"]) {
                Format::TarGz
            } else {
                Format::Gz
            })
        } else if head.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
            Some(if tar_named(&[".tar.xz", ".txz"]) {
                Format::TarXz
            } else {
                Format::Xz
            })
        } else if head.len() >= 262 && &head[257..262] == b"ustar" {
            Some(Format::Tar)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_says_what_an_archive_is() {
        assert_eq!(Format::detect("film.zip"), Some(Format::Zip));
        assert_eq!(Format::detect("film.TAR.GZ"), Some(Format::TarGz));
        assert_eq!(Format::detect("film.tgz"), Some(Format::TarGz));
        assert_eq!(Format::detect("film.tar.xz"), Some(Format::TarXz));
        assert_eq!(Format::detect("film.tar"), Some(Format::Tar));
        assert_eq!(Format::detect("report.csv.gz"), Some(Format::Gz));
        assert_eq!(Format::detect("report.csv.xz"), Some(Format::Xz));
        assert_eq!(Format::detect("film.7z"), Some(Format::SevenZip));
        assert_eq!(Format::detect("film.rar"), Some(Format::Rar));
        assert_eq!(Format::detect("film.mp4"), None);
        assert_eq!(Format::detect(""), None);
    }

    #[test]
    fn a_split_archive_is_named_after_its_first_volume() {
        assert_eq!(Format::detect("pics.7z.001"), Some(Format::SevenZip));
        assert_eq!(Format::detect("pics.zip.001"), Some(Format::Zip));
        assert_eq!(Format::detect("pics.zip.7Z.002"), Some(Format::SevenZip));
        assert_eq!(Format::detect("pics.7z.ZIP.003"), Some(Format::Zip));
        assert_eq!(Format::detect("movie.part2.rar"), Some(Format::Rar));
    }

    #[test]
    fn the_first_bytes_say_what_the_name_does_not() {
        assert_eq!(
            Format::from_head(b"PK\x03\x04rest", "film.zip"),
            Some(Format::Zip)
        );
        assert_eq!(
            Format::from_head(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C, 0, 4], "x.7z"),
            Some(Format::SevenZip)
        );
        assert_eq!(
            Format::from_head(b"Rar!\x1A\x07\x01\x00", "x.rar"),
            Some(Format::Rar)
        );
        assert_eq!(
            Format::from_head(&[0x1F, 0x8B, 0x08], "pack.tar.gz"),
            Some(Format::TarGz)
        );
        assert_eq!(
            Format::from_head(&[0x1F, 0x8B, 0x08], "notes.gz"),
            Some(Format::Gz)
        );
        assert_eq!(
            Format::from_head(&[0xFD, b'7', b'z', b'X', b'Z', 0x00], "pack.tar.xz"),
            Some(Format::TarXz)
        );
        assert_eq!(
            Format::from_head(&[0xFD, b'7', b'z', b'X', b'Z', 0x00], "notes.xz"),
            Some(Format::Xz)
        );
        assert_eq!(Format::from_head(b"just text", "notes.zip"), None);
    }

    #[test]
    fn a_tar_is_recognised_by_its_magic() {
        let mut head = [0u8; 512];
        head[257..262].copy_from_slice(b"ustar");
        assert_eq!(Format::from_head(&head, "pack.tar"), Some(Format::Tar));
    }
}
