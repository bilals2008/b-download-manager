#!/usr/bin/env node
/*
  A live check that the application stays responsive under load.

  The freeze a reader sees is the main thread blocking: the window stops
  drawing and stops answering, while the bridge keeps saying everything is
  fine. This probe starts several downloads at once and asks the application,
  over the bridge, how long the main thread needed to pick up a task.

  A main thread that needs more than a second is frozen: nothing the reader
  does is seen, and the window has to be killed.

  It needs the desktop application running.

    pnpm probe:freeze              # eight downloads at once
    pnpm probe:freeze -- 16        # sixteen at once
    pnpm probe:freeze --keep       # leave the downloaded files where they are
*/

import assert from "node:assert/strict";
import { createServer } from "node:http";
import { existsSync, rmSync } from "node:fs";

const PORT = 27500;
const BRIDGE = `http://127.0.0.1:${PORT}`;
const PROBE_ORIGIN = "moz-extension://probe-0000-0000";
const CLIENT = { "X-BDM-Client": "browser-extension" };
const EXTENSION = { ...CLIENT, Origin: PROBE_ORIGIN };

const args = process.argv.slice(2);
const keep = args.includes("--keep");
const count = Number(args.find((arg) => /^\d+$/.test(arg)) || 8);

/* The main thread may take this long and still be alive. */
const FROZEN_MS = 1000;

/* How long the file server takes to hand the bytes over, so the downloads
   are still running while the main thread is measured. */
const CHUNKS = 40;
const CHUNK_SIZE = 64 * 1024;
const PAYLOAD = Buffer.alloc(CHUNKS * CHUNK_SIZE, 0b01001100);

async function call({ method = "GET", path, headers = {}, body } = {}) {
  try {
    const response = await fetch(`${BRIDGE}${path}`, {
      method,
      headers: {
        ...headers,
        ...(body === undefined ? {} : { "Content-Type": "application/json" }),
      },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    let payload = null;
    try {
      payload = await response.json();
    } catch {
      // An answer that is not JSON is its own result.
    }
    return { status: response.status, payload };
  } catch {
    return { status: 0, payload: null };
  }
}

/* One file of its own, served in pieces with a small pause between them, so
   every download lasts long enough to be measured. */
const server = createServer(async (request, response) => {
  const range = /bytes=(\d+)-(\d*)/.exec(request.headers.range || "");
  const start = range ? Number(range[1]) : 0;
  const end = range
    ? Math.min(Number(range[2] || PAYLOAD.length - 1), PAYLOAD.length - 1)
    : PAYLOAD.length - 1;
  const length = end - start + 1;
  response.writeHead(range ? 206 : 200, {
    ...(range
      ? { "Content-Range": `bytes ${start}-${end}/${PAYLOAD.length}` }
      : {}),
    "Content-Length": length,
    "Accept-Ranges": "bytes",
  });
  let written = 0;
  while (written < length) {
    const take = Math.min(CHUNK_SIZE, length - written);
    if (!response.write(PAYLOAD.subarray(start + written, start + written + take))) {
      await new Promise((done) => response.once("drain", done));
    }
    written += take;
    await new Promise((done) => setTimeout(done, 25));
  }
  response.end();
});

await new Promise((done) => server.listen(0, "127.0.0.1", done));
const source = `http://127.0.0.1:${server.address().port}/freeze.bin`;

const results = [];
function check(name, condition, detail) {
  results.push({ name, ok: Boolean(condition), detail });
}

try {
  const ping = await call({ path: "/v1/ping", headers: EXTENSION });
  assert.equal(
    ping.payload?.data?.app,
    "b-download-manager",
    "the application is not running on the bridge port; start it first",
  );

  const health = await call({ path: "/v1/health", headers: EXTENSION });
  check(
    "the application answers a health check",
    health.status === 200,
    `it answered ${health.status}`,
  );

  /* Every download at once, so the engine, the window and the tray all have
     something to do at the same time. */
  const handed = [];
  for (let index = 0; index < count; index += 1) {
    const name = `bdm-freeze-probe-${Date.now().toString(36)}-${index}.bin`;
    const sent = await call({
      method: "POST",
      path: "/v1/downloads",
      headers: EXTENSION,
      body: { url: source, name, referer: "https://example.org/", mode: "direct" },
    });
    if (sent.status === 200 && sent.payload?.data?.id !== undefined) {
      handed.push(sent.payload.data);
    }
  }
  check(
    `the application took ${count} downloads at once`,
    handed.length === count,
    `it took ${handed.length}`,
  );

  /* While they run, the main thread is asked how long it needed. The worst
     answer is the one that matters: one long block freezes the window. */
  let worst = 0;
  let samples = 0;
  let frozen = false;
  const deadline = Date.now() + 90_000;
  while (Date.now() < deadline) {
    const beat = await call({ path: "/v1/health", headers: EXTENSION });
    if (beat.status !== 200) {
      check("the application answers while it downloads", false, `it answered ${beat.status}`);
      break;
    }
    const ms = beat.payload?.data?.mainThreadMs ?? 0;
    samples += 1;
    worst = Math.max(worst, ms);
    if (beat.payload?.data?.frozen) {
      frozen = true;
      break;
    }
    const listed = await call({ path: "/v1/downloads", headers: EXTENSION });
    const rows = listed.payload?.data?.downloads || [];
    const mine = rows.filter((row) =>
      handed.some((item) => item.id === row.id),
    );
    if (mine.length === handed.length && mine.every((row) => /complete|finished|done/i.test(row.status || ""))) {
      break;
    }
    await new Promise((done) => setTimeout(done, 250));
  }

  check(
    "the main thread never blocked",
    !frozen && worst < FROZEN_MS,
    `the worst answer was ${worst}ms over ${samples} checks`,
  );

  const listed = await call({ path: "/v1/downloads", headers: EXTENSION });
  const rows = listed.payload?.data?.downloads || [];
  const mine = rows.filter((row) => handed.some((item) => item.id === row.id));
  check(
    "every download finished",
    mine.length === handed.length &&
      mine.every((row) => /complete|finished|done/i.test(row.status || "")),
    mine.map((row) => row.status).join(", "),
  );
  check(
    "and every one of them wrote every byte",
    mine.every((row) => row.downloaded === PAYLOAD.length),
    mine.map((row) => row.downloaded).join(", "),
  );

  if (!keep) {
    for (const item of handed) {
      if (item.dest && existsSync(item.dest)) {
        rmSync(item.dest, { force: true });
      }
    }
  }

  const failed = results.filter((result) => !result.ok);
  console.log(`\nfreeze probe: ${results.length - failed.length}/${results.length} checks pass`);
  console.log(`  the main thread's worst answer: ${worst}ms over ${samples} checks`);
  for (const result of results) {
    console.log(`  ${result.ok ? "ok  " : "FAIL"} ${result.name}${result.ok || !result.detail ? "" : ` — ${result.detail}`}`);
  }
  assert.equal(failed.length, 0, "the application did not stay responsive");
} finally {
  server.close();
}
