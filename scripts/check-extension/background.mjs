// background.js: the message handler, the button and tabs, and the link window.

import assert from "node:assert/strict";
import { until } from "./common.mjs";
import {
  state,
  sandbox,
  store,
  downloadBehaviour,
  lastSend,
  lastBatch,
  sentDownloads,
} from "./sandbox.mjs";

// The background was loaded before these tests, register the listeners that
// were captured from it.
const onMessage = sandbox.listeners.get("message")[0];
assert.ok(onMessage, "the background listens for messages");

state.bridgeMode = "online";
state.pingApp = "b-download-manager";
const statusAnswer = await onMessage({ type: "bdm:status" }, {});
assert.equal(statusAnswer.ok, true);
assert.equal(statusAnswer.version, "0.1.0");
assert.equal(statusAnswer.connected, true, "a reachable app is a working integration");
const statusButton = await onMessage({ type: "bdm:video-button-settings" }, {});
assert.equal(statusButton.ok, true, "the button settings come from the app");

// An older or newer contract is a mismatch, which the popup has its own line for.
state.pingApi = 2;
const mismatch = await onMessage({ type: "bdm:status" }, {});
assert.equal(mismatch.ok, false);
assert.equal(mismatch.error.kind, "protocol", "a different API version is not a connection");
state.pingApi = 1;

/* ---------------------------------------------------- the button and the tabs */

const buttonAnswer = await onMessage({ type: "bdm:video-button-settings" }, {});
assert.equal(buttonAnswer.ok, true);
assert.equal(buttonAnswer.style, "both", "the application's look, not the extension default");
assert.equal(buttonAnswer.label, "Get it");
assert.equal(buttonAnswer.hover, true);

sandbox.calls.fetches.length = 0;
await onMessage({ type: "bdm:video-button-settings" }, {});
assert.equal(sandbox.calls.fetches.length, 0, "a fresh answer is not asked for twice");

// A page with a video on it is what starts the check the tabs are fed from.
await onMessage({ type: "bdm:media-count", count: 3 }, { tab: { id: 9 } });
assert.ok(
  sandbox.calls.alarms.some((alarm) => alarm.name === "bdm-video-button"),
  "a video page starts the periodic read of the application's settings",
);

state.videoButton = { style: "text", label: "Save", hover: false };
sandbox.calls.relayed.length = 0;
const onAlarm = sandbox.listeners.get("alarm")[0];
assert.ok(onAlarm, "the background listens for its own alarm");
await onAlarm({ name: "bdm-video-button" });
const pushed = await until(() =>
  sandbox.calls.relayed.find((message) => message.type === "bdm:video-button-changed"),
);
assert.ok(pushed, "the reader's change is pushed to the pages already open");
const change = sandbox.calls.relayed.find((message) => message.type === "bdm:video-button-changed");
assert.equal(change.style, "text");
assert.equal(change.label, "Save");
assert.equal(change.hover, false);
const afterChange = await onMessage({ type: "bdm:video-button-settings" }, {});
assert.equal(afterChange.style, "text", "a page that asks now gets the new look");

const beforeOtherAlarm = sandbox.calls.fetches.length;
await onAlarm({ name: "bdm-something-else" });
assert.equal(sandbox.calls.fetches.length, beforeOtherAlarm, "an alarm nobody else owns is ignored");

state.listPayload = { downloads: [], active: 0 };
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
state.capturePolicy = { ...state.capturePolicy, ask: true };
await onMessage(
  { type: "bdm:send", payload: { url: "https://example.org/b.bin", referer: "https://example.org/" } },
  {},
);
assert.equal(lastSend.body.mode, "ask", "the app's ask setting reaches the bridge");
state.capturePolicy = { ...state.capturePolicy, ask: false };
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

state.bridgeMode = "offline";
const offlineStatus = await onMessage({ type: "bdm:status" }, {});
assert.equal(offlineStatus.ok, false);
assert.equal(offlineStatus.error.kind, "offline");
state.bridgeMode = "online";

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
state.tabAnswer = {
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

state.batchAnswer = {
  ...state.batchAnswer,
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

state.bridgeMode = "offline";
const offlineBatch = await onMessage({ type: "bdm:batch-options" }, {});
assert.equal(offlineBatch.ok, false);
assert.equal(offlineBatch.error.kind, "offline");
state.bridgeMode = "online";
state.tabAnswer = { ok: true, links: [], page: {}, stats: {} };

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

state.bridgeMode = "offline";
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
state.bridgeMode = "online";

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
state.bridgeMode = "offline";
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
state.bridgeMode = "online";
delete store.intercept;

state.capturePolicy = { ...state.capturePolicy, types: ["mp4"] };
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
state.capturePolicy = { ...state.capturePolicy, types: [] };

state.capturePolicy = { ...state.capturePolicy, skipSites: ["example.org"] };
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
state.capturePolicy = { ...state.capturePolicy, skipSites: [] };

state.capturePolicy = { ...state.capturePolicy, capture: false };
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
state.capturePolicy = { ...state.capturePolicy, capture: true };

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
state.capturePolicy = { ...state.capturePolicy, ask: false, skipSites: ["example.org"] };
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
state.capturePolicy = { ...state.capturePolicy, skipSites: [] };

// Opening the settings from the menu works.
sandbox.calls.openedOptions = 0;
await onMenu({ menuItemId: "bdm-settings", tab: { id: 3 } });
assert.equal(sandbox.calls.openedOptions, 1);

// The page menu opens the link window for that tab.
sandbox.calls.windows.length = 0;
await onMenu({ menuItemId: "bdm-links", tab: { id: 3 } });
assert.equal(sandbox.calls.windows.length, 1, "the page menu opens the link window");
assert.match(sandbox.calls.windows[0].url, /\?tab=3$/);