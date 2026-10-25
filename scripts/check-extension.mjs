// Self-check and unit tests for the Firefox extension.
//
// The extension is plain scripts, so the checks run the files the browser
// loads, not copies: the manifest is read, every page reference is resolved,
// the settings and media helpers are executed in a sandbox, and the background
// page is loaded against a mocked browser and a mocked loopback bridge. Run it
// with `npm run check:extension`.

import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { pathToFileURL, fileURLToPath } from "node:url";
import path from "node:path";
import vm from "node:vm";

const rootDir = fileURLToPath(new URL("../extension-firefox/", import.meta.url));
const rootUrl = pathToFileURL(rootDir + path.sep);
const read = (file) => readFileSync(new URL(file, rootUrl), "utf8");
const exists = (file) => existsSync(new URL(file, rootUrl));

/* ------------------------------------------------------------- manifest */

const manifest = JSON.parse(read("manifest.json"));

assert.equal(manifest.manifest_version, 3, "Firefox uses MV3 here");
assert.ok(!manifest.background.service_worker, "Firefox has no MV3 service worker");
assert.equal(
  manifest.background.scripts.at(-1),
  "background.js",
  "the background entry runs last",
);
assert.deepEqual(
  manifest.background.scripts.slice(0, 3),
  ["lib/media.js", "lib/settings.js", "lib/bridge.js"],
  "the background gets its media, settings and bridge helpers first",
);

assert.deepEqual(
  [...manifest.permissions].sort(),
  ["contextMenus", "downloads", "storage"],
  "the extension asks for the three permissions it uses, and no more",
);
assert.deepEqual(
  manifest.host_permissions,
  ["http://127.0.0.1:27500/*"],
  "the only host permission is the loopback bridge",
);
assert.ok(!manifest.permissions.includes("scripting"));
assert.ok(!manifest.permissions.includes("activeTab"));
assert.ok(!manifest.permissions.includes("tabs"));

assert.equal(manifest.browser_specific_settings.gecko.id, "idm-bridge@my-idm-clone.local");
assert.ok(Number(manifest.version.split(".")[0]) >= 1, "the upgrade is a major version");

for (const file of [
  ...manifest.background.scripts,
  manifest.action.default_popup,
  manifest.options_ui.page,
  ...Object.values(manifest.action.default_icon),
  ...Object.values(manifest.icons),
]) {
  assert.ok(exists(file), `${file} is in the manifest but not in the folder`);
}

const contentScripts = manifest.content_scripts ?? [];
assert.equal(contentScripts.length, 1, "one content script");
const contentScript = contentScripts[0];
assert.deepEqual(contentScript.matches, ["http://*/*", "https://*/*"]);
assert.equal(contentScript.run_at, "document_idle");
assert.ok(!contentScript.all_frames, "the overlay runs in the top frame only");
assert.ok(contentScript.js.includes("content/overlay.js"), "the overlay is injected");
for (const file of contentScript.js) {
  assert.ok(exists(file), `${file} is in the manifest but not in the folder`);
}

/* ------------------------------------------------------------ every file */

function everyFile(dir, suffix) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const full = path.join(dir, name);
    if (statSync(full).isDirectory()) out.push(...everyFile(full, suffix));
    else if (full.endsWith(suffix)) out.push(full);
  }
  return out;
}

for (const file of everyFile(rootDir, ".js")) {
  // Parsing only: the scripts need a browser, a page or both to run.
  new vm.Script(readFileSync(file, "utf8"), { filename: file });
}

for (const file of everyFile(rootDir, ".css")) {
  const text = readFileSync(file, "utf8");
  const open = (text.match(/{/g) || []).length;
  const close = (text.match(/}/g) || []).length;
  assert.equal(open, close, `${path.relative(rootDir, file)} has balanced braces`);
}

/* -------------------------------------------------------------- the pages */

for (const page of [manifest.action.default_popup, manifest.options_ui.page]) {
  const htmlPath = new URL(page, rootUrl);
  const html = readFileSync(htmlPath, "utf8");

  assert.ok(
    !/<script(?![^>]*\bsrc=)[^>]*>[\s\S]*?<\/script>/i.test(html),
    `${page} has no inline script (the extension CSP would refuse it)`,
  );
  assert.ok(!/\son[a-z]+\s*=/i.test(html), `${page} has no inline event handler`);

  for (const match of html.matchAll(/(?:src|href)="([^"]+)"/g)) {
    const ref = match[1];
    if (ref.startsWith("#") || ref.startsWith("data:")) continue;
    assert.ok(!/^https?:/i.test(ref), `${page} loads ${ref} from the network`);
    const target = new URL(ref, htmlPath);
    assert.ok(existsSync(fileURLToPath(target)), `${page} -> ${ref} exists`);
  }
}

/* ----------------------------------------------------------- theme tokens */

function cssVariables(text, selector) {
  const start = text.indexOf(selector);
  assert.ok(start >= 0, `the stylesheet has ${selector}`);
  const open = text.indexOf("{", start);
  const close = text.indexOf("}", open);
  const body = text.slice(open + 1, close);
  const map = new Map();
  for (const match of body.matchAll(/--([a-z0-9-]+):\s*([^;]+);/gi)) {
    map.set(match[1].toLowerCase(), match[2].replace(/\s+/g, " ").trim().toLowerCase());
  }
  return map;
}

const themeCss = read("theme.css");
const themeDark = cssVariables(themeCss, ":root {");
const themeLight = cssVariables(themeCss, ':root[data-bdm-theme="light"] {');

const overlayContext = vm.createContext({});
vm.runInContext(read("content/overlay-style.js"), overlayContext, {
  filename: "overlay-style.js",
});
const overlayCss = vm.runInContext("BDM_OVERLAY_STYLE", overlayContext);
const overlayDark = cssVariables(overlayCss, ":host {");
const overlayLight = cssVariables(overlayCss, ':host([data-theme="light"]) {');

const shared = (overlayName, themeName) => [overlayName, themeName];

const darkPairs = [
  shared("canvas", "canvas"),
  shared("text", "text"),
  shared("muted", "text-muted"),
  shared("line", "line"),
  shared("hover", "hover"),
  shared("active", "active"),
  shared("primary", "primary"),
  shared("on-primary", "on-primary"),
  shared("progress", "progress"),
  shared("success", "success"),
  shared("danger", "danger"),
  shared("focus", "focus"),
];

for (const [overlayName, themeName] of darkPairs) {
  assert.equal(
    overlayDark.get(`bdm-${overlayName}`),
    themeDark.get(themeName),
    `the dark overlay token --bdm-${overlayName} matches theme.css --${themeName}`,
  );
}

const lightPairs = [
  // The floating panel is a raised surface over a video, so its canvas is the
  // light surface, not the window canvas behind it.
  shared("canvas", "surface"),
  ...darkPairs.slice(1),
];

for (const [overlayName, themeName] of lightPairs) {
  assert.equal(
    overlayLight.get(`bdm-${overlayName}`),
    themeLight.get(themeName),
    `the light overlay token --bdm-${overlayName} matches theme.css --${themeName}`,
  );
}

/* ------------------------------------------------------------- icon names */

const iconContext = vm.createContext({});
vm.runInContext(read("lib/icons.js"), iconContext, { filename: "icons.js" });
const iconNames = vm.runInContext("Object.keys(BDM_ICON_SHAPES)", iconContext);

for (const file of ["popup/popup.js", "options/options.js", "content/overlay.js"]) {
  for (const match of read(file).matchAll(/bdmIcon\("([a-z0-9-]+)"/g)) {
    assert.ok(
      iconNames.includes(match[1]),
      `${file} asks for the icon "${match[1]}", which lib/icons.js does not draw`,
    );
  }
}
assert.ok(
  read("popup/popup.html").includes("../lib/icons.js"),
  "the popup loads the icon helper it calls",
);

/* --------------------------------------------------------------- sandbox */

const store = {};
function mockBrowser() {
  const listeners = new Map();
  const calls = {
    menus: [],
    badges: [],
    cancelled: [],
    erased: [],
    fetches: [],
    openedOptions: 0,
  };

  const add = (name) => (fn) => {
    if (!listeners.has(name)) listeners.set(name, []);
    listeners.get(name).push(fn);
  };

  const browser = {
    contextMenus: {
      removeAll: async () => {},
      create: (item) => calls.menus.push(item),
      onClicked: { addListener: add("menu-clicked") },
    },
    runtime: {
      onInstalled: { addListener: add("installed") },
      onStartup: { addListener: add("startup") },
      onMessage: { addListener: add("message") },
      openOptionsPage: async () => {
        calls.openedOptions += 1;
      },
    },
    downloads: {
      onCreated: { addListener: add("created") },
      cancel: async (id) => calls.cancelled.push(id),
      erase: async (query) => calls.erased.push(query),
    },
    action: {
      setBadgeBackgroundColor: async () => {},
      setBadgeTextColor: async () => {},
      setBadgeText: async (value) => calls.badges.push(value),
    },
    tabs: {
      onUpdated: { addListener: add("updated") },
      onRemoved: { addListener: add("removed") },
    },
    storage: {
      local: {
        get: async (defaults) => ({ ...defaults, ...store }),
        set: async (value) => Object.assign(store, value),
      },
      onChanged: { addListener: add("storage") },
    },
  };

  return { browser, listeners, calls };
}

const sandbox = mockBrowser();
let bridgeMode = "online";
let pingApp = "b-download-manager";
let listPayload = { downloads: [], active: 0 };
let capturePolicy = { capture: true, ask: false, types: [], skipSites: [] };
const lastSend = { body: null };

async function fakeFetch(url, options = {}) {
  sandbox.calls.fetches.push({ url, options });
  if (bridgeMode === "offline") throw new TypeError("Failed to fetch");

  const pathName = new URL(String(url)).pathname;
  let data;
  if (pathName === "/v1/ping") {
    data = { app: pingApp, api: 1, version: "0.1.0", port: 27500 };
  } else if (pathName === "/v1/capture") {
    data = capturePolicy;
  } else if (pathName === "/v1/downloads" && options.method === "GET") {
    data = listPayload;
  } else if (pathName === "/v1/downloads" && options.method === "POST") {
    lastSend.body = JSON.parse(options.body);
    data = { mode: "direct", id: 42, name: "a.bin", dest: "C:\\Downloads\\a.bin" };
  } else if (pathName.startsWith("/v1/downloads/")) {
    data = { id: 42, action: pathName.split("/").pop(), status: null };
  } else {
    data = { focused: true };
  }

  return {
    status: 200,
    async text() {
      return JSON.stringify({ ok: true, data });
    },
  };
}

/** The hand-over requests, so a test can tell a filtered download from a send. */
const sentDownloads = () =>
  sandbox.calls.fetches.filter((call) => new URL(String(call.url)).pathname === "/v1/downloads");

// A cleanup timer in the takeover path waits 30 s; the sandbox does not need
// to keep the check alive for it.
const setTimeoutShim = (fn, delay, ...args) =>
  delay >= 10_000 ? 0 : setTimeout(fn, delay, ...args);

const backgroundContext = vm.createContext({
  URL,
  AbortController,
  setTimeout: setTimeoutShim,
  clearTimeout,
  console,
  fetch: fakeFetch,
  browser: sandbox.browser,
  performance,
});

for (const file of manifest.background.scripts) {
  vm.runInContext(read(file), backgroundContext, { filename: file });
}

const backgroundLoad = vm.runInContext("BDM_DEFAULT_SETTINGS", backgroundContext);
assert.equal(backgroundLoad.intercept, true);
assert.equal(backgroundLoad.overlay, true);
assert.equal(backgroundLoad.ask, false);

/* ------------------------------------------------------------ settings.js */

const {
  bdmNormalizeSettings,
  bdmShouldIntercept,
  bdmCleanSite,
  bdmCleanType,
  bdmLoadSettings,
  bdmAllowsOverlay,
} = backgroundContext;

const defaults = bdmNormalizeSettings({});
assert.equal(defaults.intercept, true, "interception is on by default");
assert.equal(defaults.overlay, true, "the video overlay is on by default");
assert.equal(defaults.ask, false, "a download starts without asking by default");
assert.equal(defaults.theme, "auto");
assert.deepEqual([...defaults.types], []);
assert.deepEqual([...defaults.skipSites], []);

const legacy = bdmNormalizeSettings({ takeover: false, panel: false });
assert.equal(legacy.intercept, false, "the old takeover switch is understood");
assert.equal(legacy.overlay, false, "the old panel switch is understood");

const typed = bdmNormalizeSettings({
  intercept: true,
  types: [".MP4", "mp4", " mkv ", "*.zip", ""],
  skipSites: ["https://Example.org/some/page", "example.net", "not a host"],
  theme: "dark",
});
assert.deepEqual([...typed.types], ["mp4", "mkv", "zip"], "types are cleaned and deduped");
assert.deepEqual([...typed.skipSites], ["example.org", "example.net"], "a site is a host");
assert.equal(typed.theme, "dark");
assert.equal(bdmNormalizeSettings({ theme: "sepia" }).theme, "auto", "an unknown theme falls back");

assert.ok(bdmShouldIntercept(defaults, "https://example.org/a.bin", "https://example.org/page"));
assert.ok(!bdmShouldIntercept(defaults, "blob:https://example.org/abc", "https://example.org/"));
assert.ok(!bdmShouldIntercept({ ...defaults, intercept: false }, "https://example.org/a.bin", ""));

const filtered = { ...defaults, types: ["mp4"], skipSites: ["example.org"] };
assert.ok(bdmShouldIntercept(filtered, "https://files.org/a.mp4", "https://files.org/"));
assert.ok(!bdmShouldIntercept(filtered, "https://files.org/a.exe", "https://files.org/"));
assert.ok(!bdmShouldIntercept(filtered, "https://example.org/a.mp4", "https://example.org/"));
assert.ok(!bdmShouldIntercept(filtered, "https://cdn.example.org/a.mp4", "https://www.example.org/"));
assert.ok(bdmShouldIntercept(filtered, "https://files.org/a.mp4", "https://notexample.org/"));
assert.ok(!bdmShouldIntercept(filtered, "https://example.org/a.mp4", ""));

assert.equal(bdmCleanSite("HTTP://USER:PASS@Example.org:8080/x"), "example.org");
assert.equal(bdmCleanSite("*.example.org"), "example.org");
assert.equal(bdmCleanSite("localhost"), "localhost");
assert.equal(bdmCleanSite("127.0.0.1"), "127.0.0.1");
assert.equal(bdmCleanSite("not a host"), "");
assert.equal(bdmCleanType("*.MP4"), "mp4");

assert.ok(bdmAllowsOverlay(defaults, "example.org"), "the overlay is on by default");
assert.ok(!bdmAllowsOverlay({ ...defaults, overlay: false }, "example.org"));
assert.ok(
  !bdmAllowsOverlay({ ...defaults, skipSites: ["example.org"] }, "cdn.example.org"),
  "an excluded site hides the overlay on its subdomains too",
);
assert.ok(
  bdmAllowsOverlay({ ...defaults, skipSites: ["example.org"] }, "notexample.org"),
  "an excluded site does not match a host that merely ends the same way",
);

store.intercept = false;
assert.equal((await bdmLoadSettings()).intercept, false, "the saved settings are read back");
delete store.intercept;

/* ---------------------------------------------------------------- media.js */

const mediaContext = vm.createContext({ URL, console });
vm.runInContext(read("lib/media.js"), mediaContext, { filename: "media.js" });
const {
  bdmIsDownloadableSource,
  bdmIsMediaLink,
  bdmIsStreamUrl,
  bdmNameFromUrl,
  bdmSourceLabel,
  bdmPercentOf,
  bdmFormatSize,
  bdmFileNameOfPath,
} = mediaContext;

assert.ok(bdmIsDownloadableSource("https://example.org/a.mp4"));
assert.ok(!bdmIsDownloadableSource("blob:https://example.org/abc"));
assert.ok(!bdmIsDownloadableSource("https://example.org/live.m3u8"));
assert.ok(!bdmIsDownloadableSource("https://example.org/live.mpd"));
assert.ok(!bdmIsDownloadableSource("javascript:alert(1)"));
assert.ok(bdmIsStreamUrl("https://example.org/live.m3u8?token=1"));
assert.ok(bdmIsMediaLink("https://example.org/a.mp4?x=1"));
assert.ok(!bdmIsMediaLink("https://example.org/page"));
assert.equal(bdmNameFromUrl("https://example.org/My%20Film.mp4?sig=1"), "My Film.mp4");
assert.equal(bdmSourceLabel("https://example.org/a.webm?x=1"), "WEBM");
assert.equal(bdmSourceLabel("https://example.org/videoplayback?id=1"), "FILE");
assert.equal(bdmPercentOf({ total: 200, downloaded: 50, status: "running" }), 25);
assert.equal(bdmPercentOf({ total: 0, downloaded: 50, status: "running" }), 0);
assert.equal(bdmPercentOf({ total: 200, downloaded: 200, status: "completed" }), 100);
assert.equal(bdmFormatSize(1536), "1.50 KB");
assert.equal(bdmFileNameOfPath("C:\\Users\\me\\Downloads\\a.mp4"), "a.mp4");
assert.equal(bdmFileNameOfPath(""), "download");

/* --------------------------------------------------------------- bridge.js */

const requestContext = vm.createContext({
  URL,
  AbortController,
  setTimeout,
  clearTimeout,
  console,
  fetch: fakeFetch,
});
vm.runInContext(read("lib/bridge.js"), requestContext, { filename: "bridge.js" });

const {
  bdmBridgePing,
  bdmBridgeList,
  bdmBridgeSend,
  bdmBridgeControl,
  bdmBridgeFocus,
  bdmBridgeRequest,
} = requestContext;

bridgeMode = "online";
pingApp = "b-download-manager";
const ping = await bdmBridgePing();
assert.equal(ping.app, "b-download-manager");
assert.equal(ping.api, 1);

const headers = sandbox.calls.fetches.at(-1).options.headers;
assert.equal(headers["X-BDM-Client"], "browser-extension", "every request carries the client header");

listPayload = {
  downloads: [{ id: 1, status: "running", downloaded: 10, total: 20 }],
  active: 1,
};
const list = await bdmBridgeList();
assert.equal(list.downloads[0].id, 1);
assert.equal(list.active, 1);

await bdmBridgeSend({ url: "https://example.org/a.bin", name: "a.bin", mode: "direct" });
assert.equal(lastSend.body.mode, "direct");
assert.equal(lastSend.body.url, "https://example.org/a.bin");
assert.equal(lastSend.body.referer, "");

await bdmBridgeSend({ url: "https://example.org/a.bin", mode: "ask" });
assert.equal(lastSend.body.mode, "ask");

await bdmBridgeControl(42, "pause");
assert.equal(
  sandbox.calls.fetches.at(-1).url,
  "http://127.0.0.1:27500/v1/downloads/42/pause",
);
await bdmBridgeFocus();
assert.equal(sandbox.calls.fetches.at(-1).url, "http://127.0.0.1:27500/v1/focus");

pingApp = "something-else";
await assert.rejects(bdmBridgePing(), (error) => error.kind === "protocol");
pingApp = "b-download-manager";

bridgeMode = "offline";
await assert.rejects(bdmBridgeList(), (error) => error.kind === "offline");
bridgeMode = "online";

const badJson = vm.createContext({
  URL,
  AbortController,
  setTimeout,
  clearTimeout,
  console,
  fetch: async () => ({ status: 200, text: async () => "not json" }),
});
vm.runInContext(read("lib/bridge.js"), badJson, { filename: "bridge.js" });
const errorJson = await badJson.bdmBridgeList().catch((error) => error);
assert.ok(errorJson, "a non-JSON answer is an error");
assert.equal(errorJson.kind, "protocol");

const refused = vm.createContext({
  URL,
  AbortController,
  setTimeout,
  clearTimeout,
  console,
  fetch: async () => ({
    status: 409,
    text: async () =>
      JSON.stringify({ ok: false, error: { code: "conflict", message: "not running" } }),
  }),
});
vm.runInContext(read("lib/bridge.js"), refused, { filename: "bridge.js" });
const refusedError = await refused.bdmBridgeControl(1, "pause").catch((error) => error);
assert.equal(refusedError.kind, "refused");
assert.equal(refusedError.code, "conflict");
assert.equal(refusedError.message, "not running");

const aborted = vm.createContext({
  URL,
  AbortController,
  setTimeout,
  clearTimeout,
  console,
  fetch: async () => {
    const error = new Error("aborted");
    error.name = "AbortError";
    throw error;
  },
});
vm.runInContext(read("lib/bridge.js"), aborted, { filename: "bridge.js" });
const timeoutError = await aborted.bdmBridgeList().catch((error) => error);
assert.equal(timeoutError.kind, "timeout");

/* ----------------------------------------------------------- background.js */

// The background was loaded before these tests, register the listeners that
// were captured from it.
const onMessage = sandbox.listeners.get("message")[0];
assert.ok(onMessage, "the background listens for messages");

bridgeMode = "online";
pingApp = "b-download-manager";
const statusAnswer = await onMessage({ type: "bdm:status" }, {});
assert.equal(statusAnswer.ok, true);
assert.equal(statusAnswer.version, "0.1.0");

listPayload = { downloads: [], active: 0 };
const listAnswer = await onMessage({ type: "bdm:list" }, {});
assert.equal(listAnswer.ok, true);
assert.deepEqual([...listAnswer.downloads], []);

sandbox.calls.fetches.length = 0;
const sendAnswer = await onMessage(
  { type: "bdm:send", payload: { url: "https://example.org/a.bin", name: "a.bin" } },
  {},
);
assert.equal(sendAnswer.ok, true);
assert.equal(sendAnswer.result.mode, "direct");
assert.equal(lastSend.body.referer, "");

store.ask = true;
capturePolicy = { ...capturePolicy, ask: true };
await onMessage(
  { type: "bdm:send", payload: { url: "https://example.org/b.bin", referer: "https://example.org/" } },
  {},
);
assert.equal(lastSend.body.mode, "ask", "the app's ask setting reaches the bridge");
capturePolicy = { ...capturePolicy, ask: false };
delete store.ask;

sandbox.calls.fetches.length = 0;
const badSend = await onMessage({ type: "bdm:send", payload: { url: "javascript:alert(1)" } }, {});
assert.equal(badSend.ok, false);
assert.equal(badSend.error.kind, "refused");
assert.equal(sandbox.calls.fetches.length, 0, "a non-download is not sent");

const badControl = await onMessage({ type: "bdm:control", id: 1, action: "explode" }, {});
assert.equal(badControl.ok, false);
const badId = await onMessage({ type: "bdm:control", id: "x", action: "pause" }, {});
assert.equal(badId.ok, false);

const controlAnswer = await onMessage({ type: "bdm:control", id: 42, action: "pause" }, {});
assert.equal(controlAnswer.ok, true);

bridgeMode = "offline";
const offlineStatus = await onMessage({ type: "bdm:status" }, {});
assert.equal(offlineStatus.ok, false);
assert.equal(offlineStatus.error.kind, "offline");
bridgeMode = "online";

const countAnswer = await onMessage({ type: "bdm:media-count", count: 3 }, { tab: { id: 9 } });
assert.equal(countAnswer.ok, true);
assert.ok(
  sandbox.calls.badges.some((badge) => badge.tabId === 9 && badge.text === "3"),
  "the media count lands on the toolbar badge",
);

// A download the browser starts is handed over before the browser copy is
// cancelled; a failure leaves the browser copy alone. While the app runs, its
// policy wins over the extension's local copy.
const onCreated = sandbox.listeners.get("created")[0];
assert.ok(onCreated, "the background watches browser downloads");

store.intercept = false;
sandbox.calls.cancelled.length = 0;
sandbox.calls.erased.length = 0;
await onCreated({
  id: 7,
  url: "https://example.org/movie.mp4",
  filename: "C:\\Users\\me\\Downloads\\movie.mp4",
  referrer: "https://example.org/",
});
assert.deepEqual([...sandbox.calls.cancelled], [7], "the browser copy is cancelled");
assert.deepEqual(
  sandbox.calls.erased.map((query) => query.id),
  [7],
  "the browser copy is erased",
);
assert.equal(
  sandbox.calls.cancelled.length,
  1,
  "the app policy takes precedence over the local switch",
);
delete store.intercept;

bridgeMode = "offline";
sandbox.calls.cancelled.length = 0;
sandbox.calls.erased.length = 0;
await onCreated({
  id: 8,
  url: "https://example.org/other.mp4",
  filename: "C:\\Users\\me\\Downloads\\other.mp4",
  referrer: "https://example.org/",
});
assert.deepEqual([...sandbox.calls.cancelled], [], "a closed app never loses the browser download");
assert.deepEqual([...sandbox.calls.erased], []);
bridgeMode = "online";

// The local copy still applies when the app is not running.
store.intercept = false;
bridgeMode = "offline";
sandbox.calls.fetches.length = 0;
await onCreated({
  id: 12,
  url: "https://example.org/other.mp4",
  filename: "C:\\Users\\me\\Downloads\\other.mp4",
  referrer: "https://example.org/",
});
assert.equal(sentDownloads().length, 0, "a closed app falls back to the local switch");
bridgeMode = "online";
delete store.intercept;

capturePolicy = { ...capturePolicy, types: ["mp4"] };
sandbox.calls.fetches.length = 0;
sandbox.calls.cancelled.length = 0;
await onCreated({
  id: 9,
  url: "https://example.org/setup.exe",
  filename: "C:\\Users\\me\\Downloads\\setup.exe",
  referrer: "https://example.org/",
});
assert.equal(sentDownloads().length, 0, "a type off the app's list stays in the browser");
assert.deepEqual([...sandbox.calls.cancelled], []);
capturePolicy = { ...capturePolicy, types: [] };

capturePolicy = { ...capturePolicy, skipSites: ["example.org"] };
sandbox.calls.fetches.length = 0;
await onCreated({
  id: 10,
  url: "https://example.org/movie.mp4",
  filename: "C:\\Users\\me\\Downloads\\movie.mp4",
  referrer: "https://example.org/",
});
assert.equal(sentDownloads().length, 0, "a skipped site keeps its download");
capturePolicy = { ...capturePolicy, skipSites: [] };

capturePolicy = { ...capturePolicy, capture: false };
sandbox.calls.fetches.length = 0;
await onCreated({
  id: 11,
  url: "https://example.org/movie.mp4",
  filename: "C:\\Users\\me\\Downloads\\movie.mp4",
  referrer: "https://example.org/",
});
assert.equal(sentDownloads().length, 0, "the app's switch turns interception off");
capturePolicy = { ...capturePolicy, capture: true };

// The menus are built on install and startup, from an empty slate.
const install = sandbox.listeners.get("installed")[0];
assert.ok(install, "the menus are built on install");
sandbox.calls.menus.length = 0;
await install();
await new Promise((resolve) => setImmediate(resolve));
assert.ok(sandbox.calls.menus.length >= 4, "the menu holds the hand-over and the app actions");
assert.ok(sandbox.calls.menus.some((item) => item.id === "bdm-download"));
assert.ok(sandbox.calls.menus.some((item) => item.id === "bdm-open"));
assert.ok(sandbox.calls.menus.some((item) => item.id === "bdm-settings"));

// The menu hand-over goes through the same bridge, even on a skipped site.
capturePolicy = { ...capturePolicy, ask: false, skipSites: ["example.org"] };
sandbox.calls.fetches.length = 0;
const onMenu = sandbox.listeners.get("menu-clicked")[0];
assert.ok(onMenu, "the menu answers clicks");
await onMenu({
  menuItemId: "bdm-download",
  linkUrl: "https://example.org/file.zip",
  pageUrl: "https://example.org/page",
  tab: { id: 3 },
});
assert.equal(sentDownloads().length, 1, "a menu click is sent even on a skipped site");
assert.equal(
  new URL(String(sandbox.calls.fetches.at(-1).url)).pathname,
  "/v1/downloads",
  "the capture policy is read first, then the hand-over",
);
assert.equal(lastSend.body.url, "https://example.org/file.zip");
assert.equal(lastSend.body.referer, "https://example.org/page");
capturePolicy = { ...capturePolicy, skipSites: [] };

// Opening the settings from the menu works.
sandbox.calls.openedOptions = 0;
await onMenu({ menuItemId: "bdm-settings", tab: { id: 3 } });
assert.equal(sandbox.calls.openedOptions, 1);

/* ------------------------------------------------------- chrome build */

// The Chrome build talks to the same versioned loopback API. It cannot share
// the Firefox background, so the contract and the "never cancel before the
// hand-over" rule are checked here.
const chromeRootDir = fileURLToPath(new URL("../browser-extension/", import.meta.url));
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

const chromeListeners = new Map();
const chromeCalls = [];
const chromeStore = {};
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
  },
  storage: {
    local: { get: async (defaults) => ({ ...defaults, ...chromeStore }) },
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
  ["fetch", "fetch", "cancel", "erase"],
  "chrome: the hand-over happens before the browser copy is cancelled",
);
assert.equal(new URL(chromeCalls[0][1]).pathname, "/v1/capture");
assert.equal(
  new URL(chromeCalls[1][1]).pathname,
  "/v1/downloads",
  "chrome: the app's capture rules are read first",
);
assert.equal(chromeCalls[1][2].headers["X-BDM-Client"], "browser-extension");
assert.equal(JSON.parse(chromeCalls[1][2].body).mode, "direct");

chromeOnline = false;
chromeCalls.length = 0;
await chromeCreated({
  id: 2,
  url: "https://example.org/b.mp4",
  referrer: "https://example.org/",
});
assert.deepEqual(
  chromeCalls.map((call) => call[0]),
  ["fetch", "fetch"],
  "chrome: a closed app leaves the browser download alone",
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
  ["fetch"],
  "chrome: the app's switch turns capture off",
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
  ["fetch"],
  "chrome: the local switch applies while the app is closed",
);
delete chromeStore.takeover;
chromeOnline = true;

console.log("extension: all checks pass");
