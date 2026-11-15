// lib/bridge.js: the versioned loopback contract, sends, batches and errors.

import assert from "node:assert/strict";
import vm from "node:vm";
import { read } from "./common.mjs";
import { state, sandbox, fakeFetch, lastSend, lastBatch } from "./sandbox.mjs";

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

state.bridgeMode = "online";
state.pingApp = "b-download-manager";

/*
  The connection, first: the client header is the whole contract, and a
  request that carries it is answered without any code.
*/
const ping = await bdmBridgePing();
assert.equal(ping.app, "b-download-manager");
assert.equal(ping.api, 1);
assert.equal(ping.connected, true, "the app recognises the extension without a code");

const headers = sandbox.calls.fetches.at(-1).options.headers;
assert.equal(headers["X-BDM-Client"], "browser-extension", "every request carries the client header");
assert.ok(!headers.Authorization, "and none carries a code");
assert.equal(await requestContext.bdmBridgeVideoButton().then((data) => data.style), "both");

state.listPayload = {
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

state.batchAnswer = {
  ...state.batchAnswer,
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

state.pingApp = "something-else";
await assert.rejects(bdmBridgePing(), (error) => error.kind === "protocol");
state.pingApp = "b-download-manager";

// An older or newer bridge contract is a mismatch, not a connection.
state.pingApi = 2;
await assert.rejects(bdmBridgePing(), (error) => error.kind === "protocol");
state.pingApi = 1;

state.bridgeMode = "offline";
await assert.rejects(bdmBridgeList(), (error) => error.kind === "offline");
state.bridgeMode = "online";

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