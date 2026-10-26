/**
 * Download domain model.
 *
 * The category definitions live in `categories.ts`; the names, the tree groups
 * and the column set come out of the string tables in `IDMan.exe` (see
 * `ANALYSIS.md`): the categories are General, Video, Music, Documents, Programs,
 * Games, Compressed and Other, and the categories pane is a tree with the
 * groups "All Downloads", "Unfinished" and "Finished" above them.
 */

import type { CategoryName } from "./categories.ts";

export {
  BUILTIN_CATEGORIES,
  CATEGORIES,
  DEFAULT_CATEGORIES,
  categoryFolder,
  categoryName,
  categoryOfFile,
  detectCategory,
  knownCategory,
  type BuiltinCategory,
  type CategoryDefinition,
  type CategoryDraft,
  type CategoryName,
} from "./categories.ts";

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
  /** The page the link was on, when the browser knew it. */
  referer: string | null;
  /** Connections the engine currently holds. */
  connections: number;
  /** Failure text from the engine, when the status is "error". */
  error: string | null;
  /** Whether the engine can continue from what is on disk. Null until known. */
  resumable: boolean | null;
  /** A note kept with the download. */
  description: string | null;
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

/**
 * The status filter of the list. It applies inside the filter of the
 * categories pane, so "Video" and "Error" can be asked for together.
 */
export type StatusFilter = "all" | DownloadStatus;

export const STATUS_FILTER_LABELS: Record<StatusFilter, string> = {
  all: "All statuses",
  ...STATUS_LABELS,
};

export function matchesStatusFilter(
  download: Download,
  filter: StatusFilter,
): boolean {
  return filter === "all" || download.status === filter;
}

export function isFinished(download: Download): boolean {
  return download.status === "completed";
}

export function isUnfinished(download: Download): boolean {
  return download.status !== "completed";
}

/**
 * Why the save location of a download is fixed, or null when it can move.
 *
 * A task that is writing holds its file open, so the parts cannot travel. A
 * queued row is parked with live parts too, and moves after it leaves the
 * queue.
 */
export function destinationLock(download: Download): string | null {
  if (download.status === "downloading") {
    return "Stop the download before changing where it is saved.";
  }
  if (download.status === "queued") {
    return "Take the download out of the queue before changing where it is saved.";
  }
  return null;
}

/** Only a finished download has a file of its own to delete. */
export function canDeleteFile(download: Download): boolean {
  return isFinished(download);
}

/** Whether a stopped download would continue from the bytes on disk. */
export function resumableLabel(value: boolean | null): string {
  if (value === null) {
    return "Not known yet";
  }
  return value
    ? "Yes, a stopped download continues from its bytes"
    : "No, a retry starts from the first byte";
}

export function progressOf(download: Download): number {
  // A finished download is all of itself. The engine drops the parts and the
  // manifest once the ranges are joined, so a row restored from the index knows
  // the size and not the bytes; the status is the answer there.
  if (download.status === "completed") {
    return 100;
  }
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

/** Last segment of a folder path, for the tree rows. */
export function folderLabel(path: string): string {
  const segments = path
    .replace(/[\\/]+$/, "")
    .split(/[\\/]/)
    .filter(Boolean);
  return segments.pop() ?? path;
}

/** The engine's eight-connection default, matching `DEFAULT_CONNECTIONS` in Rust. */
export const DEFAULT_CONNECTIONS = 8;

/** The schemes the engine can fetch. */
const DOWNLOAD_PROTOCOLS = ["http:", "https:", "ftp:"];

/**
 * The address the engine may fetch, or null when it may not.
 *
 * The browser's own parser decides, so the answer matches what the request
 * will do: a scheme of http, https or ftp, a host, and no raw whitespace.
 */
export function downloadUrl(value: string): URL | null {
  const trimmed = value.trim();
  if (!trimmed || /\s/.test(trimmed)) {
    return null;
  }
  try {
    const url = new URL(trimmed);
    if (!DOWNLOAD_PROTOCOLS.includes(url.protocol)) {
      return null;
    }
    return url.hostname ? url : null;
  } catch {
    return null;
  }
}

/**
 * What is wrong with the address in the add dialog, or null when it can go to
 * the engine.
 *
 * The answer names the fix rather than the fault: a missing scheme, a space
 * the browser would send as it stands, or a link that is not a download at
 * all. The dialog shows it beside the field, so the user does not have to
 * guess what the engine would refuse.
 */
export function urlProblem(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return "Paste the address of the file to download.";
  }
  if (/\s/.test(trimmed)) {
    return "The address cannot contain spaces.";
  }
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    return "Enter a full address, starting with http://, https:// or ftp://.";
  }
  if (!DOWNLOAD_PROTOCOLS.includes(parsed.protocol)) {
    return "Only http, https and ftp addresses can be downloaded.";
  }
  if (!parsed.hostname) {
    return "The address has no site name.";
  }
  return null;
}

/**
 * Whether two addresses name the same link. The browser's parser folds the
 * parts that do not matter (a host's case, a default port), so the duplicate
 * note in the dialog does not depend on how the address was typed.
 */
export function sameAddress(left: string, right: string): boolean {
  const canonical = (value: string) => {
    const trimmed = value.trim();
    try {
      return new URL(trimmed).href;
    } catch {
      return trimmed;
    }
  };
  return canonical(left) === canonical(right);
}

/**
 * A failed link check, in words the add dialog can show.
 *
 * The engine already explains why a server said no; this maps the common
 * refusals to the one line that helps: a sign-in, an expired link, a file
 * that is gone, a busy server. A check is advice, not a gate, so the download
 * can still be started after any of these.
 */
export function probeWarning(message: string): string {
  const text = message.trim();
  if (!text) {
    return "The link could not be checked. The download may still work.";
  }
  if (/cannot reach the server/i.test(text)) {
    return "The server could not be reached. Check the address and your connection.";
  }
  if (/\b407\b/.test(text)) {
    return "The proxy asks for a sign-in, so it refused the check.";
  }
  if (/sign in|\(401\)/i.test(text)) {
    return "The server asks for a sign-in. The download may fail without one.";
  }
  if (/\(403\)|refused the request/i.test(text)) {
    return "The server refused the check. The link may have expired, or the site may only answer a browser.";
  }
  if (/not on the server|gone for good|\(404\)|\(410\)/i.test(text)) {
    return "The file is not on the server any more.";
  }
  if (/\(429\)|server is busy/i.test(text)) {
    return "The server is busy. Try again in a moment.";
  }
  if (/redirect/i.test(text)) {
    return "The address redirected too many times.";
  }
  if (/server had a problem/i.test(text)) {
    return "The server had a problem. Try again later.";
  }
  return `The link could not be checked: ${text}`;
}

/** The longest name one component of a path may have, as `dest.rs` enforces. */
export const MAX_FILE_NAME = 200;

/**
 * What is wrong with a file name, or null when the file system can hold it.
 *
 * The rules are the ones `dest.rs` enforces on the way into the engine, so
 * the dialog explains a name before a download is created for it.
 */
export function fileNameProblem(name: string): string | null {
  if (!name.trim()) {
    return "Enter a file name.";
  }
  if (name.length > MAX_FILE_NAME) {
    return `The file name is longer than ${MAX_FILE_NAME} characters.`;
  }
  if (/[<>:"/\\|?*\u0000-\u001f]/.test(name)) {
    return 'A file name cannot contain < > : " / \\ | ? *.';
  }
  if (name === "." || name === "..") {
    return "That is not a file name.";
  }
  if (/[. ]$/.test(name)) {
    return "A file name cannot end with a dot or a space.";
  }
  const stem = name.split(".")[0].toLowerCase();
  if (RESERVED_NAMES.includes(stem)) {
    return `${name} is a name Windows keeps for a device.`;
  }
  return null;
}

/** The device names Windows reserves, with or without an extension. */
const RESERVED_NAMES = [
  "con",
  "prn",
  "aux",
  "nul",
  ...Array.from({ length: 9 }, (_, index) => `com${index + 1}`),
  ...Array.from({ length: 9 }, (_, index) => `lpt${index + 1}`),
];

/**
 * A file name the engine will accept, made from whatever the server, the
 * browser or the address offered.
 *
 * The dialog has no name field, so the name it starts with has to be one the
 * file system can hold: characters a path cannot carry are replaced, a name
 * Windows keeps for a device gets a leading underscore, and a name longer
 * than the engine allows is cut back with its extension kept.
 */
export function safeFileName(raw: string): string {
  let name = sanitizeFileName(raw);
  const stem = name.split(".")[0].toLowerCase();
  if (RESERVED_NAMES.includes(stem)) {
    name = `_${name}`;
  }
  if (name.length > MAX_FILE_NAME) {
    const dot = name.lastIndexOf(".");
    const extension =
      dot > 0 && name.length - dot <= 20 ? name.slice(dot) : "";
    const stem = extension ? name.slice(0, dot) : name;
    const room = Math.max(1, MAX_FILE_NAME - extension.length);
    name = sanitizeFileName(`${stem.slice(0, room)}${extension}`);
  }
  return name;
}

