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
import { pngSize } from "./common.mjs";

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

const event = (slot) => ({
  addListener: (fn) => {
    if (slot) channel = fn;
  },
  removeListener: () => {},
  hasListener: () => true,
});

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
