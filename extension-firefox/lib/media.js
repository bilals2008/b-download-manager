/*
  What counts as media the application can fetch.
  
  A media element can point at three kinds of address: a file over HTTP, a
  blob: URL that only exists inside the tab (MSE), or a manifest for a stream
  the application cannot mux yet. Only the first kind is offered; pretending
  otherwise would hand the user a broken download.

  Loaded by the background, the popup and the content script.
*/

/** A file the engine can fetch as it is. */
const BDM_FILE_EXTENSIONS = [
  "mp4", "m4v", "webm", "mkv", "mov", "avi", "ogv",
  "mp3", "m4a", "aac", "flac", "wav", "ogg", "oga", "opus",
];

const BDM_FILE_PATTERN = new RegExp(
  `\\.(${BDM_FILE_EXTENSIONS.join("|")})(?:[?#]|$)`,
  "i",
);

/** Streams whose parts would arrive as text, not as a playable file. */
const BDM_STREAM_PATTERN = /\.(m3u8|mpd|ism|ts)(?:[?#]|$)/i;

function bdmIsHttpUrl(url) {
  return typeof url === "string" && /^https?:\/\//i.test(url);
}

function bdmIsStreamUrl(url) {
  return typeof url === "string" && BDM_STREAM_PATTERN.test(url);
}

/** A file address the app can download and the browser can play. */
function bdmIsDownloadableSource(url) {
  return bdmIsHttpUrl(url) && !bdmIsStreamUrl(url);
}

/** A link that names a media file. */
function bdmIsMediaLink(url) {
  return bdmIsHttpUrl(url) && BDM_FILE_PATTERN.test(url);
}

/** The last segment of a URL, decoded, as a file name. */
function bdmNameFromUrl(url) {
  try {
    const path = new URL(url).pathname;
    const last = path.split("/").filter(Boolean).pop();
    return last ? decodeURIComponent(last) : "";
  } catch {
    return "";
  }
}

/** A short label for a source: MP4, WEBM, or "file" when the name is opaque. */
function bdmSourceLabel(url) {
  const extension = (url.split(/[?#]/)[0].split(".").pop() || "").toLowerCase();
  if (extension.length >= 2 && extension.length <= 5 && BDM_FILE_EXTENSIONS.includes(extension)) {
    return extension.toUpperCase();
  }
  return "FILE";
}

/** "1.2 MB" style size, matching the app's own formatting. */
function bdmFormatSize(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  const value = bytes / 1024 ** exponent;
  const decimals = exponent === 0 ? 0 : value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(decimals)} ${units[exponent]}`;
}

/** "1.2 MB/s" style rate. */
function bdmFormatSpeed(bytesPerSecond) {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return "";
  return `${bdmFormatSize(bytesPerSecond)}/s`;
}

/** The percentage a download has reached, 0 to 100. */
function bdmPercentOf(download) {
  if (!download) return 0;
  if (download.status === "completed") return 100;
  if (!download.total || download.total <= 0) return 0;
  return Math.min(100, Math.max(0, (download.downloaded / download.total) * 100));
}

/** The file name of a destination path. */
function bdmFileNameOfPath(path) {
  const last = String(path || "").split(/[\\/]/).filter(Boolean).pop();
  return last || "download";
}
