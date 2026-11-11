//! Archives in several files: which file is which volume of what, and reading
//! a byte-split archive (`x.7z.001`, `.002`…) as one file.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// How an archive is split.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeKind {
    /// `x.part1.rar`, `x.part2.rar`… (RAR 3 and later).
    RarPart,
    /// `x.rar`, `x.r00`, `x.r01`… (older RAR): `x.rar` is volume 1.
    RarOld,
    /// `x.7z.001`, `x.zip.001`…: the archive cut in pieces, byte for byte.
    Split,
}

/// A file's place in a multi-volume archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    /// The name without its volume suffix and archive extension (`movie` for
    /// `movie.part2.rar`): the volumes of one archive share it.
    pub base: String,
    /// How the archive is split.
    pub kind: VolumeKind,
    /// Which volume this is, 1-based.
    pub index: u32,
}

impl Volume {
    /// Whether this is the volume the extraction starts from.
    pub fn is_first(&self) -> bool {
        self.index == 1
    }
}

/// The volume `name` is, if it is one (case does not matter). A lone `x.rar`
/// is volume 1 of an old-style set, which may have no other volume.
pub fn volume_key(name: &str) -> Option<Volume> {
    let lower = name.to_lowercase();
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    // x.partN.rar
    if let Some(stem) = lower.strip_suffix(".rar") {
        if let Some(index) = stem.rfind(".part") {
            if digits(&stem[index + 5..]) {
                return Some(Volume {
                    base: name[..index].to_owned(),
                    kind: VolumeKind::RarPart,
                    index: stem[index + 5..].parse().ok()?,
                });
            }
        }
    }
    // x.rar, x.r00 → 1, 2
    if let Some(stem) = lower.strip_suffix(".rar") {
        return Some(Volume {
            base: name[..stem.len()].to_owned(),
            kind: VolumeKind::RarOld,
            index: 1,
        });
    }
    if let Some(index) = lower.rfind(".r") {
        if lower.len() - index == 4 && digits(&lower[index + 2..]) {
            return Some(Volume {
                base: name[..index].to_owned(),
                kind: VolumeKind::RarOld,
                index: lower[index + 2..].parse::<u32>().ok()? + 2,
            });
        }
    }
    // x.7z.001, x.zip.001
    for extension in [".7z.", ".zip."] {
        if let Some(index) = lower.rfind(extension) {
            let number = &lower[index + extension.len()..];
            if number.len() == 3 && digits(number) {
                return Some(Volume {
                    base: name[..index].to_owned(),
                    kind: VolumeKind::Split,
                    index: number.parse().ok()?,
                });
            }
        }
    }
    None
}

/// A byte-split archive's parts, read as one file: `first` is `x.7z.001`;
/// `.002`, `.003`… follow as long as they exist.
#[derive(Debug)]
pub struct MultiFile {
    parts: Vec<(File, u64)>,
    total: u64,
    pos: u64,
}

impl MultiFile {
    /// The parts of `first` and the one that is missing, if any.
    pub fn open(first: &Path) -> io::Result<Result<MultiFile, u32>> {
        let mut parts = Vec::new();
        let mut total = 0u64;
        for (number, path) in split_parts(first).iter().enumerate() {
            match File::open(path) {
                Ok(file) => {
                    let length = file.metadata()?.len();
                    total += length;
                    parts.push((file, length));
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    return Ok(Err(number as u32 + 1));
                }
                Err(e) => return Err(e),
            }
        }
        Ok(Ok(MultiFile {
            parts,
            total,
            pos: 0,
        }))
    }
}

/// `x.7z.001`'s parts on disk: `.001`, `.002`… while they exist, and any later
/// one found after a gap (so a missing middle volume is reported, not taken
/// for the end).
pub fn split_parts(first: &Path) -> Vec<PathBuf> {
    let name = first
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(stem) = name.get(..name.len().saturating_sub(3)) else {
        return vec![first.to_path_buf()];
    };
    let dir = first.parent().unwrap_or(Path::new(""));
    let path = |number: u32| dir.join(format!("{stem}{number:03}"));
    let mut last = 1;
    // The highest part there (up to 999), a gap or not.
    for number in 2..=999 {
        if path(number).exists() {
            last = number;
        } else if number > last + 10 {
            break;
        }
    }
    (1..=last).map(path).collect()
}

impl Read for MultiFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut start = 0u64;
        for (file, length) in &mut self.parts {
            if self.pos < start + *length {
                file.seek(SeekFrom::Start(self.pos - start))?;
                let room = (*length - (self.pos - start)) as usize;
                let take = buf.len().min(room);
                let read = file.read(&mut buf[..take])?;
                self.pos += read as u64;
                return Ok(read);
            }
            start += *length;
        }
        Ok(0)
    }
}

impl Seek for MultiFile {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let pos = match to {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::End(delta) => self.total as i128 + delta as i128,
            SeekFrom::Current(delta) => self.pos as i128 + delta as i128,
        };
        if pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }
        self.pos = pos as u64;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_names() {
        let volume = volume_key("Movie.2024.part02.rar").unwrap();
        assert_eq!(
            (volume.base.as_str(), volume.kind, volume.index),
            ("Movie.2024", VolumeKind::RarPart, 2)
        );
        assert_eq!(volume_key("a.PART1.RAR").unwrap().index, 1);
        assert!(volume_key("a.PART1.RAR").unwrap().is_first());
        let old = volume_key("a.rar").unwrap();
        assert_eq!((old.kind, old.index), (VolumeKind::RarOld, 1));
        assert_eq!(volume_key("a.r00").unwrap().index, 2);
        assert_eq!(volume_key("a.r01").unwrap().index, 3);
        let split = volume_key("pics.7z.003").unwrap();
        assert_eq!(
            (split.base.as_str(), split.kind, split.index),
            ("pics", VolumeKind::Split, 3)
        );
        assert!(volume_key("a.zip").is_none());
        assert!(volume_key("report.pdf").is_none());
    }

    #[test]
    fn the_parts_of_a_split_archive_are_found() {
        let folder = std::env::temp_dir().join("idm-archive-volumes-test");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder");
        let first = folder.join("pics.7z.001");
        std::fs::write(&first, b"one").expect("a file");
        std::fs::write(folder.join("pics.7z.002"), b"two").expect("a file");

        let parts = split_parts(&first);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].file_name().unwrap(), "pics.7z.002");

        let missing = MultiFile::open(&first).expect("the parts");
        assert!(missing.is_ok());

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_missing_volume_is_named() {
        let folder = std::env::temp_dir().join("idm-archive-missing-volume-test");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder");
        let first = folder.join("pics.7z.001");
        std::fs::write(&first, b"one").expect("a file");
        std::fs::write(folder.join("pics.7z.003"), b"three").expect("a file");

        let missing = MultiFile::open(&first).expect("the parts");
        assert_eq!(missing.unwrap_err(), 2);

        let _ = std::fs::remove_dir_all(&folder);
    }
}
