/** File families used by the reusable file-type icon artwork. */
export type FileKind =
  | "video"
  | "audio"
  | "archive"
  | "application"
  | "system"
  | "pdf"
  | "document"
  | "spreadsheet"
  | "slide"
  | "image"
  | "code"
  | "font"
  | "json"
  | "xml"
  | "config"
  | "csv"
  | "torrent"
  | "disk";

/** One row per family: extensions are lowercase and omit the leading dot. */
const EXTENSIONS: [FileKind, string][] = [
  ["video", "mp4 mkv avi mov webm wmv flv m4v mpg mpeg 3gp ogv"],
  ["audio", "mp3 wav flac m4a aac ogg opus wma aiff mid midi"],
  ["archive", "zip rar 7z tar gz tgz bz2 xz cab zst lz lz4"],
  ["application", "exe msi msix appx app dmg pkg deb rpm apk appimage jar bat cmd sh ps1"],
  ["system", "dll sys so dylib bin wasm o"],
  ["pdf", "pdf"],
  ["document", "txt md markdown rtf doc docx odt epub log nfo pages"],
  ["spreadsheet", "xls xlsx xlsm ods numbers"],
  ["slide", "ppt pptx odp key"],
  ["image", "png jpg jpeg gif webp bmp svg ico tif tiff avif heic psd ai raw"],
  ["code", "js jsx ts tsx mjs cjs css scss less html htm py rs go java c cpp h hpp cs php rb swift kt sql vue svelte astro"],
  ["font", "ttf otf woff woff2 eot"],
  ["json", "json jsonc json5"],
  ["xml", "xml xsl xslt svgx"],
  ["config", "yaml yml toml ini cfg conf env properties"],
  ["csv", "csv tsv"],
  ["torrent", "torrent"],
  ["disk", "iso img vhd vhdx vmdk dmg"],
];

const BY_EXTENSION: Record<string, FileKind> = {};
for (const [kind, list] of EXTENSIONS) {
  for (const extension of list.split(" ")) {
    BY_EXTENSION[extension] = kind;
  }
}

/** MIME types are a secondary clue when a filename has no recognized suffix. */
const MIME_KINDS: Record<string, FileKind> = {
  "application/json": "json",
  "text/json": "json",
  "application/xml": "xml",
  "text/xml": "xml",
  "application/pdf": "pdf",
  "application/zip": "archive",
  "application/x-7z-compressed": "archive",
  "application/x-rar-compressed": "archive",
  "application/x-tar": "archive",
  "application/x-bittorrent": "torrent",
  "application/x-iso9660-image": "disk",
  "text/csv": "csv",
  "text/tab-separated-values": "csv",
  "text/plain": "document",
  "text/markdown": "document",
  "application/rtf": "document",
  "application/msword": "document",
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document": "document",
  "application/vnd.ms-excel": "spreadsheet",
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet": "spreadsheet",
  "application/vnd.ms-powerpoint": "slide",
  "application/vnd.openxmlformats-officedocument.presentationml.presentation": "slide",
  "application/x-msdownload": "application",
  "application/x-executable": "application",
  "application/vnd.android.package-archive": "application",
  "application/x-sharedlib": "system",
  "application/x-object": "system",
  "application/x-yaml": "config",
  "text/yaml": "config",
  "application/toml": "config",
  "text/html": "code",
  "text/css": "code",
  "text/x-python": "code",
  "text/x-rust": "code",
  "font/otf": "font",
  "font/ttf": "font",
  "font/woff": "font",
  "font/woff2": "font",
};

/**
 * The family of a filename, or null when neither its suffix nor MIME type is
 * known. A recognized extension wins over MIME because it identifies the
 * saved file, while servers often send generic or inaccurate content types.
 */
export function fileKind(
  name: string,
  mimeType?: string | null,
): FileKind | null {
  const basename = name.trim().split(/[\\/]/).pop() ?? "";
  const match = /\.([a-z0-9]+)$/i.exec(basename);
  if (match) {
    const kind = BY_EXTENSION[match[1].toLowerCase()];
    if (kind) {
      return kind;
    }
  }

  if (!mimeType) {
    return null;
  }
  const mime = mimeType.split(";", 1)[0].trim().toLowerCase();
  return MIME_KINDS[mime] ?? mimeFamily(mime);
}

function mimeFamily(mime: string): FileKind | null {
  if (mime.startsWith("video/")) {
    return "video";
  }
  if (mime.startsWith("audio/")) {
    return "audio";
  }
  if (mime.startsWith("image/")) {
    return "image";
  }
  if (mime.startsWith("font/")) {
    return "font";
  }
  if (mime === "application/javascript" || mime === "text/javascript") {
    return "code";
  }
  return null;
}
