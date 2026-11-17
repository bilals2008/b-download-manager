//! Unpacking an archive, whichever format it is.
//!
//! Everything is written into an empty staging folder inside the destination
//! and moved into place afterwards, so a failure leaves nothing behind but
//! the archive itself.

use std::path::Path;

use crate::external;
use crate::format::Format;
use crate::safe::{Staging, Writer};
use crate::volumes::{volume_key, MultiFile, VolumeKind};
use crate::{Error, Policy, Report};

/// Unpack `archive` as `policy` says.
pub fn run(archive: &Path, policy: &mut Policy<'_>) -> Result<Report, Error> {
    let format = Format::detect_file(archive)?.ok_or(Error::Unsupported)?;
    let staging = Staging::new(&policy.dest)?;
    let mut writer = Writer::new(staging.path(), policy.limits, policy.cancel);
    match format {
        Format::Zip => crate::zip_fmt::extract(archive, &mut writer, policy.password.as_deref())?,
        Format::Tar | Format::TarGz | Format::TarXz => {
            crate::tar_fmt::extract(archive, format, &mut writer)?
        }
        Format::Gz | Format::Xz => crate::single::extract(archive, format, &mut writer)?,
        Format::SevenZip | Format::Rar => {
            let tool = policy
                .tool
                .clone()
                .or_else(external::find_7zip)
                .ok_or(Error::NoTool)?;
            // A split 7z's volumes are all there before 7-Zip is asked (it
            // would only say "unexpected end"): the first one missing is
            // named.
            let name = archive
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if volume_key(&name).is_some_and(|volume| volume.kind == VolumeKind::Split) {
                MultiFile::open(archive)?.map_err(Error::MissingVolume)?;
            }
            external::extract(&tool, archive, &mut writer, policy.password.as_deref())?
        }
    }
    let skipped = writer.take_skipped();
    let refused = writer.refused();
    let (mut report, placed) =
        staging.place(&crate::base_name(archive), policy.subfolder, policy.on_file)?;
    report.skipped = skipped;
    report.refused = refused;
    report.files = placed;
    report.bytes = writer.bytes();
    Ok(report)
}

/// Whether `archive` is one this crate reads.
pub fn is_archive(archive: &Path) -> bool {
    matches!(
        Format::detect_file(archive),
        Ok(Some(
            Format::Zip
                | Format::Tar
                | Format::TarGz
                | Format::TarXz
                | Format::Gz
                | Format::Xz
                | Format::SevenZip
                | Format::Rar
        ))
    )
}
