/**
 * The family of a file, from its extension. The family picks the drawing in
 * `FileTypeIcon`; an extension that is not here returns null, and the category
 * glyph is used instead, so nothing is ever left without an icon.
 */

export type FileKind =
  | "video"
  | "audio"
  | "archive"
  | "program"
  | "pdf"
  | "document"
  | "spreadsheet"
  | "slide"
  | "image"
  | "code"
  | "font";

/** One row per family: every extension it answers to, lowercased, no dot. */
const EXTENSIONS: [FileKind, string][] = [
  ["video", "mp4 mkv avi mov webm wmv flv m4v mpg mpeg 3gp"],
  ["audio", "mp3 wav flac m4a aac ogg opus wma aiff"],
  ["archive", "zip rar 7z tar gz tgz bz2 xz iso cab torrent"],
  [
    "program",
    "exe msi msix appx dmg pkg deb rpm apk appimage bin jar bat cmd sh ps1",
  ],
  ["pdf", "pdf"],
  ["document", "txt md markdown rtf doc docx odt epub log conf ini cfg nfo"],
  ["spreadsheet", "csv tsv xls xlsx ods"],
  ["slide", "ppt pptx odp key"],
  ["image", "png jpg jpeg gif webp bmp svg ico tif tiff avif heic psd ai raw"],
  [
    "code",
    "js jsx ts tsx mjs cjs json jsonc css scss less html htm py rs go java c cpp h hpp cs php rb swift kt sql yml yaml toml xml vue svelte astro",
  ],
  ["font", "ttf otf woff woff2 eot"],
];

const BY_EXTENSION: Record<string, FileKind> = {};
for (const [kind, list] of EXTENSIONS) {
  for (const extension of list.split(" ")) {
    BY_EXTENSION[extension] = kind;
  }
}

/** The family of a file name, or null when its extension has none. */
export function fileKind(name: string): FileKind | null {
  const match = /\.([a-z0-9]+)$/i.exec(name.trim());
  return match ? (BY_EXTENSION[match[1].toLowerCase()] ?? null) : null;
}
