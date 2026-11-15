// The sandbox: a mocked browser and a mocked loopback bridge, then the
// background page loaded against them. The shared, mutable answers the tests
// drive live in `state`; the mocked objects are exported by reference.

import assert from "node:assert/strict";
import vm from "node:vm";
import { read, manifest, setTimeoutShim } from "./common.mjs";

export const store = {};
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
    alarms: [],
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
        return state.tabAnswer;
      },
    },
    storage: {
      local: {
        // The real `get` answers the keys that were asked for, whether the
        // caller passed an object of defaults, one key, or a list of them.
        get: async (wanted) => {
          const defaults =
            wanted && typeof wanted === "object" && !Array.isArray(wanted) ? wanted : {};
          const keys =
            typeof wanted === "string"
              ? [wanted]
              : Array.isArray(wanted)
                ? wanted
                : Object.keys(defaults);
          const answer = {};
          for (const key of keys) {
            if (key in store) answer[key] = store[key];
            else if (key in defaults) answer[key] = defaults[key];
          }
          return answer;
        },
        set: async (value) => Object.assign(store, value),
        remove: async (key) => {
          for (const name of typeof key === "string" ? [key] : Array.isArray(key) ? key : []) {
            delete store[name];
          }
        },
      },
      onChanged: { addListener: add("storage") },
    },
    alarms: {
      create: (name, info) => calls.alarms.push({ name, info }),
      clear: async (name) => {
        calls.alarms = calls.alarms.filter((alarm) => alarm.name !== name);
        return true;
      },
      get: async (name) => calls.alarms.find((alarm) => alarm.name === name) || undefined,
      onAlarm: { addListener: add("alarm") },
    },
  };

  return { browser, listeners, calls };
}

export const sandbox = mockBrowser();
// How the mocked downloads API behaves for specific ids: how many pauses a
// download still refuses (a transfer that has not begun yet), and which
// resumes fail. The hand-over freezes a fresh download and thaws it again
// when the app does not take it, so the tests need to drive both answers.
export const downloadBehaviour = { refusedPause: new Map(), failedResume: new Set() };

// The mutable answers the mocked bridge reads and the tests rewrite. They were
// `let` bindings in the single script; a shared object keeps them live across
// the sandbox, bridge and background modules.
export const state = {
  bridgeMode: "online",
  pingApp: "b-download-manager",
  pingApi: 1,
  listPayload: { downloads: [], active: 0 },
  capturePolicy: { capture: true, ask: false, types: [], skipSites: [] },
  videoButton: { style: "both", label: "Get it", hover: true },
  tabAnswer: { ok: true, links: [], page: {}, stats: {} },
  batchOptions: {
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
  },
  batchAnswer: {
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
  },
};
export const lastSend = { body: null };
export const lastBatch = { body: null };

/** A refusal in the shape the bridge writes: one code and one sentence. */
function refusal(status, code, message) {
  return {
    status,
    async text() {
      return JSON.stringify({ ok: false, error: { code, message } });
    },
  };
}

export async function fakeFetch(url, options = {}) {
  sandbox.calls.fetches.push({ url, options });
  if (state.bridgeMode === "offline") throw new TypeError("Failed to fetch");

  const pathName = new URL(String(url)).pathname;
  const ping = pathName === "/v1/ping";
  const headers = options.headers || {};
  if (headers["X-BDM-Client"] !== "browser-extension") {
    return refusal(403, "forbidden", "the browser extension header is missing");
  }

  let data;
  if (ping) {
    data = {
      app: state.pingApp,
      api: state.pingApi,
      version: "0.1.0",
      port: 27500,
      connected: true,
    };
  } else if (pathName === "/v1/video-button-settings") {
    data = state.videoButton;
  } else if (pathName === "/v1/capture") {
    data = state.capturePolicy;
  } else if (pathName === "/v1/downloads" && options.method === "GET") {
    data = state.listPayload;
  } else if (pathName === "/v1/downloads" && options.method === "POST") {
    lastSend.body = JSON.parse(options.body);
    data = { mode: "direct", id: 42, name: "a.bin", dest: "C:\\Downloads\\a.bin" };
  } else if (pathName === "/v1/batch-options") {
    data = state.batchOptions;
  } else if (pathName === "/v1/downloads/batch" && options.method === "POST") {
    lastBatch.body = JSON.parse(options.body);
    data = {
      ...state.batchAnswer,
      queue: lastBatch.body.queue,
      folder: lastBatch.body.folder || state.batchAnswer.folder,
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
export const sentDownloads = () =>
  sandbox.calls.fetches.filter((call) => new URL(String(call.url)).pathname === "/v1/downloads");

// The background page and the mock bridge agree on the client header alone;
// there is no code to hold or to take away.

export const backgroundContext = vm.createContext({
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