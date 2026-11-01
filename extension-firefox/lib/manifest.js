/*
  What counts as a stream manifest.

  A player inside a page fetches a playlist (`.m3u8`) or an MPD (`.mpd`) before
  it fetches any media. The extension cannot download either by itself, but the
  application can parse them, so the address is reported instead of refused.
  A link that merely points at a playlist is still only a link: the page has to
  have requested it, which is what the capture helpers check.

  Loaded by the background, the popup and the content script, before media.js.
*/

/** Whether an address names a manifest the application can parse. */
function bdmIsManifestUrl(url) {
  return (
    typeof url === "string" &&
    /^https?:\/\//i.test(url) &&
    /\.(m3u8|mpd)(?:[?#]|$)/i.test(url)
  );
}

/** `hls`, `dash`, or "" when the address is not a manifest. */
function bdmManifestKind(url) {
  if (!bdmIsManifestUrl(url)) return "";
  return /\.m3u8(?:[?#]|$)/i.test(url) ? "hls" : "dash";
}

/** The short badge for a manifest row. */
function bdmManifestLabel(url) {
  const kind = bdmManifestKind(url);
  return kind === "hls" ? "HLS" : kind === "dash" ? "DASH" : "FILE";
}

/** A row name for a manifest: the page title and the format. */
function bdmManifestName(url, fallback) {
  const kind = bdmManifestKind(url);
  if (!kind) return String(fallback || "");
  const label = kind === "hls" ? "HLS stream" : "DASH stream";
  const base = String(fallback || "").trim();
  return base ? `${base} · ${label}` : label;
}

/** The file name the application should suggest for a manifest. */
function bdmManifestFileName(fallback) {
  const base = String(fallback || "")
    .replace(/[\\/:*?"<>|]+/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 120);
  return base || "video";
}
