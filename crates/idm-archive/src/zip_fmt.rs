//! Zip: every entry written by us through [`Writer`] (never
//! `ZipArchive::extract`, whose link handling was CVE-2025-29787), links left
//! out, overlapping entries (a non-recursive bomb) refused, encrypted ones
//! (ZipCrypto, WinZip AES) opened with the password.

use std::io::{Read, Seek};
use std::path::Path;

use zip::result::ZipError;

use crate::safe::Writer;
use crate::volumes::{MultiFile, VolumeKind, volume_key};
use crate::Error;

/// Unpack a zip into `writer`.
pub fn extract(archive: &Path, writer: &mut Writer, password: Option<&str>) -> Result<(), Error> {
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if volume_key(&name).is_some_and(|volume| volume.kind == VolumeKind::Split) {
        let multi = MultiFile::open(archive)?.map_err(Error::MissingVolume)?;
        run(multi, writer, password)
    } else {
        run(std::fs::File::open(archive)?, writer, password)
    }
}

/// A zip error as one of ours.
fn zip_error(error: ZipError, password: Option<&str>) -> Error {
    match error {
        ZipError::InvalidPassword => Error::WrongPassword,
        ZipError::UnsupportedArchive(message) if message == ZipError::PASSWORD_REQUIRED => {
            if password.is_some() {
                Error::WrongPassword
            } else {
                Error::NeedsPassword
            }
        }
        ZipError::Io(e) => Error::Io(e),
        other => Error::Damaged(other.to_string()),
    }
}

fn run<R: Read + Seek>(reader: R, writer: &mut Writer, password: Option<&str>) -> Result<(), Error> {
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| zip_error(e, password))?;
    refuse_overlaps(&mut zip, password)?;
    for index in 0..zip.len() {
        let mut entry = match password {
            Some(password) => zip.by_index_decrypt(index, password.as_bytes()),
            None => zip.by_index(index),
        }
        .map_err(|e| zip_error(e, password))?;
        let name = entry.name().map_err(|e| zip_error(e, password))?.into_owned();
        if entry.is_symlink() {
            writer.skip(&name, "link");
            continue;
        }
        if entry.is_dir() {
            writer.dir(&name)?;
            continue;
        }
        let size = entry.size();
        let mode = entry.unix_mode();
        let encrypted = entry.encrypted();
        match writer.file(&name, Some(size), &mut entry, mode) {
            // A wrong ZipCrypto password passes its one-byte check now and
            // then, and fails as bad data (its CRC) when read.
            Err(Error::Damaged(_)) if encrypted => return Err(Error::WrongPassword),
            other => other?,
        }
    }
    Ok(())
}

/// Two entries whose data overlap: a zip bomb built from one compressed
/// block, or a damaged archive. Refused before anything is written.
fn refuse_overlaps<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    password: Option<&str>,
) -> Result<(), Error> {
    let mut ranges = Vec::with_capacity(zip.len());
    for index in 0..zip.len() {
        let entry = match zip.by_index_raw(index) {
            Ok(entry) => entry,
            Err(e) => return Err(zip_error(e, password)),
        };
        let start = entry.header_start();
        let end = entry
            .data_start()
            .unwrap_or(start)
            .saturating_add(entry.compressed_size());
        ranges.push((start, end));
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[1].0 < pair[0].1) {
        return Err(Error::Damaged("entries overlap".into()));
    }
    Ok(())
}
