// The browser packages: one shared source, one folder per engine.
//
// The Chromium build runs the same scripts as Firefox, adapted by
// `source/lib/platform.js`; the Safari build runs them as a background
// document. These checks cover what differs per browser — the manifests, the
// service-worker entry, the pages that load the adapter, and the adapter
// itself against a callback-style `chrome`.

import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import { buildExtension, BROWSERS } from "../../extensions/build.mjs";
import { pngSize, setTimeoutShim } from "./common.mjs";

const extensionRoot = fileURLToPath(new URL("../../extensions/", import.meta.url));

const built = Object.fromEntries(
  BROWSERS.map((browser) => [browser, buildExtension(browser)]),
);

function json(browser) {
  return JSON.parse(readFileSync(path.join(built[browser], "manifest.json"), "utf8"));
}

function read(browser, file) {
  return readFileSync(path.join(built[browser], file), "utf8");
}


/* ------------------------------------------------------------- packaging */

for (const browser of BROWSERS) {
  const manifest = json(browser);
  assert.equal(manifest.manifest_version, 3, `${browser}: Manifest V3`);
  assert.equal(manifest.name, "B Download Manager", `${browser}: the app's name`);

  const files = [
    ...(manifest.background.scripts ?? []),
    ...(manifest.background.service_worker ? [manifest.background.service_worker] : []),
    manifest.action.default_popup,
    manifest.options_ui.page,
    ...Object.values(manifest.action.default_icon),
    ...Object.values(manifest.icons),
    ...(manifest.content_scripts ?? []).flatMap((entry) => entry.js ?? []),
  ];
  for (const file of files) {
    assert.ok(existsSync(path.join(built[browser], file)), `${browser}: ${file} is in the folder`);
  }

  for (const [size, file] of Object.entries({
    ...manifest.icons,
    ...manifest.action.default_icon,
  })) {
    const pixels = pngSize(readFileSync(path.join(built[browser], file)));
    assert.deepEqual(
      pixels,
      { width: Number(size), height: Number(size) },
      `${browser}: ${file} should be ${size}x${size}`,
    );
  }

  assert.ok(
    manifest.host_permissions.includes("http://127.0.0.1:27500/*"),
    `${browser}: the loopback bridge is allowed`,
  );
  assert.ok(
    manifest.host_permissions.some((host) => host === "http://*/*"),
    `${browser}: the pages the overlay reads are allowed`,
  );
}

// The source is copied, not generated: every shared file has to be the same
// bytes in every build, or the "one source" promise is a fiction.
function everyFile(dir) {
  return readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => path.relative(dir, path.join(entry.parentPath, entry.name)));
}

const sourceFiles = everyFile(path.join(extensionRoot, "source"));
for (const browser of BROWSERS) {
  for (const file of sourceFiles) {
    assert.ok(
      readFileSync(path.join(built[browser], file)).equals(
        readFileSync(path.join(extensionRoot, "source", file)),
      ),
      `${browser}: ${file} is the shared source, unchanged`,
    );
  }
}

/* -------------------------------------------------------------- chromium */

const chromium = json("chromium");
assert.equal(chromium.background.service_worker, "worker.js", "chromium: a service worker");
assert.equal(chromium.background.scripts, undefined, "chromium: no event-page scripts");
assert.ok(chromium.permissions.includes("tabs"), "chromium: the popup reads the current tab");
for (const permission of ["contextMenus", "cookies", "downloads", "storage", "alarms"]) {
  assert.ok(chromium.permissions.includes(permission), `chromium: ${permission} is asked for`);
}
assert.ok(!chromium.permissions.includes("webRequest"), "chromium: no request interception");

const chromiumContent = chromium.content_scripts[0].js;
assert.equal(chromiumContent[0], "lib/platform.js", "chromium: the adapter loads first");
assert.ok(
  chromiumContent.indexOf("lib/settings.js") < chromiumContent.indexOf("content/overlay.js"),
  "chromium: settings load before the overlay that reads them",
);

const worker = read("chromium", "worker.js");
const imported = [...worker.matchAll(/"([^"]+)"/g)].map((match) => match[1]);
assert.equal(imported[0], "lib/platform.js", "chromium: the worker adapts the API first");
assert.equal(imported.at(-1), "background.js", "chromium: the shared background runs last");
assert.deepEqual(
  imported.slice(1, -1),
  json("firefox").background.scripts.slice(0, -1),
  "chromium: the worker imports the Firefox background list, in order",
);

// The pages that talk to the extension need the adapter before the scripts
// that use `browser`; a page without it would fail on a Chromium engine.
for (const page of ["popup/popup.html", "options/options.html", "links/links.html"]) {
  const html = read("chromium", page);
  const adapter = html.indexOf("../lib/platform.js");
  assert.ok(adapter > 0, `chromium: ${page} loads the adapter`);
  for (const script of ['"../lib/settings.js"', '"../lib/link-rules.js"']) {
    const index = html.indexOf(script);
    if (index > 0) {
      assert.ok(adapter < index, `chromium: ${page} loads the adapter before ${script}`);
    }
  }
}

/* ---------------------------------------------------------------- safari */

const safari = json("safari");
assert.equal(safari.background.service_worker, undefined, "safari: a background document");
assert.ok(Array.isArray(safari.background.scripts), "safari: the shared scripts are listed");
assert.equal(safari.background.scripts[0], "lib/media.js", "safari: the same first script");
assert.equal(safari.background.scripts.at(-1), "background.js", "safari: the same entry");
assert.deepEqual(
  safari.background.scripts,
  json("firefox").background.scripts,
  "safari: the Firefox background list, unchanged",
);
assert.equal(safari.browser_specific_settings.gecko, undefined, "safari: no Gecko id");
assert.ok(
  Number.parseFloat(safari.browser_specific_settings.safari.strict_min_version) >= 16.4,
  "safari: MV3 needs 16.4 or newer",
);
assert.ok(!safari.permissions.includes("tabs"), "safari: no permission it does not use");

/* ---------------------------------------------------------------- shared */

for (const browser of ["firefox", "safari"]) {
  const html = read(browser, "popup/popup.html");
  assert.ok(
    html.includes("../lib/platform.js"),
    `${browser}: the adapter is a no-op there, and still loaded by the page`,
  );
}

/* ------------------------------------------------------------ the adapter

   Chromium's APIs take a callback; the extension is written against promises.
   The adapter is exercised here against a callback-style fake, including the
   one rule that is easy to get wrong: `runtime.lastError` has to be read
   inside the callback, and a message listener that returns a promise has to
   answer through `sendResponse` while returning `true`. */

const calls = [];
let failure = null;
let channel = null;

function answer(value, owner, name) {
  return (...args) => {
    calls.push([name, ...args]);
    const callback = args.pop();
    setTimeout(() => {
      if (failure) {
        chrome.runtime.lastError = { message: failure };
        callback(undefined);
        chrome.runtime.lastError = undefined;
      } else {
        callback(value);
      }
    }, 0);
  };
}

/** An event that keeps what was registered, so a removal can be checked. */
const event = (slot) => {
  const listeners = [];
  return {
    listeners,
    addListener: (fn) => {
      listeners.push(fn);
      if (slot) channel = fn;
    },
    removeListener: (fn) => {
      const index = listeners.indexOf(fn);
      if (index >= 0) listeners.splice(index, 1);
    },
    hasListener: (fn) => listeners.includes(fn),
  };
};

const chrome = {
  runtime: {
    id: "check-extension",
    getURL: (file) => `chrome-extension://check/${file}`,
    getManifest: () => ({ version: "1.1.6" }),
    sendMessage: answer({ ok: true }, "runtime", "sendMessage"),
    openOptionsPage: answer(undefined, "runtime", "openOptionsPage"),
    onMessage: event(true),
    onInstalled: event(),
    onStartup: event(),
  },
  storage: {
    local: {
      get: answer({ theme: "dark" }, "storage.local", "get"),
      set: answer(undefined, "storage.local", "set"),
    },
    onChanged: event(),
  },
  action: {
    setBadgeText: answer(undefined, "action", "setBadgeText"),
    setBadgeBackgroundColor: answer(undefined, "action", "setBadgeBackgroundColor"),
    setBadgeTextColor: answer(undefined, "action", "setBadgeTextColor"),
  },
  contextMenus: {
    create: (props, callback) => callback?.(),
    removeAll: answer(undefined, "contextMenus", "removeAll"),
    onClicked: event(),
  },
  tabs: {
    query: answer([{ id: 4 }], "tabs", "query"),
    create: answer({ id: 5 }, "tabs", "create"),
    sendMessage: answer({ ok: true }, "tabs", "sendMessage"),
    onUpdated: event(),
    onRemoved: event(),
  },
  windows: { create: answer({ id: 6 }, "windows", "create") },
  downloads: {
    pause: answer(undefined, "downloads", "pause"),
    resume: answer(undefined, "downloads", "resume"),
    cancel: answer(undefined, "downloads", "cancel"),
    erase: answer(undefined, "downloads", "erase"),
    onCreated: event(),
  },
  cookies: { getAll: answer([{ name: "SID", value: "1" }], "cookies", "getAll") },
  alarms: {
    create: (...args) => {
      calls.push(["alarms.create", ...args]);
    },
    onAlarm: event(),
  },
};

const context = vm.createContext({ chrome, console, setTimeout, clearTimeout, URL });
vm.runInContext(read("chromium", "lib/platform.js"), context, {
  filename: "lib/platform.js",
});

const browser = context.browser;
assert.ok(browser, "adapter: it publishes a `browser` object");

assert.equal(
  browser.runtime.getURL("popup/popup.html"),
  "chrome-extension://check/popup/popup.html",
  "adapter: getURL stays synchronous",
);
assert.deepEqual(
  await browser.storage.local.get({ theme: "light", extra: 1 }),
  { theme: "dark" },
  "adapter: a storage read answers with the browser's value",
);
assert.deepEqual(await browser.tabs.query({ active: true }), [{ id: 4 }]);
assert.deepEqual(await browser.cookies.getAll({ url: "https://example.org/" }), [
  { name: "SID", value: "1" },
]);
assert.deepEqual(await browser.downloads.pause(12), undefined, "adapter: pause answers");
assert.equal(typeof browser.action.setBadgeTextColor, "function", "adapter: the badge colour");
assert.equal(browser.alarms.create("x", {}), undefined, "adapter: alarms stay fire-and-forget");
assert.ok(
  calls.some((call) => call[0] === "alarms.create"),
  "adapter: the alarm reaches the engine",
);

failure = "no such download";
await assert.rejects(
  () => browser.downloads.cancel(99),
  /no such download/,
  "adapter: lastError becomes a rejected promise",
);
failure = null;

/* The message listener: promise in, sendResponse out, `true` returned. */
const answered = [];
browser.runtime.onMessage.addListener(async (message) => ({
  ok: true,
  echo: message.type,
}));
assert.ok(channel, "adapter: the message channel is wrapped");
const keepOpen = channel({ type: "bdm:status" }, {}, (value) => answered.push(value));
assert.equal(keepOpen, true, "adapter: an async listener keeps the channel open");
await new Promise((resolve) => setTimeout(resolve, 0));
assert.deepEqual(answered[0], { ok: true, echo: "bdm:status" });

/* The engine was given the wrapper, so a removal has to name the wrapper. */
const detach = async () => ({ ok: true });
const registered = chrome.runtime.onMessage.listeners.length;
browser.runtime.onMessage.addListener(detach);
browser.runtime.onMessage.removeListener(detach);
assert.equal(
  chrome.runtime.onMessage.listeners.length,
  registered,
  "adapter: a removed listener leaves the engine",
);
assert.ok(!browser.runtime.onMessage.hasListener(detach), "adapter: and is forgotten");

let syncChannel = null;
const syncContext = vm.createContext({
  chrome: {
    ...chrome,
    runtime: { ...chrome.runtime, onMessage: { addListener: (fn) => (syncChannel = fn) } },
  },
  console,
  setTimeout,
  clearTimeout,
  URL,
});
vm.runInContext(read("chromium", "lib/platform.js"), syncContext, {
  filename: "lib/platform.js",
});
syncContext.browser.runtime.onMessage.addListener(() => undefined);
assert.equal(
  syncChannel({ type: "bdm:links-scan-progress" }, {}),
  false,
  "adapter: a listener that answers nothing closes the channel",
);

/* Firefox and Safari already have the promise API: the adapter must not wrap
   it, or the two would drift. */
const native = { runtime: { sendMessage: () => "native" } };
const nativeContext = vm.createContext({ browser: native, console, chrome });
vm.runInContext(read("chromium", "lib/platform.js"), nativeContext, {
  filename: "lib/platform.js",
});
assert.equal(nativeContext.browser, native, "adapter: an existing `browser` is left alone");

/* --------------------------------------------------------- the worker itself

   A Chromium background is a service worker: one entry file that imports the
   rest, a global with no `window` and no `document`, and a `chrome` whose
   methods take callbacks. The chain is run here in exactly that shape, so a
   broken import list, a name two scripts both declare, a DOM call at load, a
   listener registered too late or a top-level throw fails this check instead of
   the reader's browser. */

const workerEvents = new Map();
const workerCalls = [];
const workerFetches = [];

/** Let the fake's queued callbacks answer, as the engine's would. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

function workerEvent(name) {
  return {
    addListener: (fn) => {
      workerEvents.set(name, [...(workerEvents.get(name) || []), fn]);
    },
  };
}

function workerAnswer(value, name) {
  return (...args) => {
    const callback = args.pop();
    workerCalls.push([name, ...args]);
    setTimeout(() => callback(value), 0);
  };
}

const workerChrome = {
  runtime: {
    id: "check-worker",
    getURL: (file) => `chrome-extension://check/${file}`,
    getManifest: () => ({ version: "1.1.6" }),
    sendMessage: workerAnswer(undefined, "runtime.sendMessage"),
    openOptionsPage: workerAnswer(undefined, "runtime.openOptionsPage"),
    onMessage: workerEvent("runtime.onMessage"),
    onInstalled: workerEvent("runtime.onInstalled"),
    onStartup: workerEvent("runtime.onStartup"),
    get lastError() {
      return undefined;
    },
  },
  storage: {
    local: {
      get: workerAnswer({}, "storage.get"),
      set: workerAnswer(undefined, "storage.set"),
    },
    onChanged: workerEvent("storage.onChanged"),
  },
  action: {
    setBadgeText: workerAnswer(undefined, "action.setBadgeText"),
    setBadgeBackgroundColor: workerAnswer(undefined, "action.setBadgeBackgroundColor"),
    setBadgeTextColor: workerAnswer(undefined, "action.setBadgeTextColor"),
  },
  contextMenus: {
    create: (props, callback) => {
      workerCalls.push(["contextMenus.create", props.id]);
      callback?.();
    },
    removeAll: workerAnswer(undefined, "contextMenus.removeAll"),
    onClicked: workerEvent("contextMenus.onClicked"),
  },
  tabs: {
    create: workerAnswer({}, "tabs.create"),
    sendMessage: workerAnswer({ ok: true }, "tabs.sendMessage"),
    onUpdated: workerEvent("tabs.onUpdated"),
    onRemoved: workerEvent("tabs.onRemoved"),
  },
  windows: { create: workerAnswer({}, "windows.create") },
  downloads: {
    pause: workerAnswer(undefined, "downloads.pause"),
    resume: workerAnswer(undefined, "downloads.resume"),
    cancel: workerAnswer(undefined, "downloads.cancel"),
    erase: workerAnswer(undefined, "downloads.erase"),
    onCreated: workerEvent("downloads.onCreated"),
  },
  cookies: { getAll: workerAnswer([], "cookies.getAll") },
  alarms: {
    create: (...args) => workerCalls.push(["alarms.create", ...args]),
    onAlarm: workerEvent("alarms.onAlarm"),
  },
};

/** The bridge's answers, so a hand-over can be driven to the end. */
const bridgeAnswers = {
  "/v1/ping": { app: "b-download-manager", api: 1, version: "2.0.1", connected: true },
  "/v1/capture": { capture: true, ask: false, types: [], skipSites: [] },
};

async function workerFetch(url, options) {
  const route = new URL(url).pathname;
  workerFetches.push([route, options?.method || "GET"]);
  const data = bridgeAnswers[route] || {};
  return { status: 200, text: async () => JSON.stringify({ ok: true, data }) };
}

// A service worker's global: no window, no document, and `importScripts`.
const workerGlobal = vm.createContext({
  chrome: workerChrome,
  console,
  setTimeout: setTimeoutShim,
  clearTimeout,
  URL,
  AbortController,
  fetch: workerFetch,
  navigator: { userAgent: "check-worker" },
});
workerGlobal.importScripts = (...files) => {
  for (const file of files) {
    vm.runInContext(read("chromium", file), workerGlobal, { filename: file });
  }
};

vm.runInContext(read("chromium", "worker.js"), workerGlobal, { filename: "worker.js" });

assert.ok(workerGlobal.browser, "chromium: every imported script sees one adapter");

// Manifest V3 only wakes a worker for listeners that exist at the top level.
for (const name of [
  "runtime.onMessage",
  "runtime.onInstalled",
  "runtime.onStartup",
  "contextMenus.onClicked",
  "downloads.onCreated",
  "tabs.onUpdated",
  "tabs.onRemoved",
  "alarms.onAlarm",
]) {
  assert.ok(
    workerEvents.get(name)?.length,
    `chromium: the worker listens for ${name} without being asked`,
  );
}

// A wake-up: the browser starts the worker for an event and it rebuilds itself.
for (const listener of workerEvents.get("runtime.onInstalled")) listener({ reason: "install" });
await settle();

assert.deepEqual(
  workerCalls.filter((call) => call[0] === "contextMenus.create").map((call) => call[1]),
  ["bdm-download", "bdm-links", "bdm-separator", "bdm-open", "bdm-settings"],
  "chromium: the wake-up builds the menus",
);
assert.ok(
  workerCalls.some((call) => call[0] === "action.setBadgeBackgroundColor"),
  "chromium: the worker paints the badge at load",
);

// A message from a page: promise in, sendResponse out, `true` returned.
const workerAnswers = [];
const keptOpen = workerEvents.get("runtime.onMessage")[0](
  { type: "bdm:status" },
  {},
  (value) => workerAnswers.push(value),
);
assert.equal(keptOpen, true, "chromium: the worker answers asynchronously");
await settle();
assert.deepEqual(
  // A copy: the answer was built in the worker's own realm.
  { ...workerAnswers[0] },
  { ok: true, version: "2.0.1", api: 1, connected: true },
  "chromium: the worker answers a status message",
);

// The link window, from the right-click menu.
workerCalls.length = 0;
await workerEvents.get("contextMenus.onClicked")[0]({ menuItemId: "bdm-links", tab: { id: 3 } });
const windowCall = workerCalls.find((call) => call[0] === "windows.create");
assert.ok(windowCall, "chromium: the link menu opens a window");
assert.match(
  windowCall[1].url,
  /links\/links\.html\?tab=3$/,
  "chromium: the window names the tab it describes",
);

// A download the browser started: frozen, handed over, then stopped.
workerCalls.length = 0;
await workerEvents.get("downloads.onCreated")[0]({
  id: 7,
  url: "https://example.org/file.zip",
  referrer: "https://example.org/",
  filename: "C:\\Users\\check\\Downloads\\file.zip",
});
assert.ok(
  workerCalls.some((call) => call[0] === "downloads.pause"),
  "chromium: the browser copy is frozen before the hand-over",
);
assert.ok(
  workerFetches.some(([route, method]) => route === "/v1/downloads" && method === "POST"),
  "chromium: the download reaches the bridge",
);
assert.ok(
  workerCalls.some((call) => call[0] === "downloads.cancel"),
  "chromium: the browser copy is stopped once the app has answered",
);
