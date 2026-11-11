//! One file compressed with gzip or xz: `report.csv.gz` gives `report.csv`.

use std::io::Read;
use std::path::Path;

use crate::format::Format;
use crate::safe::Writer;

/// Unpack a single compressed file into `writer`.
pub fn extract(archive: &Path, format: Format, writer: &mut Writer) -> Result<(), crate::Error> {
    let file = std::io::BufReader::new(std::fs::File::open(archive)?);
    let name = archive
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_lowercase();
    let inner = [".gz", ".xz"]
        .iter()
        .find(|extension| lower.ends_with(*extension) && lower.len() > extension.len())
        .map(|extension| name[..name.len() - extension.len()].to_owned())
        .unwrap_or_else(|| format!("{name}.out"));
    let mut reader: Box<dyn Read> = match format {
        Format::Xz => Box::new(lzma_rust2::XzReader::new(file, true)),
        _ => Box::new(flate2::read::MultiGzDecoder::new(file)),
    };
    writer.file(&inner, None, &mut reader, None)
}
