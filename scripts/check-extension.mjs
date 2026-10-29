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
  manifest.background.scripts.slice(0, 4),
  ["lib/media.js", "lib/link-rules.js", "lib/settings.js", "lib/bridge.js"],
  "the background gets its media, link, settings and bridge helpers first",
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
assert.ok(
  contentScript.js.includes("content/video-registry.js"),
  "the video registry is injected",
);
assert.ok(
  contentScript.js.includes("content/page-links.js"),
  "the page link reader is injected",
);
assert.ok(
  contentScript.js.includes("lib/link-rules.js"),
  "the link rules reach the content script that classifies links",
);
assert.ok(
  contentScript.js.indexOf("lib/link-rules.js") <
    contentScript.js.indexOf("content/page-links.js"),
  "the rules load before the reader that uses them",
);
assert.ok(
  contentScript.js.indexOf("content/video-registry.js") <
    contentScript.js.indexOf("content/overlay.js"),
  "the registry loads before the overlay that asks it for a video",
);
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

for (const page of [
  manifest.action.default_popup,
  manifest.options_ui.page,
  "links/links.html",
]) {
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

/* ------------------------------------------------ the link window's wiring */

// The window is a page of its own; a control it names but the markup does not
// have would break it only once a page of links was open.
const linksHtml = read("links/links.html");
const linkIds = new Set(
  [...linksHtml.matchAll(/\sid="([^"]+)"/g)].map((match) => match[1]),
);
for (const id of [
  "send",
  "folder",
  "queue",
  "connections",
  "category",
  "extension",
  "pages",
  "links",
  "report",
  "selection",
]) {
  assert.ok(linkIds.has(id), `links.html has #${id}`);
}
for (const match of read("links/links.js").matchAll(/getElementById\("([^"]+)"\)/g)) {
  assert.ok(
    linkIds.has(match[1]),
    `links.js asks for #${match[1]}, which links.html does not define`,
  );
}
assert.ok(
  linksHtml.includes('../lib/link-rules.js') && linksHtml.includes("links.js"),
  "the window loads the rules and its own script",
);

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
    paused: [],
    resumed: [],
    fetches: [],
    windows: [],
    tabs: [],
    sentToTab: [],
    relayed: [],
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
      getURL: (file) => `moz-extension://test/${file}`,
      sendMessage: async (message) => {
        calls.relayed.push(message);
        return { ok: true };
      },
      openOptionsPage: async () => {
        calls.openedOptions += 1;
      },
    },
    windows: {
      create: async (options) => {
        calls.windows.push(options);
        return { id: 1 };
      },
    },
    downloads: {
      onCreated: { addListener: add("created") },
      cancel: async (id) => calls.cancelled.push(id),
      erase: async (query) => calls.erased.push(query),
      pause: async (id) => {
        calls.paused.push(id);
        const left = downloadBehaviour.refusedPause.get(id) || 0;
        if (left > 0) {
          downloadBehaviour.refusedPause.set(id, left - 1);
          throw new Error("Download is not in progress");
        }
      },
      resume: async (id) => {
        calls.resumed.push(id);
        if (downloadBehaviour.failedResume.has(id)) {
          throw new Error("Download is paused");
        }
      },
    },
    action: {
      setBadgeBackgroundColor: async () => {},
      setBadgeTextColor: async () => {},
      setBadgeText: async (value) => calls.badges.push(value),
    },
    tabs: {
      onUpdated: { addListener: add("updated") },
      onRemoved: { addListener: add("removed") },
      create: async (options) => {
        calls.tabs.push(options);
        return { id: 2 };
      },
      sendMessage: async (tabId, message) => {
        calls.sentToTab.push({ tabId, message });
        return tabAnswer;
      },
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
// How the mocked downloads API behaves for specific ids: how many pauses a
// download still refuses (a transfer that has not begun yet), and which
// resumes fail. The hand-over freezes a fresh download and thaws it again
// when the app does not take it, so the tests need to drive both answers.
const downloadBehaviour = { refusedPause: new Map(), failedResume: new Set() };
let bridgeMode = "online";
let pingApp = "b-download-manager";
let listPayload = { downloads: [], active: 0 };
let capturePolicy = { capture: true, ask: false, types: [], skipSites: [] };
let tabAnswer = { ok: true, links: [], page: {}, stats: {} };
let batchOptions = {
  folders: [
    { label: "App default", path: "C:\\Users\\me\\Downloads" },
    { label: "Last used", path: "C:\\Users\\me\\Downloads\\Films" },
  ],
  queues: [
    { id: "now", label: "Start now" },
    { id: "main", label: "Main queue" },
  ],
  connections: 8,
  maxItems: 25,
};
let batchAnswer = {
  folder: "C:\\Users\\me\\Downloads",
  queue: "now",
  connections: 8,
  counts: { started: 1, queued: 0, skipped: 0, duplicates: 0, invalid: 0, refused: 0 },
  items: [
    {
      url: "https://example.org/a.zip",
      name: "a.zip",
      status: "started",
      id: 42,
      dest: "C:\\Users\\me\\Downloads\\a.zip",
      message: "",
    },
  ],
};
const lastSend = { body: null };
const lastBatch = { body: null };

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
  } else if (pathName === "/v1/batch-options") {
    data = batchOptions;
  } else if (pathName === "/v1/downloads/batch" && options.method === "POST") {
    lastBatch.body = JSON.parse(options.body);
    data = {
      ...batchAnswer,
      queue: lastBatch.body.queue,
      folder: lastBatch.body.folder || batchAnswer.folder,
    };
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

/* ------------------------------------------------------------- link-rules.js */

const {
  bdmLinkClassify,
  bdmLinkCategory,
  bdmLinkExtension,
  bdmLinkKind,
  bdmLinkKey,
  bdmLinkAppCategory,
  bdmLinkSkipLabel,
} = backgroundContext;

assert.equal(bdmLinkExtension("https://example.org/a.MP4?x=1"), "mp4");
assert.equal(bdmLinkExtension("https://example.org/get?id=1", "report.pdf"), "pdf");
assert.equal(bdmLinkExtension("https://example.org/get?id=1"), "");
assert.equal(bdmLinkExtension("https://example.org/archive.tar.gz"), "gz");
assert.equal(bdmLinkExtension("https://example.org/.env"), "");
assert.equal(
  bdmLinkExtension("https://example.org/My%20Film%2Emp4"),
  "mp4",
  "an encoded dot still names the extension",
);

assert.equal(bdmLinkCategory("zip"), "Compressed");
assert.equal(bdmLinkCategory("mp4"), "Video");
assert.equal(bdmLinkCategory("jpg"), "Images");
assert.equal(bdmLinkCategory("exe"), "Programs");
assert.equal(bdmLinkCategory("pdf"), "Documents");
assert.equal(bdmLinkCategory("opus"), "Other", "a type the app has no rule for is Other");
assert.equal(bdmLinkAppCategory("Images"), "Other", "the app's tree has no Images category");
assert.equal(bdmLinkAppCategory("Video"), "Video");

assert.equal(bdmLinkKind("zip"), "file");
assert.equal(bdmLinkKind(""), "page");
assert.equal(bdmLinkKind("html"), "page");
assert.equal(bdmLinkKind("m3u8"), "unsupported", "a manifest is not a file");
assert.equal(bdmLinkKind("ts"), "unsupported");

const zipLink = bdmLinkClassify({ href: "https://example.org/files/a.zip" }, "https://example.org/");
assert.equal(zipLink.ok, true);
assert.equal(zipLink.item.category, "Compressed");
assert.equal(zipLink.item.badge, "ZIP");
assert.equal(zipLink.item.name, "a.zip");

const namedLink = bdmLinkClassify(
  { href: "https://example.org/get?id=7", download: "report.docx" },
  "https://example.org/",
);
assert.equal(namedLink.ok, true, "the download name is what makes an extensionless link a file");
assert.equal(namedLink.item.extension, "docx");
assert.equal(namedLink.item.name, "report.docx");
assert.equal(namedLink.item.kind, "file");

const pageLink = bdmLinkClassify({ href: "https://example.org/about" }, "https://example.org/");
assert.equal(pageLink.ok, true);
assert.equal(pageLink.item.kind, "page");
assert.equal(pageLink.item.badge, "PAGE");

assert.equal(
  bdmLinkClassify({ href: "https://example.org/files/page.html#top" }, "https://example.org/files/page.html").reason,
  "self",
  "a link to this page is not a download",
);
assert.equal(
  bdmLinkClassify({ href: "https://example.org/stream.m3u8" }, "https://example.org/").reason,
  "stream",
  "a manifest the engine cannot save is reported, not offered",
);
assert.equal(
  bdmLinkClassify({ href: "mailto:someone@example.org" }, "https://example.org/").reason,
  "scheme",
);
assert.equal(
  bdmLinkClassify({ href: "javascript:alert(1)" }, "https://example.org/").reason,
  "scheme",
);
assert.equal(bdmLinkClassify({ href: "" }, "https://example.org/").reason, "unusable");

assert.equal(
  bdmLinkKey("https://example.org/a.zip#again"),
  bdmLinkKey("https://example.org/a.zip"),
  "a fragment is not a second file",
);
assert.notEqual(
  bdmLinkKey("https://example.org/a.zip?v=1"),
  bdmLinkKey("https://example.org/a.zip?v=2"),
  "a different query may be a different file",
);
assert.equal(bdmLinkSkipLabel("stream"), "stream manifests the engine cannot save");

/* ------------------------------------------------------- content/page-links.js */

const scanAnchors = [
  { href: "https://example.org/files/a.zip" },
  { href: "https://example.org/files/a.zip#again" },
  { href: "https://example.org/files/b.pdf" },
  { href: "https://example.org/files/clip.mp4" },
  { href: "https://example.org/get?id=7", download: "report.docx" },
  { href: "https://example.org/stream.m3u8" },
  { href: "mailto:someone@example.org" },
  { href: "https://example.org/files/page.html#top" },
  { href: "https://example.org/about" },
  { href: "https://example.org/files/photo.JPG" },
];

const scanMessages = [];
let scanListener;
const scanContext = vm.createContext({
  URL,
  console,
  performance,
  setTimeout,
  clearTimeout,
  location: {
    href: "https://example.org/files/page.html",
    hostname: "example.org",
  },
  document: {
    title: "Files",
    links: scanAnchors,
  },
  browser: {
    runtime: {
      sendMessage: async (message) => {
        scanMessages.push(message);
        return { ok: true };
      },
      onMessage: {
        addListener: (fn) => {
          scanListener = fn;
        },
      },
    },
  },
});
vm.runInContext(read("lib/link-rules.js"), scanContext, { filename: "link-rules.js" });
vm.runInContext(read("content/page-links.js"), scanContext, { filename: "page-links.js" });

const scan = await scanContext.bdmScanPageLinks("scan-1", () => {});
assert.equal(scan.ok, true);
assert.equal(scan.stats.scanned, scanAnchors.length);
assert.equal(scan.stats.files, 5, "five files are offered");
assert.equal(scan.stats.pages, 1, "the extensionless link is a page");
assert.equal(scan.stats.duplicates, 1, "the same address with a fragment is listed once");
assert.equal(scan.stats.skipped.stream, 1);
assert.equal(scan.stats.skipped.scheme, 1);
assert.equal(scan.stats.skipped.self, 1);
assert.equal(scan.stats.truncated, false);
assert.equal(scan.page.title, "Files");
assert.equal(scan.page.host, "example.org");
assert.equal(
  scan.links.filter((item) => item.url.endsWith("a.zip")).length,
  1,
  "a duplicate address is kept once",
);
const photo = scan.links.find((item) => item.name === "photo.JPG");
assert.equal(photo.category, "Images");
assert.equal(photo.badge, "JPG");
assert.ok(photo.key, "a link carries the key the window selects it by");
assert.ok(
  !scan.links.some((item) => item.url.startsWith("https://example.org/stream")),
  "a stream manifest is never offered",
);

// A page with thousands of links is capped, not copied whole. The anchor cap
// stops the walk; the result cap stops it earlier when every address is new.
scanContext.document.links = Array.from({ length: 9000 }, (_value, index) => ({
  href: `https://example.org/files/repeat${index % 2}.zip`,
}));
const repeated = await scanContext.bdmScanPageLinks("scan-repeat", () => {});
assert.equal(repeated.stats.total, 9000);
assert.equal(repeated.stats.scanned, 6000, "the scan stops at its anchor cap");
assert.equal(repeated.links.length, 2, "two addresses are kept however often they repeat");
assert.equal(repeated.stats.truncated, true, "a capped scan says it was capped");

scanContext.document.links = Array.from({ length: 9000 }, (_value, index) => ({
  href: `https://example.org/files/f${index}.zip`,
}));
const bigScan = await scanContext.bdmScanPageLinks("scan-big", () => {});
assert.equal(bigScan.links.length, 400, "only the first 400 addresses are kept");
assert.equal(bigScan.stats.scanned, 400, "the scan stops once the window is full");
assert.equal(bigScan.stats.truncated, true);
scanContext.document.links = scanAnchors;

const scanAnswer = await scanListener({ type: "bdm:links-scan", scanId: "scan-2" });
assert.equal(scanAnswer.ok, true);
assert.ok(
  scanMessages.some((message) => message.type === "bdm:links-progress"),
  "the reader reports its progress to the window",
);
assert.equal(scanListener({ type: "bdm:media-list" }), undefined, "other messages are left alone");

/* ---------------------------------------------------------------- media.js */

const mediaContext = vm.createContext({ URL, console });
vm.runInContext(read("lib/media.js"), mediaContext, { filename: "media.js" });
const {
  bdmIsDownloadableSource,
  bdmIsMediaLink,
  bdmIsStreamUrl,
  bdmIsCapturedMedia,
  bdmFullMediaUrl,
  bdmMediaKey,
  bdmItagOf,
  bdmMediaKind,
  bdmMediaSize,
  bdmMediaLabel,
  bdmMediaName,
  bdmMediaExtension,
  bdmMediaFileName,
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

/* A player's signed range request becomes one whole-file address per itag. */
const ranged =
  "https://rr1---sn-abc.googlevideo.com/videoplayback?id=o-abc&itag=137" +
  "&mime=video%2Fmp4&clen=24117248&range=0-1048575&rn=8&rbuf=0&sig=xyz";
const audio = "https://rr1---sn-abc.googlevideo.com/videoplayback?id=o-abc&itag=140&mime=audio%2Fwebm";

assert.ok(bdmIsCapturedMedia(ranged), "a player request is captured");
assert.ok(bdmIsCapturedMedia("https://example.org/a.mp4?x=1"), "a fetched file is captured");
assert.ok(!bdmIsCapturedMedia("https://example.org/page"));
assert.ok(!bdmIsCapturedMedia("https://example.org/live.m3u8"), "a manifest is not a file");

assert.equal(bdmItagOf(ranged), "137");
assert.equal(bdmFullMediaUrl(ranged), bdmFullMediaUrl(ranged), "the whole-file address is stable");
const full = bdmFullMediaUrl(ranged);
assert.ok(!/[?&](range|rn|rbuf)=/.test(full), "range parameters are stripped");
assert.ok(full.includes("sig=xyz"), "the signature is kept");
assert.equal(bdmMediaKey(full), bdmMediaKey(ranged), "two ranges share one key");
assert.notEqual(
  bdmMediaKey(ranged),
  bdmMediaKey(ranged.replace("itag=137", "itag=136")),
  "a different itag is a different stream",
);

assert.equal(bdmMediaKind(ranged), "video");
assert.equal(bdmMediaKind(audio), "audio");
assert.equal(bdmMediaKind(ranged.replace("itag=137", "itag=18")), "muxed", "itag 18 is picture and sound");
assert.equal(bdmMediaKind("https://example.org/a.mp4"), "", "a plain file is not a stream");
assert.equal(bdmMediaSize(ranged), 24117248);

assert.equal(bdmMediaLabel(ranged), "MP4");
assert.equal(bdmMediaLabel(audio), "AUDIO");
assert.equal(bdmMediaLabel("https://example.org/a.webm"), "WEBM");
assert.equal(bdmMediaName("https://example.org/a.mp4", "Page"), "a.mp4");
assert.equal(bdmMediaName(ranged, "Film"), "Film · 1080p video only · 23.0 MB");
assert.equal(bdmMediaName(ranged, ""), "1080p video only · 23.0 MB");
assert.equal(bdmMediaName(audio, "Film"), "Film · audio");
assert.equal(bdmMediaExtension(ranged), "mp4");
assert.equal(bdmMediaExtension(audio), "webm");
assert.equal(bdmMediaFileName(ranged, "My / Film: 1"), "My Film 1 1080p.mp4");
assert.equal(bdmMediaFileName("https://example.org/a.webm?sig=1", "Page"), "a.webm");

/* ----------------------------------------------------------- page-media.js */

const pageVideo = {
  currentSrc: "blob:https://example.org/abc",
  src: "",
  mediaKeys: null,
  querySelectorAll: () => [],
};
const pageAudio = {
  currentSrc: "https://cdn.example.org/song.mp3",
  src: "",
  mediaKeys: null,
  querySelectorAll: () => [],
};
const pageContext = vm.createContext({
  URL,
  console,
  performance: {
    getEntriesByType: () => [
      { name: ranged },
      { name: ranged.replace("range=0-1048575", "range=1048576-2097151") },
      { name: audio },
      { name: ranged.replace("itag=137", "itag=18") },
      { name: "https://example.org/app.js" },
    ],
  },
  PerformanceObserver: class {
    observe() {}
  },
  document: {
    title: "A Test Video - YouTube",
    querySelectorAll: (selector) => (selector === "video, audio" ? [pageVideo, pageAudio] : []),
  },
  location: { href: "https://example.org/watch" },
});
vm.runInContext(read("lib/media.js"), pageContext, { filename: "media.js" });
vm.runInContext(read("content/page-media.js"), pageContext, { filename: "page-media.js" });
pageContext.bdmWatchMediaRequests();

const listed = pageContext.bdmPageMedia();
assert.equal(listed.length, 4, "the song and the three distinct streams are listed");
assert.deepEqual(
  Array.from(listed, (item) => item.label),
  ["MP3", "MP4", "AUDIO", "MP4"],
);
assert.equal(listed[1].name, "A Test Video · 1080p video only · 23.0 MB");
assert.equal(listed[1].file, "A Test Video 1080p.mp4");
assert.equal(listed[2].file, "A Test Video audio.webm");
assert.ok(
  listed.every((item) => !/[?&](range|rn|rbuf)=/.test(item.url)),
  "no range request is handed over",
);
assert.equal(pageContext.bdmMediaCount(), 4, "the count matches the list");

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
  bdmBridgeBatchOptions,
  bdmBridgeBatch,
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

const options = await bdmBridgeBatchOptions();
assert.equal(options.folders[0].path, "C:\\Users\\me\\Downloads");
assert.equal(options.queues[1].id, "main");
assert.equal(options.maxItems, 25);

batchAnswer = {
  ...batchAnswer,
  counts: { started: 0, queued: 1, skipped: 0, duplicates: 0, invalid: 0, refused: 0 },
};
const batch = await bdmBridgeBatch({
  items: [{ url: "https://example.org/a.zip", name: "a.zip", category: "Compressed" }],
  folder: "C:\\Users\\me\\Downloads",
  connections: 4,
  queue: "main",
  referer: "https://example.org/page",
});
assert.equal(batch.queue, "main");
assert.equal(lastBatch.body.folder, "C:\\Users\\me\\Downloads");
assert.equal(lastBatch.body.connections, 4);
assert.equal(lastBatch.body.items[0].category, "Compressed");
assert.equal(lastBatch.body.referer, "https://example.org/page");

// An empty connection count means the app's own default, not one.
await bdmBridgeBatch({ items: [], connections: 0, folder: "" });
assert.equal(lastBatch.body.connections, null, "zero connections asks the app for its default");
assert.equal(lastBatch.body.queue, "now", "an unknown queue falls back to starting now");

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

// A video panel hand-over always asks, so its dialog can name the stream.
await onMessage(
  { type: "bdm:send", mode: "ask", payload: { url: "https://example.org/c.mp4", name: "c.mp4" } },
  {},
);
assert.equal(lastSend.body.mode, "ask", "a media hand-over opens the add dialog");

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

/* --------------------------------------------------- the link window */

sandbox.calls.windows.length = 0;
const openWindow = await onMessage({ type: "bdm:links-open", tabId: 5 }, {});
assert.equal(openWindow.ok, true);
assert.equal(sandbox.calls.windows.length, 1, "the window opens once");
assert.match(
  sandbox.calls.windows[0].url,
  /links\/links\.html\?tab=5$/,
  "the window is told which tab to read",
);
assert.equal(sandbox.calls.windows[0].type, "popup", "a compact window, not a tab");

const noTabWindow = await onMessage({ type: "bdm:links-open" }, {});
assert.equal(noTabWindow.ok, false, "a window without a tab is refused");

// A browser that will not open a popup window gets a tab instead.
const windowsCreate = sandbox.browser.windows.create;
sandbox.browser.windows.create = async () => {
  throw new Error("no popups");
};
sandbox.calls.tabs.length = 0;
const tabWindow = await onMessage({ type: "bdm:links-open", tabId: 6 }, {});
assert.equal(tabWindow.ok, true, "the window still opens as a tab");
assert.match(sandbox.calls.tabs[0].url, /\?tab=6$/);
sandbox.browser.windows.create = windowsCreate;

// The scan is asked of the page itself, through its content script.
tabAnswer = {
  ok: true,
  links: [{ url: "https://example.org/a.zip", key: "https://example.org/a.zip" }],
  page: { url: "https://example.org/", host: "example.org", title: "Home" },
  stats: { files: 1, pages: 0 },
};
sandbox.calls.sentToTab.length = 0;
const scanThroughBackground = await onMessage(
  { type: "bdm:links-scan", tabId: 5, scanId: "scan-9" },
  {},
);
assert.equal(scanThroughBackground.ok, true);
assert.equal(scanThroughBackground.links.length, 1);
assert.equal(sandbox.calls.sentToTab[0].tabId, 5);
assert.equal(sandbox.calls.sentToTab[0].message.type, "bdm:links-scan");
assert.equal(sandbox.calls.sentToTab[0].message.scanId, "scan-9");

const tabSend = sandbox.browser.tabs.sendMessage;
sandbox.browser.tabs.sendMessage = async () => {
  throw new Error("no receiver");
};
const noPage = await onMessage({ type: "bdm:links-scan", tabId: 7, scanId: "scan-9" }, {});
assert.equal(noPage.ok, false);
assert.equal(noPage.error.kind, "unavailable", "a page Firefox will not read says so");
sandbox.browser.tabs.sendMessage = tabSend;

const badScan = await onMessage({ type: "bdm:links-scan", scanId: "scan-9" }, {});
assert.equal(badScan.ok, false);

// The progress of a running scan is passed on to the window.
sandbox.calls.relayed.length = 0;
const progressed = await onMessage(
  { type: "bdm:links-progress", scanId: "scan-9", scanned: 30, total: 90, found: 4 },
  {},
);
assert.equal(progressed.ok, true);
assert.equal(sandbox.calls.relayed[0].type, "bdm:links-scan-progress");
assert.equal(sandbox.calls.relayed[0].scanId, "scan-9");
assert.equal(sandbox.calls.relayed[0].found, 4);

// The batch: the folders the app offers, then the ticked links.
const batchOptionsAnswer = await onMessage({ type: "bdm:batch-options" }, {});
assert.equal(batchOptionsAnswer.ok, true);
assert.equal(batchOptionsAnswer.folders.length, 2);
assert.equal(batchOptionsAnswer.queues[1].id, "main");

batchAnswer = {
  ...batchAnswer,
  counts: { started: 1, queued: 0, skipped: 0, duplicates: 0, invalid: 0, refused: 0 },
};
sandbox.calls.fetches.length = 0;
const batchThroughBackground = await onMessage(
  {
    type: "bdm:batch",
    payload: {
      items: [
        { url: "https://example.org/a.zip", name: "a.zip", category: "Compressed" },
        { url: "javascript:alert(1)", name: "bad" },
      ],
      folder: "C:\\Users\\me\\Downloads",
      connections: 6,
      queue: "main",
      referer: "https://example.org/page",
    },
  },
  {},
);
assert.equal(batchThroughBackground.ok, true);
assert.equal(lastBatch.body.items.length, 1, "a non-download address never reaches the app");
assert.equal(lastBatch.body.items[0].category, "Compressed");
assert.equal(lastBatch.body.queue, "main");
assert.equal(lastBatch.body.connections, 6);
assert.equal(batchThroughBackground.result.counts.started, 1);

const emptyBatch = await onMessage(
  { type: "bdm:batch", payload: { items: [{ url: "javascript:alert(1)" }] } },
  {},
);
assert.equal(emptyBatch.ok, false, "a batch of nothing valid is refused");

bridgeMode = "offline";
const offlineBatch = await onMessage({ type: "bdm:batch-options" }, {});
assert.equal(offlineBatch.ok, false);
assert.equal(offlineBatch.error.kind, "offline");
bridgeMode = "online";
tabAnswer = { ok: true, links: [], page: {}, stats: {} };

const countAnswer = await onMessage({ type: "bdm:media-count", count: 3 }, { tab: { id: 9 } });
assert.equal(countAnswer.ok, true);
assert.ok(
  sandbox.calls.badges.some((badge) => badge.tabId === 9 && badge.text === "3"),
  "the media count lands on the toolbar badge",
);

// A download the browser starts is frozen at once, so it cannot finish in the
// browser's own downloads folder while the hand-over round-trips, and it is
// only cancelled once the app has answered for the file. A refusal thaws the
// browser copy again. While the app runs, its policy wins over the extension's
// local copy.
const onCreated = sandbox.listeners.get("created")[0];
assert.ok(onCreated, "the background watches browser downloads");

store.intercept = false;
sandbox.calls.cancelled.length = 0;
sandbox.calls.erased.length = 0;
sandbox.calls.paused.length = 0;
sandbox.calls.resumed.length = 0;
await onCreated({
  id: 7,
  url: "https://example.org/movie.mp4",
  filename: "C:\\Users\\me\\Downloads\\movie.mp4",
  referrer: "https://example.org/",
});
assert.deepEqual([...sandbox.calls.paused], [7], "the browser copy is frozen before anything is decided");
assert.deepEqual([...sandbox.calls.cancelled], [7], "the browser copy is cancelled");
assert.deepEqual(
  sandbox.calls.erased.map((query) => query.id),
  [7],
  "the browser copy is erased",
);
assert.deepEqual([...sandbox.calls.resumed], [], "a taken download is not thawed");
assert.equal(
  sandbox.calls.cancelled.length,
  1,
  "the app policy takes precedence over the local switch",
);
delete store.intercept;

bridgeMode = "offline";
sandbox.calls.cancelled.length = 0;
sandbox.calls.erased.length = 0;
sandbox.calls.paused.length = 0;
sandbox.calls.resumed.length = 0;
await onCreated({
  id: 8,
  url: "https://example.org/other.mp4",
  filename: "C:\\Users\\me\\Downloads\\other.mp4",
  referrer: "https://example.org/",
});
assert.deepEqual([...sandbox.calls.cancelled], [], "a closed app never loses the browser download");
assert.deepEqual([...sandbox.calls.erased], []);
assert.deepEqual([...sandbox.calls.paused], [8]);
assert.deepEqual([...sandbox.calls.resumed], [8], "a download the closed app did not take runs again");
bridgeMode = "online";

// A pause asked before the transfer has begun is refused; the freeze retries
// once and the hand-over goes on from a frozen copy.
downloadBehaviour.refusedPause.set(13, 1);
sandbox.calls.cancelled.length = 0;
sandbox.calls.paused.length = 0;
sandbox.calls.resumed.length = 0;
await onCreated({
  id: 13,
  url: "https://example.org/retry.mp4",
  filename: "C:\\Users\\me\\Downloads\\retry.mp4",
  referrer: "https://example.org/",
});
assert.deepEqual([...sandbox.calls.paused], [13, 13], "the refused pause is retried once");
assert.deepEqual([...sandbox.calls.cancelled], [13], "the retried freeze still hands over");
assert.deepEqual([...sandbox.calls.resumed], []);
downloadBehaviour.refusedPause.clear();

// The local copy still applies when the app is not running.
store.intercept = false;
bridgeMode = "offline";
sandbox.calls.fetches.length = 0;
sandbox.calls.paused.length = 0;
sandbox.calls.resumed.length = 0;
await onCreated({
  id: 12,
  url: "https://example.org/other.mp4",
  filename: "C:\\Users\\me\\Downloads\\other.mp4",
  referrer: "https://example.org/",
});
assert.equal(sentDownloads().length, 0, "a closed app falls back to the local switch");
assert.deepEqual([...sandbox.calls.resumed], [12], "a download the rules leave alone runs again");
bridgeMode = "online";
delete store.intercept;

capturePolicy = { ...capturePolicy, types: ["mp4"] };
sandbox.calls.fetches.length = 0;
sandbox.calls.cancelled.length = 0;
sandbox.calls.paused.length = 0;
sandbox.calls.resumed.length = 0;
await onCreated({
  id: 9,
  url: "https://example.org/setup.exe",
  filename: "C:\\Users\\me\\Downloads\\setup.exe",
  referrer: "https://example.org/",
});
assert.equal(sentDownloads().length, 0, "a type off the app's list stays in the browser");
assert.deepEqual([...sandbox.calls.cancelled], []);
assert.deepEqual([...sandbox.calls.resumed], [9], "the frozen copy is thawed, not stopped");
capturePolicy = { ...capturePolicy, types: [] };

capturePolicy = { ...capturePolicy, skipSites: ["example.org"] };
sandbox.calls.fetches.length = 0;
sandbox.calls.paused.length = 0;
sandbox.calls.resumed.length = 0;
await onCreated({
  id: 10,
  url: "https://example.org/movie.mp4",
  filename: "C:\\Users\\me\\Downloads\\movie.mp4",
  referrer: "https://example.org/",
});
assert.equal(sentDownloads().length, 0, "a skipped site keeps its download");
assert.deepEqual([...sandbox.calls.resumed], [10]);
capturePolicy = { ...capturePolicy, skipSites: [] };

capturePolicy = { ...capturePolicy, capture: false };
sandbox.calls.fetches.length = 0;
sandbox.calls.paused.length = 0;
sandbox.calls.resumed.length = 0;
await onCreated({
  id: 11,
  url: "https://example.org/movie.mp4",
  filename: "C:\\Users\\me\\Downloads\\movie.mp4",
  referrer: "https://example.org/",
});
assert.equal(sentDownloads().length, 0, "the app's switch turns interception off");
assert.deepEqual([...sandbox.calls.resumed], [11]);
capturePolicy = { ...capturePolicy, capture: true };

// The menus are built on install and startup, from an empty slate.
const install = sandbox.listeners.get("installed")[0];
assert.ok(install, "the menus are built on install");
sandbox.calls.menus.length = 0;
await install();
await new Promise((resolve) => setImmediate(resolve));
assert.ok(sandbox.calls.menus.length >= 4, "the menu holds the hand-over and the app actions");
assert.ok(sandbox.calls.menus.some((item) => item.id === "bdm-download"));
assert.ok(sandbox.calls.menus.some((item) => item.id === "bdm-links"));
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

// The page menu opens the link window for that tab.
sandbox.calls.windows.length = 0;
await onMenu({ menuItemId: "bdm-links", tab: { id: 3 } });
assert.equal(sandbox.calls.windows.length, 1, "the page menu opens the link window");
assert.match(sandbox.calls.windows[0].url, /\?tab=3$/);

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
    pause: async (id) => chromeCalls.push(["pause", id]),
    resume: async (id) => chromeCalls.push(["resume", id]),
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

// The overlay needs a page rather than a sandbox: scripts/check-overlay.mjs
// runs the content scripts in a fake one and exercises the button there.
await import("./check-overlay.mjs");

// The link window is a page of its own: scripts/check-links.mjs runs it
// against a small document and exercises the list, the filters and a send.
await import("./check-links.mjs");

console.log("extension: all checks pass");
