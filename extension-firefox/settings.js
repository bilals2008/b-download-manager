/*
  The settings the popup, the settings page and the download takeover share.
  Loaded before the other scripts in each page, so the functions below are
  plain globals.
*/

const DEFAULT_SETTINGS = {
  takeover: true,
  /* The download button in the corner of a page that offers media. */
  panel: true,
  /* Empty means every file type is taken over. */
  types: [],
  /* Hosts the browser keeps. */
  skipSites: [],
};

/** ".MP4", "*.mp4" and " mp4 " are all the same extension. */
function cleanType(entry) {
  return String(entry || "")
    .trim()
    .toLowerCase()
    .replace(/^\*?\./, "");
}

/** A host name out of anything the user may have typed. Junk becomes "". */
const HOST = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$/;

function cleanSite(entry) {
  const text = String(entry || "")
    .trim()
    .replace(/^\*?\./, "");
  if (!text) return "";
  try {
    const host = new URL(text.includes("://") ? text : `http://${text}`).hostname.toLowerCase();
    if (host === "localhost") return host;
    return HOST.test(host) ? host : "";
  } catch {
    return "";
  }
}

function normalizeSettings(raw) {
  const entries = (value) =>
    (Array.isArray(value) ? value : [])
      .map((entry) => String(entry).trim().toLowerCase())
      .filter(Boolean);

  return {
    takeover: raw.takeover !== false,
    panel: raw.panel !== false,
    types: [...new Set(entries(raw.types).map(cleanType).filter(Boolean))],
    skipSites: [...new Set(entries(raw.skipSites).map(cleanSite).filter(Boolean))],
  };
}

async function loadSettings() {
  return normalizeSettings(await browser.storage.local.get(DEFAULT_SETTINGS));
}

async function saveSettings(settings) {
  const clean = normalizeSettings(settings);
  await browser.storage.local.set(clean);
  return clean;
}

/** The extension of a link, lower case, without the dot. */
function extensionOf(url) {
  try {
    const name = new URL(url).pathname.split("/").filter(Boolean).pop() || "";
    const index = name.lastIndexOf(".");
    return index > 0 ? name.slice(index + 1).toLowerCase() : "";
  } catch {
    return "";
  }
}

/** A host the settings asked to leave alone, or one of its subdomains. */
function isSkipped(host, skipSites) {
  if (!host) return false;
  return skipSites.some((entry) => host === entry || host.endsWith(`.${entry}`));
}

/*
  Whether the application should take a download over. A skip rule follows the
  page the file was linked from; a download with no referrer falls back to the
  host of the file itself.
*/
function shouldTakeOver(settings, url, pageUrl) {
  if (!settings.takeover) return false;
  if (!/^(https?|ftp):/.test(url)) return false;
  if (settings.types.length && !settings.types.includes(extensionOf(url))) return false;
  if (isSkipped(cleanSite(pageUrl) || cleanSite(url), settings.skipSites)) return false;
  return true;
}
