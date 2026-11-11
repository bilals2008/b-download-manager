//! What is inside an archive, read without writing anything.
//!
//! The download window shows the contents of an archive before the user
//! unpacks it, so the names have to be readable on their own: the formats
//! this crate reads in process (zip, tar, gzip, xz) are read directly, and
//! 7z and RAR are asked of 7-Zip.

use std::io::Read;
use std::path::Path;

use crate::external;
use crate::format::Format;
use crate::volumes::{volume_key, MultiFile, VolumeKind};
use crate::Error;

/// One thing inside an archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The name inside the archive, with the separators it was written with.
    pub path: String,
    /// How many bytes it holds.
    pub size: u64,
    /// Whether it is a folder.
    pub is_dir: bool,
}

/// The contents of `archive`, read without writing them.
pub fn list(archive: &Path) -> Result<Vec<Entry>, Error> {
    let format = Format::detect_file(archive)?.ok_or(Error::Unsupported)?;
    match format {
        Format::Zip => zip_entries(archive),
        Format::Tar | Format::TarGz | Format::TarXz => tar_entries(archive, format),
        Format::Gz | Format::Xz => Ok(vec![single_entry(archive, format)]),
        Format::SevenZip | Format::Rar => {
            let tool = external::find_7zip().ok_or(Error::NoTool)?;
            external::list(&tool, archive)
        }
    }
}

/// The names inside a zip, from its directory of contents.
fn zip_entries(archive: &Path) -> Result<Vec<Entry>, Error> {
    let reader = open_zip(archive)?;
    let mut zip = zip::ZipArchive::new(reader).map_err(damaged)?;
    let mut entries = Vec::with_capacity(zip.len());
    for index in 0..zip.len() {
        let entry = zip.by_index(index).map_err(damaged)?;
        let path = entry.name().map_err(damaged)?.into_owned();
        entries.push(Entry {
            path,
            size: entry.size(),
            is_dir: entry.is_dir(),
        });
    }
    Ok(entries)
}

trait ReadSeek: Read + std::io::Seek {}

impl<T: Read + std::io::Seek> ReadSeek for T {}

/// The zip's bytes, from the first volume when it is a split one.
fn open_zip(archive: &Path) -> Result<Box<dyn ReadSeek>, Error> {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if volume_key(&name).is_some_and(|volume| volume.kind == VolumeKind::Split) {
        let multi = MultiFile::open(archive)?.map_err(Error::MissingVolume)?;
        Ok(Box::new(multi))
    } else {
        Ok(Box::new(std::fs::File::open(archive)?))
    }
}

/// The names inside a tar, read through its compression.
fn tar_entries(archive: &Path, format: Format) -> Result<Vec<Entry>, Error> {
    let file = std::io::BufReader::new(std::fs::File::open(archive)?);
    let reader: Box<dyn Read> = match format {
        Format::TarGz => Box::new(flate2::read::MultiGzDecoder::new(file)),
        Format::TarXz => Box::new(lzma_rust2::XzReader::new(file, true)),
        _ => Box::new(file),
    };
    let mut tar = tar::Archive::new(reader);
    let mut entries = Vec::new();
    for entry in tar.entries().map_err(damaged)? {
        let entry = entry.map_err(damaged)?;
        let path = entry
            .path()
            .map_err(damaged)?
            .to_string_lossy()
            .into_owned();
        let kind = entry.header().entry_type();
        entries.push(Entry {
            path,
            size: entry.header().size().unwrap_or(0),
            is_dir: kind.is_dir(),
        });
    }
    Ok(entries)
}

/// The one file a gzip or an xz holds, named after the archive without its
/// compression.
fn single_entry(archive: &Path, format: Format) -> Entry {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_lowercase();
    let extension = match format {
        Format::Xz => ".xz",
        _ => ".gz",
    };
    let inner = if lower.ends_with(extension) && lower.len() > extension.len() {
        name[..name.len() - extension.len()].to_owned()
    } else {
        format!("{name}.out")
    };
    Entry {
        path: inner,
        size: 0,
        is_dir: false,
    }
}

fn damaged(e: impl std::fmt::Display) -> Error {
    Error::Damaged(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_compressed_file_is_named_after_itself() {
        let entry = single_entry(Path::new("report.csv.gz"), Format::Gz);
        assert_eq!(entry.path, "report.csv");
        assert!(!entry.is_dir);
        let xz = single_entry(Path::new("report.csv.xz"), Format::Xz);
        assert_eq!(xz.path, "report.csv");
    }
}
