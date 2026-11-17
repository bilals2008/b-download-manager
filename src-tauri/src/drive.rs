//! Google Drive links: the address that actually carries the bytes.
//!
//! A `drive.google.com` link is not a file: it is a page that shows a file.
//! The engine fetches the endpoints that carry the bytes instead — the
//! download endpoint for a file the reader can see, the export endpoint for
//! an editor document. A private file is refused with a sentence, never
//! downloaded as an HTML page; a shared folder is refused too, because
//! opening it is another feature, not a download.

use idm_core::sites::gdrive::{self, DriveLink, ExportFormat};

/// The address the engine fetches instead of the Drive page.
///
/// `Document` links are offered as an export, the way a browser does: the
/// format the reader picked, or a PDF. `File` links are fetched through the
/// direct endpoint. `Unknown` is left as it was; `Folder` is refused with a
/// sentence, never downloaded as a page.
pub(crate) fn resolve(url: &str, export: Option<&str>) -> Result<String, String> {
    match gdrive::parse_link(url) {
        DriveLink::Unknown => Ok(url.to_string()),
        DriveLink::File { id, .. } => Ok(gdrive::download_endpoint(&id)),
        DriveLink::Document { id, kind } => {
            Ok(gdrive::export_endpoint(&id, kind, export_format(export)))
        }
        DriveLink::Folder { .. } => Err(
            "this build cannot open a shared Drive folder yet: pick a file link instead"
                .to_string(),
        ),
    }
}

/// The export format the reader picked, with a plain PDF as the default for
/// anything this build does not know.
fn export_format(export: Option<&str>) -> ExportFormat {
    match export
        .map(|text| text.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("docx") => ExportFormat::Docx,
        Some("csv") => ExportFormat::Csv,
        Some("xlsx") => ExportFormat::Xlsx,
        Some("txt") => ExportFormat::Txt,
        Some("pptx") => ExportFormat::Pptx,
        Some("png") => ExportFormat::Png,
        _ => ExportFormat::Pdf,
    }
}
