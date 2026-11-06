/*
  The Chrome/Edge build: the same loopback API as the Firefox extension.

  Three things hand a link to the application: the right click menu, a
  download the browser starts, and a media address the page's player has
  fetched — captured with the browser's own headers, the way the classic
  Windows download manager does it.

  While the application runs, its Settings page owns the capture switch, the
  ask switch, the file types and the site list. The small local fallback
  (`takeover`) is used only when the application cannot be reached.
*/

const BRIDGE = "http://127.0.0.1:27500";
const CLIENT = "browser-extension";
const APP = "b-download-manager";

async function request(method, path, body) {
  const response = await fetch(`${BRIDGE}${path}`, {
    method,
    cache: "no-store",
    credentials: "omit",
    headers: {
      "X-BDM-Client": CLIENT,
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  const payload = await response.json().catch(() => null);
  if (!payload || !payload.ok) {
    throw new Error(payload?.error?.message || `the app answered ${response.status}`);
  }
  return payload.data;
}

/** Downloads already handed over, so a repeated event cannot send twice. */
const handedOver = new Set();
/** The tab whose first manifest has been sent; later media playlists are its children. */
const manifestTabs = new Set();
chrome.tabs?.onUpdated?.addListener((tabId, change) => {
  if (change.status === "loading") {
    manifestTabs.delete(tabId);
    mediaCounts.delete(tabId);
    seenMedia.delete(tabId);
    chrome.action.setBadgeText({ tabId, text: "" }).catch(() => {});
  }
});
chrome.tabs?.onRemoved?.addListener((tabId) => {
  mediaCounts.delete(tabId);
  seenMedia.delete(tabId);
});

async function handOver(url, referer, name, ask) {
  return request("POST", "/v1/downloads", {
    url,
    name: name || "",
    referer: referer || "",
    mode: ask ? "ask" : "direct",
  });
}

/** The file name of a link, the way the application would name it. */
function fileNameOf(url) {
  try {
    const path = new URL(url).pathname;
    const last = path.split("/").filter(Boolean).pop();
    return last ? decodeURIComponent(last) : "";
  } catch {
    return "";
  }
}

function extensionOf(url) {
  const name = fileNameOf(url);
  const index = name.lastIndexOf(".");
  return index > 0 ? name.slice(index + 1).toLowerCase() : "";
}

function hostOf(url) {
  try {
    return new URL(url).hostname.toLowerCase();
  } catch {
    return "";
  }
}

function skipped(host, sites) {
  if (!host) return false;
  return sites.some((entry) => host === entry || host.endsWith(`.${entry}`));
}

/** The rules the app owns, or the local fallback when it is not running. */
async function effectiveSettings() {
  let policy = null;
  try {
    policy = await request("GET", "/v1/capture");
  } catch {
    // A closed app leaves the browser to its own rules.
  }
  if (policy && typeof policy === "object") {
    return {
      capture: policy.capture === true,
      ask: policy.ask === true,
      types: Array.isArray(policy.types) ? policy.types.map(String) : [],
      skipSites: Array.isArray(policy.skipSites) ? policy.skipSites.map(String) : [],
    };
  }
  const local = await chrome.storage.local.get({ takeover: true, ask: false });
  return {
    capture: local.takeover !== false,
    ask: local.ask === true,
    types: [],
    skipSites: [],
  };
}

function createMenus() {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({
      id: "bdm-download",
      title: "Download with B Download Manager",
      contexts: ["link", "video", "audio", "image"],
    });
  });
}

chrome.runtime.onInstalled.addListener(createMenus);
chrome.runtime.onStartup.addListener(createMenus);

chrome.contextMenus.onClicked.addListener((info) => {
  if (info.menuItemId !== "bdm-download") return;
  const url = info.linkUrl || info.srcUrl;
  if (!url) return;
  // An explicit click is never filtered by the site list.
  effectiveSettings()
    .then((settings) => handOver(url, info.pageUrl, fileNameOf(url), settings.ask))
    .catch((error) => {
      console.warn("B Download Manager did not take the link:", error.message);
    });
});

/* The browser started a download. Freeze it first, then hand it over, and
   only then stop the browser copy.

   The freeze matters: deciding round-trips to the bridge — the capture policy
   and the hand-over itself — and while that is in flight the download keeps
   writing into the browser's own downloads folder. Without it a small or fast
   file finishes as a browser download before the cancel lands. If the
   application is closed, or does not take the link, the browser copy is let go
   again instead of being lost. */

async function freezeDownload(id) {
  const hold = async () => {
    try {
      await chrome.downloads.pause(id);
      return true;
    } catch {
      return false;
    }
  };
  if (await hold()) return true;
  // A pause asked before the transfer has begun is refused; one short retry
  // covers that window. A download that is already over fails twice, and the
  // hand-over goes on regardless — the app still takes the link.
  await new Promise((resolve) => setTimeout(resolve, 150));
  return hold();
}

async function thawDownload(id) {
  await chrome.downloads.resume(id).catch(() => {
    // The copy is already running, or already finished; the browser owns it.
  });
}

chrome.downloads.onCreated.addListener(async (item) => {
  if (!item || handedOver.has(item.id)) return;
  if (!/^https?:\/\//i.test(item.url)) return;

  const frozen = await freezeDownload(item.id);
  const leaveToBrowser = async () => {
    if (frozen) await thawDownload(item.id);
  };

  const settings = await effectiveSettings();
  if (!settings.capture) {
    await leaveToBrowser();
    return;
  }
  if (settings.types.length && !settings.types.includes(extensionOf(item.url))) {
    await leaveToBrowser();
    return;
  }
  const host = hostOf(item.referrer || "") || hostOf(item.url);
  if (skipped(host, settings.skipSites)) {
    await leaveToBrowser();
    return;
  }

  handedOver.add(item.id);
  setTimeout(() => handedOver.delete(item.id), 30000);

  try {
    await handOver(item.url, item.referrer || "", fileNameOf(item.url), settings.ask);
  } catch {
    // The app did not take it; the browser download goes on untouched.
    await leaveToBrowser();
    return;
  }

  await chrome.downloads.cancel(item.id).catch(() => {});
  await chrome.downloads.erase({ id: item.id }).catch(() => {});
});

/* ------------------------------------------------------- media capture */

/*
  The third hand-over: the requests the page's player makes. An address the
  player was already authorized for is offered to the application together
  with the exact cookies and agent the player used, which is what lets the
  application fetch an address that refuses any other visitor.
*/

const MEDIA_STREAM_PATTERN = /\.(m3u8|mpd|ism)(?:[?#]|$)/i;
const MEDIA_FILE_PATTERN = /\.(mp4|m4v|webm|mkv|mov|avi|ogv|mp3|m4a|aac|flac|wav|ogg|oga|opus)(?:[?#]|$)/i;
/** Query parameters that name one byte range of a whole media file. */
const MEDIA_RANGE_PARAMS = ["range", "rn", "rbuf", "bytestart", "byteend"];

/** The media count of each tab, shown over the toolbar button. */
const mediaCounts = new Map();
/** Reported addresses per tab, so a range request cannot report twice. */
const seenMedia = new Map();

chrome.action.setBadgeBackgroundColor({ color: "#c8ff00" }).catch(() => {});

/** What an address is to the application: a kind, or "" when it is not media. */
function mediaKindOf(url) {
  if (MEDIA_STREAM_PATTERN.test(url)) {
    return /\.m3u8(?:[?#]|$)/i.test(url) ? "hls" : "dash";
  }
  try {
    const parsed = new URL(url);
    if (
      parsed.searchParams.has("itag") &&
      /\/(?:videoplayback|get_video)$/.test(parsed.pathname)
    ) {
      return "youtube";
    }
  } catch {
    return "";
  }
  return MEDIA_FILE_PATTERN.test(url) ? "file" : "";
}

/** The whole-file address behind a range request. */
function fullMediaUrl(url) {
  try {
    const parsed = new URL(url);
    let ranged = false;
    for (const name of MEDIA_RANGE_PARAMS) {
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

/** Two ranges of one stream share a key; two different streams do not. */
function mediaKeyOf(url) {
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

function seenMediaFor(tabId) {
  let seen = seenMedia.get(tabId);
  if (!seen) {
    seen = new Set();
    seenMedia.set(tabId, seen);
  }
  return seen;
}

/** The headers the player's requests carry: the site's cookies and the
    browser's user agent. The application scopes them to the manifest's own
    origin and asks the reader once before it forwards a cookie. */
async function mediaHeaders(url) {
  const headers = {};
  try {
    const cookies = await chrome.cookies.getAll({ url });
    if (Array.isArray(cookies) && cookies.length) {
      headers.Cookie = cookies
        .map((cookie) => `${cookie.name}=${cookie.value}`)
        .join("; ");
    }
  } catch {
    // The cookies permission may be refused; the address may still work.
  }
  if (navigator.userAgent) headers["User-Agent"] = navigator.userAgent;
  return headers;
}

chrome.webRequest.onCompleted.addListener(
  (request) => {
    void reportMedia(request).catch(() => {});
  },
  { urls: ["<all_urls>"], types: ["media", "xmlhttprequest", "object"] },
);

async function reportMedia(request) {
  const url = String(request.url || "");
  if (url.startsWith(BRIDGE)) return;
  const kind = mediaKindOf(url);
  if (!kind) return;
  // A named file is offered only when a player element fetched it; streams
  // and manifests arrive through the player's script, which is an XHR.
  if (kind === "file" && request.type !== "media") return;
  const tabId = request.tabId;
  if (!Number.isInteger(tabId) || tabId < 0) return;

  const manifestUrl = fullMediaUrl(url);
  const key = mediaKeyOf(manifestUrl);
  const seen = seenMediaFor(tabId);
  if (seen.has(key)) return;
  seen.add(key);

  const settings = await effectiveSettings();
  if (!settings.capture) return;
  const tab = await chrome.tabs.get(tabId).catch(() => null);
  if (!tab || !/^https?:\/\//i.test(tab.url || "")) return;
  if (skipped(hostOf(tab.url), settings.skipSites)) return;

  try {
    await request("POST", "/v1/media", {
      pageUrl: tab.url,
      manifestUrl,
      kind,
      title: tab.title || "",
      referer: tab.url,
      headers: await mediaHeaders(manifestUrl),
      presentation: "window",
    });
    const count = Math.min(99, (mediaCounts.get(tabId) || 0) + 1);
    mediaCounts.set(tabId, count);
    chrome.action
      .setBadgeText({ tabId, text: String(count) })
      .catch(() => {});
  } catch {
    // The app is closed or refused it; the player keeps its own copy.
  }
}
