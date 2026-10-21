/**
 * Download domain model.
 *
 * The category names, the tree groups and the column set come out of the
 * string tables in `IDMan.exe` (see `ANALYSIS.md`): the categories are
 * General, Video, Music, Documents, Programs, Games, Compressed and Other, and
 * the categories pane is a tree with the groups "All Downloads", "Unfinished"
 * and "Finished" above them.
 */

export const CATEGORIES = [
  "General",
  "Video",
  "Music",
  "Documents",
  "Programs",
  "Games",
  "Compressed",
  "Other",
] as const;

export type CategoryName = (typeof CATEGORIES)[number];

export type DownloadStatus =
  | "downloading"
  | "queued"
  | "paused"
  | "stopped"
  | "completed"
  | "error";

export interface Download {
  id: number;
  fileName: string;
  url: string;
  category: CategoryName;
  /** Total size in bytes. */
  size: number;
  /** Bytes already on disk. */
  downloaded: number;
  status: DownloadStatus;
  /** Bytes per second while downloading. */
  speed: number;
  /** Seconds left, 0 when not downloading. */
  eta: number;
  /** Epoch millis the app first saw the download. */
  addedAt: number;
  /** Epoch millis of the last progress event. */
  lastTryAt: number;
  /** Folder the file is saved to. */
  savePath: string;
  /** Full destination path, as the engine reports it. */
  dest: string;
  /** Connections the engine currently holds. */
  connections: number;
  /** Failure text from the engine, when the status is "error". */
  error: string | null;
}

/** The categories pane filter. A category name filters to that category. */
export type DownloadFilter = "all" | "unfinished" | "finished" | CategoryName;

export const FILTER_ALL: DownloadFilter = "all";

export type SortKey =
  | "order"
  | "name"
  | "size"
  | "status"
  | "eta"
  | "speed"
  | "lastTry"
  | "path";

export interface SortState {
  key: SortKey;
  dir: "asc" | "desc";
}

export const STATUS_LABELS: Record<DownloadStatus, string> = {
  downloading: "Downloading",
  queued: "Queued",
  paused: "Paused",
  stopped: "Stopped",
  completed: "Completed",
  error: "Error",
};

export function isFinished(download: Download): boolean {
  return download.status === "completed";
}

export function isUnfinished(download: Download): boolean {
  return download.status !== "completed";
}

export function progressOf(download: Download): number {
  if (download.size <= 0) {
    return 0;
  }
  return Math.min(100, (download.downloaded / download.size) * 100);
}

/** "1.2 GB" style size, matching how IDM labels the Size column. */
export function formatSize(bytes: number): string {
  if (bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(
    units.length - 1,
    Math.floor(Math.log(bytes) / Math.log(1024)),
  );
  const value = bytes / 1024 ** exponent;
  const decimals = exponent === 0 ? 0 : value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(decimals)} ${units[exponent]}`;
}

/** "128.4 KB/s" style transfer rate. */
export function formatSpeed(bytesPerSecond: number): string {
  if (bytesPerSecond <= 0) {
    return "";
  }
  return `${formatSize(bytesPerSecond)}/s`;
}

/** "1 min 12 s", "2 h 5 min" or an em dash when there is no estimate. */
export function formatEta(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return "—";
  }
  if (seconds < 60) {
    return `${Math.round(seconds)} s`;
  }
  if (seconds < 3600) {
    const minutes = Math.floor(seconds / 60);
    return `${minutes} min ${Math.round(seconds % 60)} s`;
  }
  const hours = Math.floor(seconds / 3600);
  return `${hours} h ${Math.floor((seconds % 3600) / 60)} min`;
}

/** "21.10.2026 14:32" style timestamp for the date columns. */
export function formatDate(epochMillis: number): string {
  const date = new Date(epochMillis);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${pad(date.getDate())}.${pad(date.getMonth() + 1)}.${date.getFullYear()} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** Make a user-typed name safe for a Windows path. */
export function sanitizeFileName(name: string): string {
  const cleaned = name
    .replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[. ]+$/, "");
  return cleaned || "download";
}

/** File name of a URL, without the query string. */
export function fileNameFromUrl(url: string): string {
  try {
    const parsed = new URL(url);
    const last = parsed.pathname.split("/").filter(Boolean).pop();
    return last ? decodeURIComponent(last) : "download";
  } catch {
    return "download";
  }
}

/** Split a destination path into its folder and file name. */
export function splitPath(dest: string): { dir: string; base: string } {
  const trimmed = dest.replace(/[\\/]+$/, "");
  const index = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  if (index <= 0) {
    return { dir: ".", base: trimmed };
  }
  return { dir: trimmed.slice(0, index), base: trimmed.slice(index + 1) };
}

/** Join a folder and a file name with the separator the folder already uses. */
export function joinPath(dir: string, fileName: string): string {
  const folder = dir.trim().replace(/[\\/]+$/, "");
  if (!folder) {
    return fileName;
  }
  const separator = folder.includes("\\") || !folder.includes("/") ? "\\" : "/";
  return `${folder}${separator}${fileName}`;
}

const EXTENSION_CATEGORY: Record<string, CategoryName> = {
  mp4: "Video",
  mkv: "Video",
  avi: "Video",
  mov: "Video",
  webm: "Video",
  m4v: "Video",
  mpg: "Video",
  mpeg: "Video",
  mp3: "Music",
  flac: "Music",
  wav: "Music",
  m4a: "Music",
  ogg: "Music",
  aac: "Music",
  pdf: "Documents",
  doc: "Documents",
  docx: "Documents",
  txt: "Documents",
  rtf: "Documents",
  xls: "Documents",
  xlsx: "Documents",
  ppt: "Documents",
  pptx: "Documents",
  csv: "Documents",
  epub: "Documents",
  exe: "Programs",
  msi: "Programs",
  msix: "Programs",
  dmg: "Programs",
  pkg: "Programs",
  apk: "Programs",
  deb: "Programs",
  rpm: "Programs",
  appimage: "Programs",
  zip: "Compressed",
  rar: "Compressed",
  "7z": "Compressed",
  tar: "Compressed",
  gz: "Compressed",
  bz2: "Compressed",
  xz: "Compressed",
  iso: "Compressed",
};

/** Best-effort category for a file name, so the categories pane stays useful. */
export function categoryOfFile(fileName: string): CategoryName {
  const extension = fileName.split(".").pop()?.toLowerCase() ?? "";
  return EXTENSION_CATEGORY[extension] ?? "Other";
}

/** The engine's eight-connection default, matching `DEFAULT_CONNECTIONS` in Rust. */
export const DEFAULT_CONNECTIONS = 8;

