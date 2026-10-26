// Behaviour checks for the Site Grabber's window helpers.
//
// The crawl itself is Rust and is tested there, against a local server. This
// checks the parts the window owns: the address line, the rules and types it
// reads, the preferences it remembers, how events fold into the list, and what
// the report says. Node strips the types, so these are the same functions the
// window imports.

import assert from "node:assert/strict";

import {
  adoptGrabberSnapshot,
  applyGrabberEvent,
  chosenFiles,
  chunks,
  DEFAULT_KINDS,
  DEFAULT_PREFERENCES,
  EMPTY_SNAPSHOT,
  filePath,
  folderProblem,
  grabberAddressProblem,
  loadGrabberPreferences,
  MAX_LOG,
  optionsFrom,
  parseExtraExtensions,
  parseRules,
  progressLine,
  resultSummary,
  notStarted,
  saveGrabberPreferences,
  scanSummary,
  shortAddress,
  visibleFiles,
} from "../src/lib/grabber.ts";

let checks = 0;

function is(actual, expected, label) {
  assert.deepEqual(actual, expected, label);
  checks += 1;
}

function ok(value, label) {
  assert.ok(value, label);
  checks += 1;
}

/* ------------------------------------------------------------- the address */

is(
  grabberAddressProblem(""),
  "Enter the address of the page to read.",
  "an empty address",
);
is(
  grabberAddressProblem("example.org/files"),
  "Enter a full address, like https://example.org/files/.",
  "a bare host",
);
is(grabberAddressProblem("https://example.org/files"), null, "an https page");
is(grabberAddressProblem(" http://example.org/a "), null, "an http page");
is(
  grabberAddressProblem("ftp://example.org/pub"),
  "Only http:// and https:// pages can be read.",
  "an unsupported scheme",
);
is(
  grabberAddressProblem("file:///c:/windows"),
  "Only http:// and https:// pages can be read.",
  "a local path",
);
is(
  grabberAddressProblem("https://user:secret@example.org/"),
  "An address with a user name or password is not followed.",
  "credentials in the address",
);
is(
  grabberAddressProblem("https://example.org/a b"),
  "The address cannot contain spaces.",
  "a raw space",
);

/* --------------------------------------------------------------- the rules */

is(parseRules(""), [], "no rules");
is(
  parseRules("/private/*, /old/\n/cdn.example.org,/private/*"),
  ["/private/*", "/old/", "/cdn.example.org"],
  "rules are trimmed, deduplicated and split on lines and commas",
);
is(
  parseRules(`${"/x".repeat(150)}`).length,
  0,
  "a rule longer than 200 characters is dropped",
);
is(
  parseRules(Array.from({ length: 40 }, (_v, index) => `/p${index}`).join(","))
    .length,
  32,
  "the rule list stops at its limit",
);

is(
  parseExtraExtensions("iso, .EPUB, *.bin"),
  ["iso", "epub", "bin"],
  "typed extensions are cleaned",
);
is(parseExtraExtensions(""), [], "no extra types");

/* ------------------------------------------------------------ the request */

const options = optionsFrom(" https://example.org/files/ ", {
  depth: 99,
  pages: 10_000,
  types: ["video", "archives"],
  extensions: "iso, .EPUB",
  exclude: "/private/*\n/tmp",
  folder: "C:/Downloads",
  connections: 4,
  queue: "main",
});
is(options.url, "https://example.org/files/", "the address is trimmed");
is(options.depth, 5, "depth is clamped to the limit");
is(options.pages, 500, "pages are clamped to the limit");
is(options.types, ["video", "archives"], "the chosen groups are sent");
is(options.extensions, ["iso", "epub"], "the extra types are sent");
is(options.exclude, ["/private/*", "/tmp"], "the rules are sent");

is(
  optionsFrom("https://example.org/", {
    ...DEFAULT_PREFERENCES,
    types: [],
  }).types,
  DEFAULT_KINDS,
  "an empty group choice falls back to the defaults",
);

/* ---------------------------------------------------------------- the fold */

const file = {
  url: "https://example.org/files/a.zip",
  name: "a.zip",
  extension: "zip",
  kind: "archives",
  category: "Compressed",
  page: "https://example.org/",
};
const other = {
  ...file,
  url: "https://example.org/files/b.mp4",
  name: "b.mp4",
  extension: "mp4",
  kind: "video",
  category: "Video",
};

let snapshot = applyGrabberEvent(EMPTY_SNAPSHOT, {
  kind: "started",
  url: "https://example.org/",
  site: "example.org",
  pages: 50,
  depth: 2,
});
is(snapshot.status, "running");
is(snapshot.pageLimit, 50);
is(snapshot.depth, 2);
is(snapshot.files, [], "a new crawl starts with an empty list");

snapshot = applyGrabberEvent(snapshot, { kind: "file", file });
snapshot = applyGrabberEvent(snapshot, { kind: "file", file });
snapshot = applyGrabberEvent(snapshot, { kind: "file", file: other });
is(snapshot.files.length, 2, "a repeated event does not list a file twice");

snapshot = applyGrabberEvent(snapshot, {
  kind: "progress",
  pagesRead: 3,
  pagesQueued: 4,
  files: 2,
  pageLimit: 50,
});
is(snapshot.pagesRead, 3);
is(snapshot.pagesQueued, 4);

snapshot = applyGrabberEvent(snapshot, {
  kind: "note",
  level: "blocked",
  url: "example.org/private",
  message: "robots.txt asks crawlers not to read this page.",
});
is(snapshot.notes.length, 1);
is(snapshot.notes[0].level, "blocked");

snapshot = applyGrabberEvent(snapshot, {
  kind: "done",
  status: "done",
  message: "Read 3 pages and found 2 files.",
  pagesRead: 3,
  pagesQueued: 0,
  files: 2,
  duplicates: 1,
  restricted: 2,
  excluded: 3,
  unsupported: 4,
  truncated: true,
});
is(snapshot.status, "done");
is(snapshot.message, "Read 3 pages and found 2 files.");
is(snapshot.duplicates, 1);
is(snapshot.restricted, 2);
is(snapshot.truncated, true);

let fullLog = { ...EMPTY_SNAPSHOT };
for (let index = 0; index < MAX_LOG + 5; index += 1) {
  fullLog = applyGrabberEvent(fullLog, {
    kind: "note",
    level: "blocked",
    url: `example.org/${index}`,
    message: "left out",
  });
}
is(fullLog.notes.length, MAX_LOG, "the log stops at its limit");
is(fullLog.notesOmitted, 5, "and counts what it left out");

// A file that arrived while the backend's snapshot was in flight is kept.
const inFlight = applyGrabberEvent(
  applyGrabberEvent(EMPTY_SNAPSHOT, {
    kind: "started",
    url: "https://example.org/",
    site: "example.org",
    pages: 50,
    depth: 1,
  }),
  { kind: "file", file },
);
const merged = adoptGrabberSnapshot(
  { ...inFlight, files: [other] },
  inFlight,
);
is(
  merged.files.map((entry) => entry.name),
  ["b.mp4", "a.zip"],
  "a snapshot and an event are merged without losing a file",
);
is(
  adoptGrabberSnapshot({ ...inFlight, status: "running" }, snapshot).status,
  "done",
  "a finished crawl is not shown as running again",
);
is(
  adoptGrabberSnapshot({ ...inFlight, status: "done" }, inFlight).status,
  "done",
  "the backend's later answer wins over an earlier running state",
);

/* ---------------------------------------------------------------- the list */

is(
  visibleFiles([file, other], "all", "").length,
  2,
  "no filter shows every file",
);
is(
  visibleFiles([file, other], "video", "").map((entry) => entry.name),
  ["b.mp4"],
  "the type filter shows one group",
);
is(
  visibleFiles([file, other], "all", "B.MP").map((entry) => entry.name),
  ["b.mp4"],
  "the search ignores case",
);
is(
  visibleFiles([file, other], "all", "files/a.zip").length,
  1,
  "the search reads the address too",
);
is(
  visibleFiles([file, other], "documents", "").length,
  0,
  "a filter with no match is empty",
);

is(
  chosenFiles([file, other], new Set([file.url]), new Set()).map((f) => f.name),
  ["a.zip"],
  "only the ticked file is chosen",
);
is(
  chosenFiles([file, other], new Set([file.url]), new Set([file.url])).length,
  0,
  "an answered file is not chosen twice",
);

is(filePath(`${file.url}?token=secret`), "example.org/files/a.zip", "the path has no query");
is(shortAddress(`${file.url}?token=secret`), "example.org/files/a.zip", "a log line has no query");

/* -------------------------------------------------------------- the report */

is(
  scanSummary({ ...snapshot, files: [file, other] }),
  "2 files · 3 pages read · 2 blocked · 3 excluded · 1 repeated · 4 other types · limit reached",
  "the summary names what was found and left out",
);
is(
  progressLine({ ...snapshot, status: "running", pagesRead: 0, pageLimit: 50 }),
  "Reading the first page…",
  "a running crawl says where it is",
);
is(
  progressLine({ ...snapshot, status: "running", pagesRead: 4, pageLimit: 50 }),
  "Reading page 5 of 50…",
);
is(
  progressLine({ ...snapshot, status: "idle" }),
  "Enter an address, then read the site.",
);

const results = [
  {
    url: "https://example.org/a.zip",
    name: "a.zip",
    status: "started",
    id: 1,
    dest: "C:\\Downloads\\a.zip",
    message: "",
  },
  {
    url: "https://example.org/b.mp4",
    name: "b.mp4",
    status: "started",
    id: 2,
    dest: "C:\\Downloads\\b.mp4",
    message: "",
  },
  {
    url: "https://example.org/c.zip",
    name: "c.zip",
    status: "duplicate",
    id: null,
    dest: "",
    message: "This file is already in the download list.",
  },
  {
    url: "https://example.org/d.zip",
    name: "d.zip",
    status: "refused",
    id: null,
    dest: "",
    message: "The server refused the link.",
  },
];
is(
  resultSummary(results),
  "2 downloads started · 1 already in the list · 1 not started",
  "the report counts each answer",
);
is(
  notStarted(results).map((entry) => entry.name),
  ["c.zip", "d.zip"],
  "the report keeps the answers that need a sentence",
);
is(resultSummary([]), "", "nothing sent, nothing to report");

is(chunks([1, 2, 3, 4, 5], 2), [[1, 2], [3, 4], [5]], "the hand-over is chunked");
is(
  chunks(Array.from({ length: 26 }, (_v, index) => index), 25).map(
    (group) => group.length,
  ),
  [25, 1],
  "a long selection is sent in two parts",
);

/* ---------------------------------------------------------- the destination */

is(folderProblem("C:\\Users\\me\\Downloads"), null, "a Windows folder");
is(folderProblem("/home/me/Downloads"), null, "a Unix folder");
is(folderProblem("\\\\server\\share"), null, "a network folder");
is(folderProblem("Downloads"), "Choose a folder with the folder picker.", "a relative folder");
is(folderProblem(""), "Choose a folder to save the files to.", "no folder");

/* -------------------------------------------------------- the preferences */

const storage = new Map();
globalThis.window = {
  localStorage: {
    getItem: (key) => storage.get(key) ?? null,
    setItem: (key, value) => storage.set(key, String(value)),
  },
};

is(loadGrabberPreferences(), DEFAULT_PREFERENCES, "the defaults on a fresh window");

saveGrabberPreferences({
  depth: 99,
  pages: 0,
  types: ["video", "not-a-kind", "archives"],
  extensions: "iso, epub",
  exclude: "/private/*",
  folder: "C:\\Downloads\\Grab",
  connections: 99,
  queue: "elsewhere",
});
const loaded = loadGrabberPreferences();
is(loaded.depth, 5, "the remembered depth is clamped");
is(loaded.pages, 1, "the remembered page count is clamped");
is(loaded.types, ["video", "archives"], "an unknown group is dropped");
is(loaded.extensions, "iso, epub", "the extra types come back");
is(loaded.exclude, "/private/*", "the rules come back");
is(loaded.folder, "C:\\Downloads\\Grab", "the folder comes back");
is(loaded.connections, 32, "the connection count is clamped");
is(loaded.queue, "now", "an unknown queue falls back to starting now");

saveGrabberPreferences({ ...DEFAULT_PREFERENCES, types: [] });
is(
  loadGrabberPreferences().types,
  DEFAULT_KINDS,
  "an empty group choice comes back as the defaults",
);

storage.set("idm-clone.grabber.v1", "not json");
is(loadGrabberPreferences(), DEFAULT_PREFERENCES, "a broken preference file falls back");

ok(MAX_LOG > 0, "the log has room for a useful number of lines");

console.log(`Site Grabber: ${checks} checks pass`);
