/*
  What counts as media the application can fetch.
  
  A media element can point at three kinds of address: a file over HTTP, a
  blob: URL that only exists inside the tab (MSE), or a manifest for a stream
  the application cannot mux yet. The file is offered as it is; the blob: URL is
  offered through the requests the page made for it, which the capture helpers
  below read. A manifest is never offered, because it would save as a playlist.

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

/** Query parameters that describe one byte range of a whole media file. */
const BDM_RANGE_PARAMS = ["range", "rn", "rbuf"];

/**
 * The whole-file address behind a range request.
 *
 * A streaming player asks for the same signed URL once per part. Removing the
 * range parameters leaves the signature intact and makes that URL serve the
 * whole file, which is the address the application can take.
 */
function bdmFullMediaUrl(url) {
  if (!bdmIsHttpUrl(url)) return url;
  try {
    const parsed = new URL(url);
    let ranged = false;
    for (const name of BDM_RANGE_PARAMS) {
      if (parsed.searchParams.has(name)) {
        parsed.searchParams.delete(name);
        ranged = true;
      }
    }
    return ranged ? parsed.href : url;
  } catch {
    return url;
  }
}

/**
 * A request the page made for a media file the application can fetch: a named
 * file, or the endpoint a player streams from, which always carries an itag.
 */
function bdmIsCapturedMedia(url) {
  if (!bdmIsDownloadableSource(url)) return false;
  if (bdmIsMediaLink(url)) return true;
  try {
    const parsed = new URL(url);
    return parsed.searchParams.has("itag") && /\/(?:videoplayback|get_video)$/.test(parsed.pathname);
  } catch {
    return false;
  }
}

/** Two ranges of one stream share a key; two different streams do not. */
function bdmMediaKey(url) {
  try {
    const parsed = new URL(url);
    const itag = parsed.searchParams.get("itag");
    if (itag) {
      const id = parsed.searchParams.get("id") || "";
      const mime = parsed.searchParams.get("mime") || "";
      return `${id}|${itag}|${mime}`;
    }
  } catch {
    // A malformed address only stands for itself.
  }
  return url;
}

/** YouTube's itag numbers, as the quality a person recognises. */
const BDM_ITAG_QUALITIES = {
  5: "240p", 6: "270p", 17: "144p", 18: "360p", 22: "720p",
  34: "360p", 35: "480p", 36: "240p", 37: "1080p", 38: "4K",
  43: "360p", 44: "480p", 45: "720p", 46: "1080p",
  82: "360p", 83: "480p", 84: "720p", 85: "1080p",
  133: "240p", 134: "360p", 135: "480p", 136: "720p", 137: "1080p",
  138: "2160p", 160: "144p", 242: "240p", 243: "360p", 244: "480p",
  247: "720p", 248: "1080p", 264: "1440p", 266: "2160p",
  271: "1440p", 272: "2160p", 278: "144p", 298: "720p", 299: "1080p",
  302: "720p", 303: "1080p", 308: "1440p", 313: "2160p", 315: "2160p",
  330: "144p", 331: "240p", 332: "360p", 333: "480p", 334: "720p",
  335: "1080p", 336: "1440p", 337: "2160p",
};

/** Itags that carry sound only, and the older itags that carry both tracks. */
const BDM_AUDIO_ITAGS = [139, 140, 141, 249, 250, 251, 256, 258, 325, 327, 328, 338, 380];
const BDM_MUXED_ITAGS = [5, 6, 17, 18, 22, 34, 35, 36, 37, 38, 43, 44, 45, 46];

/** The stream number in a player's address, or "" for a plain file. */
function bdmItagOf(url) {
  try {
    return new URL(url).searchParams.get("itag") || "";
  } catch {
    return "";
  }
}

/**
 * What a captured address holds: "audio", "muxed" for picture and sound in one
 * file, "video" for a picture-only stream, or "" when it is not a stream.
 *
 * shortcut: an itag newer than the lists above is read from its mime alone, so
 * a muxed newcomer is offered as picture only; add it to BDM_MUXED_ITAGS.
 */
function bdmMediaKind(url) {
  const itag = Number(bdmItagOf(url));
  if (!itag) return "";
  if (BDM_AUDIO_ITAGS.includes(itag)) return "audio";
  if (BDM_MUXED_ITAGS.includes(itag)) return "muxed";
  try {
    const mime = new URL(url).searchParams.get("mime") || "";
    if (/^audio\//i.test(mime)) return "audio";
  } catch {
    // A malformed address is a picture stream until proven otherwise.
  }
  return "video";
}

/** The whole-file size the player was told, in bytes, or 0 when it is unknown. */
function bdmMediaSize(url) {
  try {
    const size = Number(new URL(url).searchParams.get("clen"));
    return Number.isFinite(size) && size > 0 ? size : 0;
  } catch {
    return 0;
  }
}

/** The short badge for a row: the container, or AUDIO for a sound stream. */
function bdmMediaLabel(url) {
  if (!bdmItagOf(url)) return bdmSourceLabel(url);
  return bdmMediaKind(url) === "audio" ? "AUDIO" : bdmMediaExtension(url).toUpperCase();
}

/** The row text: a file name, or the quality and size a stream stands for. */
function bdmMediaName(url, fallback) {
  if (!bdmItagOf(url)) return bdmNameFromUrl(url) || fallback || "";
  const kind = bdmMediaKind(url);
  const quality = BDM_ITAG_QUALITIES[Number(bdmItagOf(url))] || "";
  const size = bdmMediaSize(url);
  const bits = [];
  if (kind === "audio") bits.push("audio");
  else if (quality) bits.push(`${quality}${kind === "video" ? " video only" : ""}`);
  else if (kind === "video") bits.push("video only");
  if (size) bits.push(bdmFormatSize(size));
  const detail = bits.join(" · ") || "stream";
  return fallback ? `${fallback} · ${detail}` : detail;
}

/** The container a captured stream is saved in, from its mime. */
function bdmMediaExtension(url) {
  try {
    const mime = new URL(url).searchParams.get("mime") || "";
    const subtype = mime.split("/")[1]?.split(";")[0].trim().toLowerCase();
    if (subtype === "mp4" || subtype === "webm") return subtype;
  } catch {
    // Fall through to the path.
  }
  const extension = bdmNameFromUrl(url).split(".").pop()?.toLowerCase() || "";
  return BDM_FILE_EXTENSIONS.includes(extension) ? extension : "mp4";
}

/** A file name the application can save, with the quality in its name. */
function bdmMediaFileName(url, fallback) {
  const base = String(fallback || "")
    .replace(/[\\/:*?"<>|]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 120);
  if (!bdmItagOf(url)) return bdmNameFromUrl(url) || base || "media";
  const kind = bdmMediaKind(url);
  const label = kind === "audio"
    ? "audio"
    : (BDM_ITAG_QUALITIES[Number(bdmItagOf(url))] || "stream").toLowerCase();
  return `${base || "video"} ${label}.${bdmMediaExtension(url)}`;
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
