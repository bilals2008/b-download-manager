// The Chrome build: the same loopback contract, the freeze-before-hand-over
// rule, and the webRequest media capture.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import vm from "node:vm";
import { pngSize, setTimeoutShim, until } from "./common.mjs";

// The Chrome build talks to the same versioned loopback API. It cannot share
// the Firefox background, so the contract and the "never cancel before the
// hand-over" rule are checked here.
const chromeRootDir = fileURLToPath(new URL("../../browser-extension/", import.meta.url));
const chromeRead = (file) => readFileSync(path.join(chromeRootDir, file), "utf8");
const chromeManifest = JSON.parse(chromeRead("manifest.json"));

assert.equal(chromeManifest.manifest_version, 3);
for (const file of [
  chromeManifest.background.service_worker,
  chromeManifest.action.default_popup,
  ...Object.values(chromeManifest.action.default_icon),
  ...Object.values(chromeManifest.icons),
]) {
  assert.ok(existsSync(path.join(chromeRootDir, file)), `chrome: ${file} exists`);
}

for (const [size, file] of Object.entries({
  ...chromeManifest.icons,
  ...chromeManifest.action.default_icon,
})) {
  const pixels = pngSize(readFileSync(path.join(chromeRootDir, file)));
  assert.deepEqual(
    pixels,
    { width: Number(size), height: Number(size) },
    `chrome: ${file} should be ${size}x${size}`,
  );
}

const chromeListeners = new Map();
const chromeCalls = [];
const chromeStore = {};
const chromeTabs = new Map();
const chromeCookies = new Map();
const chromeBadges = [];
let chromeOnline = true;
let chromePolicy = { capture: true, ask: false, types: [], skipSites: [] };

const chromeBrowser = {
  runtime: {
    onInstalled: { addListener: (fn) => chromeListeners.set("installed", fn) },
    onStartup: { addListener: (fn) => chromeListeners.set("startup", fn) },
  },
  contextMenus: {
    removeAll: (callback) => callback?.(),
    create: () => {},
    onClicked: { addListener: (fn) => chromeListeners.set("menu-clicked", fn) },
  },
  downloads: {
    onCreated: { addListener: (fn) => chromeListeners.set("created", fn) },
    cancel: async (id) => chromeCalls.push(["cancel", id]),
    erase: async (query) => chromeCalls.push(["erase", query]),
    pause: async (id) => chromeCalls.push(["pause", id]),
    resume: async (id) => chromeCalls.push(["resume", id]),
  },
  storage: {
    local: { get: async (defaults) => ({ ...defaults, ...chromeStore }) },
  },
  action: {
    setBadgeBackgroundColor: async () => {},
    setBadgeText: async ({ tabId, text }) => chromeBadges.push([tabId, text]),
  },
  tabs: {
    onUpdated: { addListener: (fn) => chromeListeners.set("tab-updated", fn) },
    onRemoved: { addListener: (fn) => chromeListeners.set("tab-removed", fn) },
    get: async (tabId) => {
      const tab = chromeTabs.get(tabId);
      if (!tab) throw new Error(`no tab ${tabId}`);
      return tab;
    },
  },
  cookies: {
    getAll: async ({ url }) => chromeCookies.get(url) || [],
  },
  webRequest: {
    onCompleted: {
      addListener: (fn, filter) => chromeListeners.set("media-completed", { fn, filter }),
    },
  },
};

async function chromeFetch(url, options = {}) {
  chromeCalls.push(["fetch", url, options]);
  if (!chromeOnline) throw new TypeError("Failed to fetch");
  const pathName = new URL(String(url)).pathname;
  const data = pathName === "/v1/capture" ? chromePolicy : { mode: "direct", id: 7 };
  return { json: async () => ({ ok: true, data }) };
}

const chromeContext = vm.createContext({
  URL,
  setTimeout: setTimeoutShim,
  clearTimeout,
  console,
  navigator: { userAgent: "Mozilla/5.0 (check-extension)" },
  fetch: chromeFetch,
  chrome: chromeBrowser,
});
vm.runInContext(chromeRead("background.js"), chromeContext, {
  filename: "browser-extension/background.js",
});

const chromeCreated = chromeListeners.get("created");
assert.ok(chromeCreated, "chrome: the background watches browser downloads");

chromeCalls.length = 0;
await chromeCreated({
  id: 1,
  url: "https://example.org/a.mp4",
  referrer: "https://example.org/",
});
assert.deepEqual(
  chromeCalls.map((call) => call[0]),
  ["pause", "fetch", "fetch", "cancel", "erase"],
  "chrome: the copy is frozen first and the hand-over happens before it is cancelled",
);
assert.equal(new URL(chromeCalls[1][1]).pathname, "/v1/capture");
assert.equal(
  new URL(chromeCalls[2][1]).pathname,
  "/v1/downloads",
  "chrome: the app's capture rules are read first",
);
assert.equal(chromeCalls[2][2].headers["X-BDM-Client"], "browser-extension");
assert.equal(JSON.parse(chromeCalls[2][2].body).mode, "direct");

chromeOnline = false;
chromeCalls.length = 0;
await chromeCreated({
  id: 2,
  url: "https://example.org/b.mp4",
  referrer: "https://example.org/",
});
assert.deepEqual(
  chromeCalls.map((call) => call[0]),
  ["pause", "fetch", "fetch", "resume"],
  "chrome: a closed app leaves the browser download alone, thawed again",
);
chromeOnline = true;

chromePolicy = { ...chromePolicy, capture: false };
chromeCalls.length = 0;
await chromeCreated({
  id: 3,
  url: "https://example.org/c.mp4",
  referrer: "https://example.org/",
});
assert.deepEqual(
  chromeCalls.map((call) => call[0]),
  ["pause", "fetch", "resume"],
  "chrome: the app's switch turns capture off and the copy runs again",
);
chromePolicy = { ...chromePolicy, capture: true };

chromeOnline = false;
chromeStore.takeover = false;
chromeCalls.length = 0;
await chromeCreated({
  id: 4,
  url: "https://example.org/d.mp4",
  referrer: "https://example.org/",
});
assert.deepEqual(
  chromeCalls.map((call) => call[0]),
  ["pause", "fetch", "resume"],
  "chrome: the local switch applies while the app is closed",
);
delete chromeStore.takeover;
chromeOnline = true;

/* ------------------------------------------------- chrome media capture

   The webRequest callback used to name its argument `request`, which hid the
   `request()` bridge helper, so every media registration threw and was
   swallowed. These checks fire the real listener: a registration must reach
   /v1/media, it must be remembered only once the bridge accepted it, and a
   closed app must leave it retryable. */

const chromeMedia = chromeListeners.get("media-completed");
assert.ok(chromeMedia, "chrome: the background watches completed requests");
assert.ok(
  chromeMedia.filter?.urls?.includes("<all_urls>"),
  "chrome: media capture is not narrowed to one site",
);
const chromeFireMedia = (details) => {
  chromeMedia.fn(details);
  return new Promise((resolve) => setTimeout(resolve, 0));
};
const chromeMediaPosts = () =>
  chromeCalls.filter(
    (call) => call[0] === "fetch" && new URL(call[1]).pathname === "/v1/media",
  );

chromeTabs.set(9, {
  id: 9,
  url: "https://www.youtube.com/watch?v=abc",
  title: "A video",
});
const mediaUrl =
  "https://rr1.googlevideo.com/videoplayback?itag=137&id=abc&mime=video%2Fmp4&expire=1&sig=secret";
chromeCookies.set(mediaUrl, [{ name: "SID", value: "signed-in" }]);

chromeCalls.length = 0;
await chromeFireMedia({ url: mediaUrl, type: "xmlhttprequest", tabId: 9 });
await until(() => chromeMediaPosts().length > 0);
const mediaPost = chromeMediaPosts()[0];
assert.ok(mediaPost, "chrome: a player's media address reaches the bridge");
assert.equal(mediaPost[2].method, "POST");
assert.equal(mediaPost[2].headers["X-BDM-Client"], "browser-extension");
const mediaBody = JSON.parse(mediaPost[2].body);
assert.equal(mediaBody.kind, "youtube");
assert.equal(mediaBody.pageUrl, "https://www.youtube.com/watch?v=abc");
assert.equal(mediaBody.referer, "https://www.youtube.com/watch?v=abc");
assert.equal(mediaBody.presentation, "window");
assert.equal(mediaBody.headers.Cookie, "SID=signed-in");
assert.equal(mediaBody.headers["User-Agent"], "Mozilla/5.0 (check-extension)");
assert.deepEqual(chromeBadges.at(-1), [9, "1"], "chrome: the count is shown on the button");

// The same stream reported again (a range request, a second segment) is not
// sent twice.
chromeCalls.length = 0;
await chromeFireMedia({
  url: `${mediaUrl}&range=0-99`,
  type: "xmlhttprequest",
  tabId: 9,
});
await until(() => false, 5);
assert.equal(chromeMediaPosts().length, 0, "chrome: one stream is registered once");

// A closed app: the registration fails, so it must not be remembered.
chromeOnline = false;
const retriedUrl =
  "https://rr1.googlevideo.com/videoplayback?itag=251&id=abc&mime=audio%2Fwebm";
chromeCookies.set(retriedUrl, []);
chromeCalls.length = 0;
await chromeFireMedia({ url: retriedUrl, type: "xmlhttprequest", tabId: 9 });
await until(() => chromeMediaPosts().length > 0);
assert.equal(
  chromeMediaPosts().length,
  1,
  "chrome: a closed app is recorded as one failed attempt",
);

chromeOnline = true;
chromeCalls.length = 0;
await chromeFireMedia({ url: retriedUrl, type: "xmlhttprequest", tabId: 9 });
await until(() => chromeMediaPosts().length > 0);
assert.equal(
  chromeMediaPosts().length,
  1,
  "chrome: a failed registration is retried instead of being lost",
);
assert.equal(JSON.parse(chromeMediaPosts()[0][2].body).kind, "youtube");

// A page navigation clears the tab's memory so the next watch registers.
chromeCalls.length = 0;
chromeListeners.get("tab-updated")(9, { status: "loading" });
await chromeFireMedia({ url: mediaUrl, type: "xmlhttprequest", tabId: 9 });
await until(() => chromeMediaPosts().length > 0);
assert.equal(
  chromeMediaPosts().length,
  1,
  "chrome: the same stream registers again after a navigation",
);