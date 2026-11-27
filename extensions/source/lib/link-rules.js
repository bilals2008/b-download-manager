/*
  What a linked address is, for the Download all links window.

  The window offers the files a page links to, grouped the way the application
  groups them. Everything here is pure: no DOM, no storage, no network, so the
  content script, the background page and the window can share one answer, and
  the checks can run the same code.

  The category rules mirror the application's built-in categories
  (`src/lib/categories.ts`). A link is never guessed to be a file: an address
  with no extension, or with a page extension, is a page, and a manifest for a
  stream the engine cannot save is not offered at all.
*/

/** The categories the window filters by, in the order it shows them. */
const BDM_LINK_CATEGORIES = [
  "Video",
  "Music",
  "Images",
  "Documents",
  "Programs",
  "Compressed",
  "Other",
];

/**
 * The extensions of each category.
 *
 * These are the application's built-in rules, plus the image types the tree
 * has no category for; an image download is filed under Other there. A custom
 * category the user added in the application is not mirrored here, because the
 * extension cannot read the application's preference file.
 */
const BDM_LINK_EXTENSIONS = {
  Video: ["mp4", "mkv", "avi", "mov", "webm", "m4v", "mpg", "mpeg"],
  Music: ["mp3", "flac", "wav", "m4a", "ogg", "aac"],
  Images: ["jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "tif", "tiff", "heic", "avif"],
  Documents: [
    "pdf", "doc", "docx", "txt", "rtf", "xls", "xlsx", "ppt", "pptx", "csv", "epub",
  ],
  Programs: ["exe", "msi", "msix", "dmg", "pkg", "apk", "deb", "rpm", "appimage"],
  Compressed: ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso"],
};

/** A linked page, not a file. Offered only when the window asks for pages. */
const BDM_LINK_PAGES = [
  "html", "htm", "xhtml", "shtml", "php", "asp", "aspx", "jsp", "jspx", "cfm", "cgi",
];

/**
 * Streams and their segments. The engine has no muxer, so a `.m3u8` would be
 * saved as a playlist and a `.ts` segment as one piece of a film. They are
 * reported instead of offered.
 */
const BDM_LINK_STREAMS = ["m3u8", "mpd", "ism", "m4s", "ts"];

/** Why a link was left out, in the words the window reports. */
const BDM_LINK_SKIP_LABELS = {
  self: "links to this page",
  scheme: "links that are not web addresses",
  stream: "stream manifests the engine cannot save",
  unusable: "links that could not be read",
};

/** The protocol an address has, with the colon, or "" when it cannot be read. */
function bdmLinkProtocol(url) {
  try {
    return new URL(url).protocol;
  } catch {
    return "";
  }
}

/**
 * The extension of a link, lower case and without the dot.
 *
 * The name the link carries is read first, because `download="report.pdf"`
 * on `/get?id=5` is the only place that names the file. A trailing slash and a
 * query string are not part of the extension.
 */
function bdmLinkExtension(url, downloadName) {
  const named = String(downloadName || "").trim();
  const path = named || (() => {
    try {
      return new URL(url).pathname;
    } catch {
      return String(url || "");
    }
  })();
  const last = path.split("/").filter(Boolean).pop() || "";
  // A file whose dot arrived as `%2E` is still a file.
  let name = last;
  try {
    name = decodeURIComponent(last);
  } catch {
    // A malformed escape is read as it is.
  }
  const dot = name.lastIndexOf(".");
  if (dot <= 0 || dot === name.length - 1) return "";
  const extension = name.slice(dot + 1).toLowerCase();
  return /^[a-z0-9][a-z0-9+_-]{0,15}$/.test(extension) ? extension : "";
}

/** The category a file belongs to. An unknown type is Other, never an error. */
function bdmLinkCategory(extension) {
  const wanted = String(extension || "").toLowerCase();
  if (!wanted) return "Other";
  for (const name of BDM_LINK_CATEGORIES) {
    if (name !== "Other" && (BDM_LINK_EXTENSIONS[name] || []).includes(wanted)) {
      return name;
    }
  }
  return "Other";
}

/** The category the application stores. It has no Images category. */
function bdmLinkAppCategory(category) {
  return category === "Images" ? "Other" : category || "Other";
}

/** What kind of link it is: a file, a page, or a stream the engine cannot save. */
function bdmLinkKind(extension) {
  const wanted = String(extension || "").toLowerCase();
  if (!wanted) return "page";
  if (BDM_LINK_STREAMS.includes(wanted)) return "unsupported";
  if (BDM_LINK_PAGES.includes(wanted)) return "page";
  return "file";
}

/** The short badge of a row: the type in capitals, or PAGE. */
function bdmLinkBadge(extension, kind) {
  if (kind === "page") return "PAGE";
  const wanted = String(extension || "").toUpperCase();
  if (!wanted) return "FILE";
  return wanted.length > 5 ? "FILE" : wanted;
}

/** The last segment of an address, decoded, as a file name. */
function bdmLinkName(url, downloadName) {
  const named = String(downloadName || "").trim();
  if (named) {
    return named.split(/[\\/]/).filter(Boolean).pop() || named;
  }
  try {
    const last = new URL(url).pathname.split("/").filter(Boolean).pop();
    if (last) return decodeURIComponent(last);
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

/** The same address without its fragment. That is what makes two links one. */
function bdmLinkKey(url) {
  try {
    const parsed = new URL(String(url || ""));
    parsed.hash = "";
    return parsed.href;
  } catch {
    return String(url || "");
  }
}

/** The address of a page without its fragment, for the self-link comparison. */
function bdmLinkPageKey(url) {
  return bdmLinkKey(url);
}

/**
 * One link, or the reason it was left out.
 *
 * `anchor` is anything with `href` and an optional `download` name: an `<a>`,
 * an `<area>`, or a plain object in a test. `pageUrl` is the page the link was
 * found on, so a link back to the same page is not offered as a download.
 */
function bdmLinkClassify(anchor, pageUrl) {
  const href = String(anchor?.href || "").trim();
  if (!href) return { ok: false, reason: "unusable" };

  const protocol = bdmLinkProtocol(href);
  if (!protocol) return { ok: false, reason: "unusable" };
  if (protocol !== "http:" && protocol !== "https:" && protocol !== "ftp:") {
    return { ok: false, reason: "scheme" };
  }

  const key = bdmLinkKey(href);
  if (pageUrl && key === bdmLinkPageKey(pageUrl)) {
    return { ok: false, reason: "self" };
  }

  const download = String(anchor?.download || "").trim();
  const extension = bdmLinkExtension(href, download);
  const kind = bdmLinkKind(extension);
  if (kind === "unsupported") return { ok: false, reason: "stream" };

  const name = bdmLinkName(href, download);
  if (!name) return { ok: false, reason: "unusable" };

  const category = kind === "page" ? "Other" : bdmLinkCategory(extension);
  return {
    ok: true,
    item: {
      url: href,
      key,
      name,
      extension,
      category,
      kind,
      badge: bdmLinkBadge(extension, kind),
    },
  };
}

/** A link the engine can save as a file. */
function bdmLinkIsFile(item) {
  return Boolean(item) && item.kind === "file";
}

/** The phrase a count of skipped links is reported with. */
function bdmLinkSkipLabel(reason) {
  return BDM_LINK_SKIP_LABELS[reason] || "links that were not offered";
}
