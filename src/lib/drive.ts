/**
 * Google Drive links, as the add dialog sees them.
 *
 * A Drive page is not a file; the byte-carrying endpoints are. The dialog
 * offers an export format for an editor link, and probes the endpoint that
 * will actually be fetched, so the name, size and resumability it shows are
 * the real ones.
 */

export type DriveDocumentKind = "doc" | "sheet" | "slides" | "drawing" | "form";

export interface DriveLink {
  kind: "file" | "folder" | "document" | "unknown";
  id?: string;
  document?: DriveDocumentKind;
}

/** The shape of a Drive link, on `drive.google.com` or `docs.google.com`. */
export function parseDriveLink(url: string): DriveLink | null {
  let host: string;
  let path: string;
  let query: string;
  try {
    const parsed = new URL(url);
    host = parsed.hostname.toLowerCase();
    path = parsed.pathname;
    query = parsed.search;
  } catch {
    return null;
  }
  if (host !== "drive.google.com" && host !== "docs.google.com") {
    return null;
  }
  const file = /^\/file\/d\/([^/?#]+)/.exec(path);
  if (file) {
    return { kind: "file", id: file[1] };
  }
  if (/^\/drive\/folders\/([^/?#]+)/.test(path)) {
    return { kind: "folder", id: /^\/drive\/folders\/([^/?#]+)/.exec(path)?.[1] };
  }
  const document =
    /^\/document\/d\/([^/?#]+)/.exec(path) ||
    /^\/spreadsheets\/d\/([^/?#]+)/.exec(path) ||
    /^\/presentation\/d\/([^/?#]+)/.exec(path) ||
    /^\/drawings\/d\/([^/?#]+)/.exec(path) ||
    /^\/forms\/d\/([^/?#]+)/.exec(path);
  if (document) {
    const [, id] = document;
    const kind: DriveDocumentKind =
      path.includes("/document/") ? "doc"
      : path.includes("/spreadsheets/") ? "sheet"
      : path.includes("/presentation/") ? "slides"
      : path.includes("/drawings/") ? "drawing"
      : "form";
    return { kind: "document", id, document: kind };
  }
  if (/^\/(uc|open)\?/.test(path + query)) {
    const id = new URLSearchParams(query).get("id");
    if (id) {
      return query.includes("folder") ? { kind: "folder", id } : { kind: "file", id };
    }
  }
  return { kind: "unknown" };
}

/** The address that carries the bytes of a Drive file. */
export function driveFileEndpoint(id: string): string {
  return `https://drive.usercontent.google.com/download?id=${encodeURIComponent(id)}&export=download&confirm=t`;
}

/** The address that carries the bytes of an editor export. */
export function driveExportEndpoint(
  id: string,
  document: DriveDocumentKind,
  format: string,
): string {
  const editor = {
    doc: "document",
    sheet: "spreadsheets",
    slides: "presentation",
    drawing: "drawings",
    form: "forms",
  }[document];
  return `https://docs.google.com/${editor}/d/${encodeURIComponent(id)}/export?format=${format}`;
}

/** The exports each editor offers, in the order the picker shows them. */
export function driveExports(
  document: DriveDocumentKind,
): { value: string; label: string }[] {
  switch (document) {
    case "doc":
      return [
        { value: "pdf", label: "PDF" },
        { value: "docx", label: "Word (.docx)" },
        { value: "txt", label: "Plain text (.txt)" },
      ];
    case "sheet":
      return [
        { value: "pdf", label: "PDF" },
        { value: "csv", label: "CSV" },
        { value: "xlsx", label: "Excel (.xlsx)" },
      ];
    case "slides":
      return [
        { value: "pdf", label: "PDF" },
        { value: "pptx", label: "PowerPoint (.pptx)" },
      ];
    case "drawing":
      return [
        { value: "pdf", label: "PDF" },
        { value: "png", label: "PNG" },
      ];
    case "form":
      return [{ value: "pdf", label: "PDF" }];
  }
}
