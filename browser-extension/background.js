/*
  The Chrome/Edge build: the same loopback API as the Firefox extension.

  Two things hand a link to the application: the right click menu, and a
  download the browser starts. A browser download is handed over before it is
  cancelled, so a closed application never loses the file.
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

async function handOver(url, referer, name) {
  return request("POST", "/v1/downloads", {
    url,
    name: name || "",
    referer: referer || "",
    mode: "direct",
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

function createMenus() {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({
      id: "idm-download",
      title: "Download with B Download Manager",
      contexts: ["link", "video", "audio", "image"],
    });
  });
}

chrome.runtime.onInstalled.addListener(createMenus);
chrome.runtime.onStartup.addListener(createMenus);

chrome.contextMenus.onClicked.addListener((info) => {
  if (info.menuItemId !== "idm-download") return;
  const url = info.linkUrl || info.srcUrl;
  if (!url) return;
  handOver(url, info.pageUrl, fileNameOf(url)).catch((error) => {
    console.warn("B Download Manager did not take the link:", error.message);
  });
});

/* The browser started a download. Hand it over before the browser copy is
   stopped: if the application is closed, the file stays in the browser. */
chrome.downloads.onCreated.addListener(async (item) => {
  if (!item || handedOver.has(item.id)) return;
  if (!/^https?:\/\//i.test(item.url)) return;

  const settings = await chrome.storage.local.get({ takeover: true });
  if (!settings.takeover) return;

  handedOver.add(item.id);
  setTimeout(() => handedOver.delete(item.id), 30000);

  try {
    await handOver(item.url, item.referrer || "", fileNameOf(item.url));
  } catch {
    // The app did not take it; the browser download goes on untouched.
    return;
  }

  await chrome.downloads.cancel(item.id).catch(() => {});
  await chrome.downloads.erase({ id: item.id }).catch(() => {});
});
