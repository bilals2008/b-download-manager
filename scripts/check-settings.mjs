// Self-check for the settings model: defaults, validation, the capture rules
// and the remembered folder. Node strips the types, so this runs against the
// same module the options window imports. Run it with `npm run check:settings`.

import assert from "node:assert/strict";

const store = new Map();
globalThis.window = {
  localStorage: {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => store.set(key, String(value)),
    removeItem: (key) => store.delete(key),
  },
};

const {
  DEFAULT_SETTINGS,
  DUPLICATE_POLICIES,
  MAX_CONNECTIONS,
  MAX_RULE_ENTRIES,
  cleanCaptureType,
  cleanSite,
  clearLastFolder,
  loadLastFolder,
  loadSettings,
  loadToolbarSize,
  preferredFolder,
  saveLastFolder,
  saveSettings,
  saveToolbarSize,
} = await import("../src/lib/settings.ts");

let checks = 0;

function is(actual, expected, label) {
  assert.deepEqual(actual, expected, label);
  checks += 1;
}

function reset() {
  store.clear();
}

const STORAGE_KEY = "idm-clone.settings.v1";

/* --------------------------------------------------------------- defaults */

reset();
is(loadSettings(), DEFAULT_SETTINGS, "an empty store gives the defaults");
is(DEFAULT_SETTINGS.defaultConnections, 8, "a download starts with 8 connections");
is(DEFAULT_SETTINGS.maxConnections, MAX_CONNECTIONS, "the cap matches the engine");
is(DEFAULT_SETTINGS.duplicatePolicy, "ask", "an existing file asks by default");
is(DEFAULT_SETTINGS.browserCapture, true, "browser capture is on by default");
is(DEFAULT_SETTINGS.captureAsk, false, "a hand-over starts without asking");
is(DEFAULT_SETTINGS.captureTypes, [], "every type is captured by default");
is(DEFAULT_SETTINGS.skipSites, [], "no site is skipped by default");

/* ----------------------------------------------------------- round trip */

reset();
const saved = {
  ...DEFAULT_SETTINGS,
  launchOnStartup: true,
  clipboardCapture: true,
  showProgressOnStart: true,
  theme: "dark",
  uiFont: "plex",
  uiWeight: "semibold",
  defaultFolder: "D:\\Downloads",
  rememberLastFolder: false,
  defaultConnections: 16,
  maxConnections: 24,
  speedLimitEnabled: true,
  speedLimitBytes: 1048576,
  proxyUrl: "http://127.0.0.1:8080",
  duplicatePolicy: "rename",
  browserCapture: false,
  captureAsk: true,
  captureTypes: ["mp4", "mkv"],
  skipSites: ["example.org"],
};
saveSettings(saved);
is(loadSettings(), saved, "every field survives a save and a load");

/* ----------------------------------------------------------- bad storage */

reset();
store.set(STORAGE_KEY, "{not json");
is(loadSettings(), DEFAULT_SETTINGS, "broken JSON falls back to the defaults");
store.set(STORAGE_KEY, "null");
is(loadSettings(), DEFAULT_SETTINGS, "a null document falls back");
store.set(STORAGE_KEY, JSON.stringify({ theme: 7, captureTypes: "mp4" }));
const partial = loadSettings();
is(partial.theme, DEFAULT_SETTINGS.theme, "a bad theme falls back");
is(partial.captureTypes, DEFAULT_SETTINGS.captureTypes, "a non-array list falls back");
is(partial.maxConnections, DEFAULT_SETTINGS.maxConnections, "a missing field falls back");

globalThis.window.localStorage.getItem = () => {
  throw new Error("blocked");
};
is(loadSettings(), DEFAULT_SETTINGS, "blocked storage reads the defaults");
globalThis.window.localStorage.getItem = (key) => store.get(key) ?? null;

globalThis.window.localStorage.setItem = () => {
  throw new Error("blocked");
};
saveSettings(DEFAULT_SETTINGS);
checks += 1;
globalThis.window.localStorage.setItem = (key, value) => store.set(key, String(value));

/* ------------------------------------------------------------- validation */

reset();
store.set(
  STORAGE_KEY,
  JSON.stringify({
    ...DEFAULT_SETTINGS,
    theme: "sepia",
    uiFont: "comic-sans",
    uiWeight: "heavy",
    duplicatePolicy: "yolo",
    defaultConnections: 0,
    maxConnections: 999,
    speedLimitBytes: -5,
    launchOnStartup: "yes",
  }),
);
const clamped = loadSettings();
is(clamped.theme, "auto", "an unknown theme is not stored");
is(clamped.uiFont, DEFAULT_SETTINGS.uiFont, "an unknown font falls back");
is(clamped.uiWeight, DEFAULT_SETTINGS.uiWeight, "an unknown weight falls back");
is(clamped.duplicatePolicy, "ask", "an unknown duplicate policy falls back");
is(clamped.defaultConnections, 1, "a connection count below the minimum clamps");
is(clamped.maxConnections, MAX_CONNECTIONS, "a connection count above the cap clamps");
is(clamped.speedLimitBytes, 0, "a negative cap clamps to none");
is(clamped.launchOnStartup, false, "a non-boolean switch falls back");

store.set(
  STORAGE_KEY,
  JSON.stringify({ ...DEFAULT_SETTINGS, defaultConnections: 7.6, speedLimitBytes: 2 ** 31 }),
);
const rounded = loadSettings();
is(rounded.defaultConnections, 8, "a connection count is rounded");
is(rounded.speedLimitBytes, 1024 * 1024 * 1024, "the speed cap tops out at 1 GiB/s");

is(
  DUPLICATE_POLICIES.map((entry) => entry.value),
  ["ask", "rename", "replace", "skip"],
  "the offered duplicate policies are the stored ones",
);

/* ---------------------------------------------------------- capture rules */

is(cleanCaptureType(".MP4"), "mp4", "a leading dot is dropped");
is(cleanCaptureType("*.mkv"), "mkv", "a star and dot are dropped");
is(cleanCaptureType("  mp4  "), "mp4", "surrounding space is dropped");
is(cleanCaptureType("not a type!"), "", "junk is refused");
is(cleanCaptureType(""), "", "an empty type is refused");

is(cleanSite("https://Example.org/some/page"), "example.org", "a URL becomes its host");
is(cleanSite("*.example.org"), "example.org", "a wildcard becomes its host");
is(cleanSite("example.org:8080"), "example.org", "a port is dropped");
is(cleanSite("localhost"), "localhost", "localhost is allowed");
is(cleanSite("127.0.0.1"), "127.0.0.1", "an address is allowed");
is(cleanSite("not a host"), "", "junk is refused");
is(cleanSite(""), "", "an empty site is refused");

reset();
store.set(
  STORAGE_KEY,
  JSON.stringify({
    ...DEFAULT_SETTINGS,
    captureTypes: [" .MP4 ", "*.mkv", "mp4", "not a type!", 5, ""],
    skipSites: ["https://Example.org/path", "*.example.net", "localhost", "not a host", 7],
  }),
);
const cleaned = loadSettings();
is(cleaned.captureTypes, ["mp4", "mkv"], "types are cleaned and deduped");
is(cleaned.skipSites, ["example.org", "example.net", "localhost"], "sites are hosts and unique");

reset();
store.set(
  STORAGE_KEY,
  JSON.stringify({
    ...DEFAULT_SETTINGS,
    captureTypes: Array.from({ length: 100 }, (_, index) => `t${index}`),
    skipSites: Array.from({ length: 100 }, (_, index) => `s${index}.example.org`),
  }),
);
const capped = loadSettings();
is(capped.captureTypes.length, MAX_RULE_ENTRIES, "the type list is capped");
is(capped.skipSites.length, MAX_RULE_ENTRIES, "the site list is capped");

/* --------------------------------------------------------- last folder */

reset();
saveLastFolder("  D:\\Downloads\\film  ");
is(loadLastFolder(), "D:\\Downloads\\film", "the last folder is trimmed");
is(preferredFolder(), "D:\\Downloads\\film", "the remembered folder wins");

saveLastFolder("");
is(loadLastFolder(), "D:\\Downloads\\film", "an empty folder is not remembered");

saveSettings({ ...DEFAULT_SETTINGS, defaultFolder: "E:\\Files", rememberLastFolder: false });
is(preferredFolder(), "E:\\Files", "without remembering, the default folder wins");

saveSettings({ ...DEFAULT_SETTINGS, defaultFolder: "E:\\Files", rememberLastFolder: true });
is(preferredFolder(), "D:\\Downloads\\film", "remembering keeps the last folder");

clearLastFolder();
is(loadLastFolder(), "", "a reset forgets the last folder");
is(preferredFolder(), "E:\\Files", "and the default folder takes over");

saveSettings({ ...DEFAULT_SETTINGS, defaultFolder: "  E:\\Files  " });
is(preferredFolder(), "E:\\Files", "the default folder is trimmed");

/* ------------------------------------------------------------- toolbar */

reset();
is(loadToolbarSize(), "large", "the toolbar starts large");
saveToolbarSize("small");
is(loadToolbarSize(), "small", "the toolbar size survives");
store.set("idm-clone.toolbar-size.v1", "huge");
is(loadToolbarSize(), "large", "an unknown size falls back");

console.log(`settings: ${checks} checks pass`);
