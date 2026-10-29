/*
  The Chrome/Edge build: the same loopback API as the Firefox extension.

  Two things hand a link to the application: the right click menu, and a
  download the browser starts. A browser download is handed over before it is
  cancelled, so a closed application never loses the file.

  While the application runs, its Settings page owns the capture switch, the
  ask switch, the file types and the site list. The small local fallback
  (`takeover`) is used only when the application cannot be reached.
*/

const BRIDGE = "http://127.0.0.1:27500";
const CLIENT = "browser-extension";
const APP = "b-download-manager";

/** Downloads already handed over, so a repeated event cannot send twice. */
const handedOver = new Set();

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
