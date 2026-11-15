//! Google Drive and Google Docs: the addresses, the names and the refusals.
//!
//! A Drive link is not the address of a file. A shared file goes through a
//! confirmation page, a Google Doc has no bytes of its own at all, and a
//! folder is a list of files. This module recognises those shapes, says which
//! address to fetch and what the result should be called; the engine's own
//! client does the fetching, and nothing here touches the network.

use reqwest::Url;
use serde::{Deserialize, Serialize};

const DRIVE_HOST: &str = "drive.google.com";
const DOCS_HOST: &str = "docs.google.com";

/// What a Drive link points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DriveLink {
    /// A file with bytes behind it. `name` is there only when the link
    /// itself carried one; the real name comes from the response.
    File { id: String, name: Option<String> },
    /// A folder: a list of files, not one download.
    Folder { id: String },
    /// A document that exists only as an export.
    Document { id: String, kind: DocumentKind },
    /// Not a Drive link, or a shape this module does not know.
    Unknown,
}

/// Which Google editor holds a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentKind {
    Doc,
    Sheet,
    Slides,
    Drawing,
    Form,
}

impl DocumentKind {
    /// The path segment the editor uses in its own addresses.
    fn path(self) -> &'static str {
        match self {
            DocumentKind::Doc => "document",
            DocumentKind::Sheet => "spreadsheets",
            DocumentKind::Slides => "presentation",
            DocumentKind::Drawing => "drawings",
            DocumentKind::Form => "forms",
        }
    }
}

/// The formats a document can be exported as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Pdf,
    Docx,
    Csv,
    Xlsx,
    Txt,
    Pptx,
    Png,
}

impl ExportFormat {
    /// The extension the exported file has.
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "pdf",
            ExportFormat::Docx => "docx",
            ExportFormat::Csv => "csv",
            ExportFormat::Xlsx => "xlsx",
            ExportFormat::Txt => "txt",
            ExportFormat::Pptx => "pptx",
            ExportFormat::Png => "png",
        }
    }

    /// The name a person reads in the list of formats.
    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "PDF",
            ExportFormat::Docx => "Word (.docx)",
            ExportFormat::Csv => "CSV (.csv)",
            ExportFormat::Xlsx => "Excel (.xlsx)",
            ExportFormat::Txt => "Plain text (.txt)",
            ExportFormat::Pptx => "PowerPoint (.pptx)",
            ExportFormat::Png => "PNG image (.png)",
        }
    }
}

/// The formats one document type can be exported as.
pub fn exports_for(kind: DocumentKind) -> Vec<ExportFormat> {
    match kind {
        DocumentKind::Doc => vec![ExportFormat::Pdf, ExportFormat::Docx, ExportFormat::Txt],
        DocumentKind::Sheet => vec![ExportFormat::Pdf, ExportFormat::Csv, ExportFormat::Xlsx],
        DocumentKind::Slides => vec![ExportFormat::Pdf, ExportFormat::Pptx],
        DocumentKind::Drawing => vec![ExportFormat::Pdf, ExportFormat::Png],
        DocumentKind::Form => vec![ExportFormat::Pdf],
    }
}

/// What a Drive link points at.
///
/// Everything that is not on `drive.google.com` or `docs.google.com`, and
/// every shape on those hosts that this module does not know, is
/// [`DriveLink::Unknown`]: the engine then downloads the address as it came.
pub fn parse_link(url: &str) -> DriveLink {
    let Ok(parsed) = Url::parse(url) else {
        return DriveLink::Unknown;
    };
    let Some(host) = parsed.host_str() else {
        return DriveLink::Unknown;
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host != DRIVE_HOST && host != DOCS_HOST {
        return DriveLink::Unknown;
    }

    let segments: Vec<&str> = parsed
        .path()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    let query = |key: &str| {
        pairs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    };

    // A folder names itself in the path, or says so in the query.
    if let Some(index) = segments.iter().position(|segment| *segment == "folders") {
        if let Some(id) = segments.get(index + 1) {
            return DriveLink::Folder {
                id: (*id).to_string(),
            };
        }
    }
    if let Some(id) = query("folderid") {
        return DriveLink::Folder { id };
    }
    // A query that carries the id and says the link is a folder.
    let folder_hint = query("folder").is_some()
        || query("folderid").is_some()
        || segments.iter().any(|segment| segment.starts_with("folder"));
    if folder_hint {
        if let Some(id) = query("id") {
            return DriveLink::Folder { id };
        }
    }

    // The editors: the path says which one, then `d`, then the id.
    for (segment, kind) in [
        ("document", DocumentKind::Doc),
        ("spreadsheets", DocumentKind::Sheet),
        ("presentation", DocumentKind::Slides),
        ("drawings", DocumentKind::Drawing),
        ("forms", DocumentKind::Form),
    ] {
        if let Some(id) = editor_id(&segments, segment) {
            return DriveLink::Document {
                id: id.to_string(),
                kind,
            };
        }
    }

    // A file: `/file/d/<id>/view`, or `open?id=` and `uc?id=` in the query.
    if let Some(id) = editor_id(&segments, "file") {
        return DriveLink::File {
            id: id.to_string(),
            name: named_in_query(&pairs),
        };
    }
    if let Some(id) = query("id") {
        return DriveLink::File {
            id,
            name: named_in_query(&pairs),
        };
    }

    DriveLink::Unknown
}

/// The id behind an editor segment, when the path has the shape
/// `<segment>/d/<id>`.
fn editor_id<'a>(segments: &[&'a str], segment: &str) -> Option<&'a str> {
    let index = segments.iter().position(|part| *part == segment)?;
    if segments.get(index + 1) != Some(&"d") {
        return None;
    }
    segments.get(index + 2).copied()
}

/// The name a link carried in its query, when it carried one.
fn named_in_query(pairs: &[(String, String)]) -> Option<String> {
    for key in ["name", "filename", "title"] {
        if let Some((_, value)) = pairs.iter().find(|(name, _)| name == key) {
            if !value.trim().is_empty() {
                return Some(value.clone());
            }
        }
    }
    None
}

/// The address the bytes of a Drive file come from.
///
/// `confirm=t` is what tells Drive the visitor already saw the warning page;
/// a file over the size threshold still answers with the confirmation page,
/// and the engine follows that form the way a browser does.
pub fn download_endpoint(id: &str) -> String {
    format!("https://drive.usercontent.google.com/download?id={id}&export=download&confirm=t")
}

/// The address an editor document is exported from.
pub fn export_endpoint(id: &str, kind: DocumentKind, format: ExportFormat) -> String {
    format!(
        "https://{DOCS_HOST}/{}/d/{id}/export?format={}",
        kind.path(),
        format.extension()
    )
}

/// The name the download is saved under.
///
/// The title the response gave is used when there is one; otherwise the id
/// is, because a file with a wrong name is still a file. The extension comes
/// from the endpoint or from the response, and is never added twice.
pub fn file_name(id: &str, title: Option<&str>, extension: &str) -> String {
    let base = title
        .and_then(sanitize)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| id.to_string());
    let extension = extension.trim().trim_start_matches('.');
    if extension.is_empty() {
        return base;
    }
    if base
        .to_ascii_lowercase()
        .ends_with(&format!(".{}", extension.to_ascii_lowercase()))
    {
        return base;
    }
    format!("{base}.{extension}")
}

/// The name a response carried, from the `Content-Disposition` header or
/// from the title of an HTML page.
///
/// The name is cleaned of separators and of `..`, so a hostile header cannot
/// write outside the folder the user chose.
pub fn name_from_response(body: &str, content_disposition: Option<&str>) -> Option<String> {
    if let Some(header) = content_disposition {
        if let Some(name) = crate::http::file_name_from_disposition(header) {
            if let Some(clean) = sanitize(&name) {
                return Some(clean);
            }
        }
    }
    let title = title_of(body)?;
    sanitize(&strip_browser_suffix(&title))
}

/// Whether the body is Drive's page for a file whose owner is over their
/// quota.
///
/// The refusal has to say that, because "the download failed" sends the
/// person looking for a network problem that is not there.
pub fn quota_exceeded(body: &str) -> bool {
    let lower = body.to_lowercase();
    lower.contains("quota exceeded")
        || lower.contains("exceeded their quota")
        || lower.contains("exceeded your quota")
        || (lower.contains("quota") && lower.contains("exceed"))
}

/// Whether the body is a sign-in page.
///
/// A private file must be reported as such; downloading the HTML of the
/// sign-in page would save a page of Google instead of the file.
pub fn needs_sign_in(body: &str) -> bool {
    let lower = body.to_lowercase();
    [
        "accounts.google.com/servicelogin",
        "sign in to continue",
        "you need to sign in",
        "please sign in",
        "request access",
        "need access to this file",
        "you don't have access",
        "access denied",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// The title of an HTML page.
fn title_of(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open = body[start..].find('>')? + start + 1;
    let end = lower[open..].find("</title>")? + open;
    let title = body[open..end].trim();
    if title.is_empty() {
        return None;
    }
    Some(decode_entities(title))
}

/// The text of a page title without the name of the app that appended it.
fn strip_browser_suffix(title: &str) -> String {
    let mut out = title.trim();
    for suffix in [
        " - Google Docs",
        " - Google Sheets",
        " - Google Slides",
        " - Google Forms",
        " - Google Drawings",
        " - Google Drive",
    ] {
        if let Some(stripped) = out.strip_suffix(suffix) {
            out = stripped.trim();
        }
    }
    out.to_string()
}

/// The few entities a page title usually carries.
fn decode_entities(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// A name that is safe to write on disk.
///
/// The characters Windows refuses are turned into spaces, and every `..` is
/// removed, so no name can climb out of the folder it is saved in.
fn sanitize(value: &str) -> Option<String> {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => out.push(' '),
            control if (control as u32) < 0x20 => {}
            other => out.push(other),
        }
    }
    let without_climbing = out.replace("..", "");
    let cleaned = without_climbing
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches('.')
        .to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_id(link: &DriveLink) -> Option<&str> {
        match link {
            DriveLink::File { id, .. } => Some(id),
            _ => None,
        }
    }

    #[test]
    fn a_file_link_is_recognised() {
        assert_eq!(
            parse_link("https://drive.google.com/file/d/1AbC-dEf/view"),
            DriveLink::File {
                id: "1AbC-dEf".to_string(),
                name: None
            }
        );
        assert_eq!(
            file_id(&parse_link("https://drive.google.com/open?id=1AbC-dEf")),
            Some("1AbC-dEf")
        );
        assert_eq!(
            file_id(&parse_link(
                "https://drive.google.com/uc?id=1AbC-dEf&export=download"
            )),
            Some("1AbC-dEf")
        );
    }

    #[test]
    fn a_link_that_carries_a_name_keeps_it() {
        let link = parse_link("https://drive.google.com/open?id=1AbC-dEf&name=Budget.xlsx");
        assert_eq!(
            link,
            DriveLink::File {
                id: "1AbC-dEf".to_string(),
                name: Some("Budget.xlsx".to_string())
            }
        );
    }

    #[test]
    fn every_editor_is_recognised() {
        for (url, kind) in [
            (
                "https://docs.google.com/document/d/1AbC-dEf/edit",
                DocumentKind::Doc,
            ),
            (
                "https://docs.google.com/spreadsheets/d/1AbC-dEf/edit#gid=0",
                DocumentKind::Sheet,
            ),
            (
                "https://docs.google.com/presentation/d/1AbC-dEf/edit",
                DocumentKind::Slides,
            ),
            (
                "https://docs.google.com/drawings/d/1AbC-dEf/edit",
                DocumentKind::Drawing,
            ),
            (
                "https://docs.google.com/forms/d/1AbC-dEf/viewform",
                DocumentKind::Form,
            ),
        ] {
            assert_eq!(
                parse_link(url),
                DriveLink::Document {
                    id: "1AbC-dEf".to_string(),
                    kind
                },
                "{url}"
            );
        }
    }

    #[test]
    fn a_folder_is_recognised() {
        assert_eq!(
            parse_link("https://drive.google.com/drive/folders/1AbC-dEf"),
            DriveLink::Folder {
                id: "1AbC-dEf".to_string()
            }
        );
        assert_eq!(
            parse_link("https://drive.google.com/drive/u/0/folders/1AbC-dEf"),
            DriveLink::Folder {
                id: "1AbC-dEf".to_string()
            }
        );
        assert_eq!(
            parse_link("https://drive.google.com/open?id=1AbC-dEf&folder=1"),
            DriveLink::Folder {
                id: "1AbC-dEf".to_string()
            }
        );
        assert_eq!(
            parse_link("https://drive.google.com/drive/folders/?id=1AbC-dEf"),
            DriveLink::Folder {
                id: "1AbC-dEf".to_string()
            }
        );
    }

    #[test]
    fn another_host_is_not_a_drive_link() {
        assert_eq!(
            parse_link("https://example.com/file/d/1AbC-dEf/view"),
            DriveLink::Unknown
        );
        assert_eq!(
            parse_link("https://evil.example/open?id=1AbC-dEf"),
            DriveLink::Unknown
        );
        assert_eq!(parse_link("not a url at all"), DriveLink::Unknown);
        assert_eq!(parse_link("https://drive.google.com/"), DriveLink::Unknown);
    }

    #[test]
    fn the_download_address_is_the_one_drive_serves() {
        assert_eq!(
            download_endpoint("1AbC-dEf"),
            "https://drive.usercontent.google.com/download?id=1AbC-dEf&export=download&confirm=t"
        );
    }

    #[test]
    fn every_document_type_offers_exactly_its_formats() {
        assert_eq!(
            exports_for(DocumentKind::Doc),
            vec![ExportFormat::Pdf, ExportFormat::Docx, ExportFormat::Txt]
        );
        assert_eq!(
            exports_for(DocumentKind::Sheet),
            vec![ExportFormat::Pdf, ExportFormat::Csv, ExportFormat::Xlsx]
        );
        assert_eq!(
            exports_for(DocumentKind::Slides),
            vec![ExportFormat::Pdf, ExportFormat::Pptx]
        );
        assert_eq!(
            exports_for(DocumentKind::Drawing),
            vec![ExportFormat::Pdf, ExportFormat::Png]
        );
        assert_eq!(exports_for(DocumentKind::Form), vec![ExportFormat::Pdf]);
    }

    #[test]
    fn a_format_a_document_cannot_be_exported_as_is_not_offered() {
        assert!(!exports_for(DocumentKind::Doc).contains(&ExportFormat::Png));
        assert!(!exports_for(DocumentKind::Sheet).contains(&ExportFormat::Docx));
        assert!(!exports_for(DocumentKind::Slides).contains(&ExportFormat::Csv));
        assert!(!exports_for(DocumentKind::Form).contains(&ExportFormat::Docx));
    }

    #[test]
    fn the_export_address_names_the_editor_and_the_format() {
        assert_eq!(
            export_endpoint("1AbC", DocumentKind::Doc, ExportFormat::Pdf),
            "https://docs.google.com/document/d/1AbC/export?format=pdf"
        );
        assert_eq!(
            export_endpoint("1AbC", DocumentKind::Sheet, ExportFormat::Xlsx),
            "https://docs.google.com/spreadsheets/d/1AbC/export?format=xlsx"
        );
        assert_eq!(
            export_endpoint("1AbC", DocumentKind::Slides, ExportFormat::Pptx),
            "https://docs.google.com/presentation/d/1AbC/export?format=pptx"
        );
        assert_eq!(
            export_endpoint("1AbC", DocumentKind::Drawing, ExportFormat::Png),
            "https://docs.google.com/drawings/d/1AbC/export?format=png"
        );
        assert_eq!(
            export_endpoint("1AbC", DocumentKind::Form, ExportFormat::Pdf),
            "https://docs.google.com/forms/d/1AbC/export?format=pdf"
        );
    }

    #[test]
    fn a_format_knows_its_extension_and_its_name() {
        assert_eq!(ExportFormat::Pdf.extension(), "pdf");
        assert_eq!(ExportFormat::Docx.extension(), "docx");
        assert_eq!(ExportFormat::Xlsx.extension(), "xlsx");
        assert_eq!(ExportFormat::Png.extension(), "png");
        assert_eq!(ExportFormat::Pdf.label(), "PDF");
        assert_eq!(ExportFormat::Docx.label(), "Word (.docx)");
    }

    #[test]
    fn a_named_file_is_saved_under_its_own_name() {
        assert_eq!(
            file_name("1AbC", Some("Budget 2024"), "pdf"),
            "Budget 2024.pdf"
        );
        assert_eq!(file_name("1AbC", None, "pdf"), "1AbC.pdf");
    }

    #[test]
    fn a_name_with_characters_windows_refuses_is_cleaned() {
        let name = file_name("1AbC", Some("Report: 2024/Q4"), "pdf");
        assert_eq!(name, "Report 2024 Q4.pdf");
        assert!(!name.contains(':'));
        assert!(!name.contains('/'));
    }

    #[test]
    fn the_extension_is_never_added_twice() {
        assert_eq!(
            file_name("1AbC", Some("Budget 2024.pdf"), "pdf"),
            "Budget 2024.pdf"
        );
    }

    #[test]
    fn a_name_is_read_out_of_the_disposition_header() {
        assert_eq!(
            name_from_response("", Some("attachment; filename=\"Budget 2024.pdf\"")),
            Some("Budget 2024.pdf".to_string())
        );
    }

    #[test]
    fn a_name_is_read_out_of_the_page_title() {
        assert_eq!(
            name_from_response(
                "<html><head><title>Budget 2024 - Google Sheets</title></head></html>",
                None
            ),
            Some("Budget 2024".to_string())
        );
    }

    #[test]
    fn a_response_without_a_name_has_none() {
        assert_eq!(name_from_response("", None), None);
        assert_eq!(
            name_from_response("<html><body>nothing</body></html>", None),
            None
        );
        assert_eq!(name_from_response("", Some("inline")), None);
    }

    #[test]
    fn a_name_that_tries_to_escape_the_folder_is_cleaned() {
        let name = name_from_response("", Some("attachment; filename=\"../../evil.exe\""))
            .expect("a name");
        assert_eq!(name, "evil.exe");
        assert!(!name.contains(".."));
        assert!(!name.contains('/'));
        assert!(!name.contains('\\'));
    }

    #[test]
    fn the_quota_page_is_recognised() {
        assert!(quota_exceeded(
            "Sorry, you can't view or download this file at this time because the owner's storage quota is exceeded."
        ));
        assert!(quota_exceeded("QUOTA EXCEEDED"));
        assert!(!quota_exceeded("Everything is fine."));
    }

    #[test]
    fn the_sign_in_page_is_recognised() {
        assert!(needs_sign_in(
            "<a href=\"https://accounts.google.com/ServiceLogin\">Sign in</a>"
        ));
        assert!(needs_sign_in(
            "Sorry, you need access to this file. Request access."
        ));
        assert!(needs_sign_in("You don't have access to this file."));
        assert!(!needs_sign_in("PK\u{3}\u{4}the bytes of a file"));
    }
}
