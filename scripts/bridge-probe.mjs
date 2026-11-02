#!/usr/bin/env node
/*
  A live check of the loopback bridge, against the application as it runs.

  The unit tests in `src-tauri/src/bridge` cover the rule set as a function; this
  covers the same rules over a real socket, and finishes with a real download:
  the probe serves one file of its own, hands its address to the application, and
  waits for the answer to say where the bytes went.

  It needs the desktop application running. There is no code to read or paste:
  the probe presents an extension origin and the extension's client header, the
  same two things the installed extension presents.

    pnpm probe:bridge            # the rule matrix and one download
    pnpm probe:bridge --keep     # leave the downloaded file where it landed
*/

import assert from "node:assert/strict";
import { createServer } from "node:http";
import { connect } from "node:net";
import { existsSync, readFileSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const PORT = 27500;
const BRIDGE = `http://127.0.0.1:${PORT}`;
/** The origin this probe asks with, as an installed extension would. */
const PROBE_ORIGIN = "moz-extension://probe-0000-0000";
const CLIENT = { "X-BDM-Client": "browser-extension" };
const EXTENSION = { ...CLIENT, Origin: PROBE_ORIGIN };
const keep = process.argv.includes("--keep");

/* One request to the bridge. A socket that cannot be opened is answer 0. */
async function call({ method = "GET", path, headers = {}, body } = {}) {
  try {
    const response = await fetch(`${BRIDGE}${path}`, {
      method,
      headers: { ...headers, ...(body === undefined ? {} : { "Content-Type": "application/json" }) },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    let payload = null;
    try {
      payload = await response.json();
    } catch {
      // An answer that is not JSON is its own result.
    }
    return { status: response.status, payload, headers: Object.fromEntries(response.headers) };
  } catch {
    return { status: 0, payload: null, headers: {} };
  }
}

/*
  One request written to the socket by hand.

  A browser picks its own `Host` header, and so does Node's `fetch` — it cannot
  be given another. The rebinding rule is about that header, so it is tested over
  a plain socket, the way a hostile page's browser would send it.
*/
function raw({ method, path, host, headers = {}, body }) {
  return new Promise((done) => {
    const socket = connect(PORT, "127.0.0.1", () => {
      const lines = [
        `${method} ${path} HTTP/1.1`,
        `Host: ${host}`,
        ...Object.entries(headers).map(([name, value]) => `${name}: ${value}`),
      ];
      const text = body === undefined ? "" : JSON.stringify(body);
      if (body !== undefined) lines.push(`Content-Length: ${Buffer.byteLength(text)}`);
      socket.end(`${lines.join("\r\n")}\r\n\r\n${text}`);
    });
    let answer = "";
    socket.setTimeout(5000, () => socket.destroy(new Error("the bridge did not answer")));
    socket.on("data", (chunk) => {
      answer += chunk;
    });
    socket.on("end", () => {
      done({ status: Number(/^HTTP\/1\.1 (\d+)/.exec(answer)?.[1] ?? 0), text: answer });
    });
    socket.on("error", () => {
      // Nothing is listening, which is answer 0, the same as a refused fetch.
      done({ status: 0, text: "" });
    });
  });
}

const results = [];
/** One line of the report. Nothing throws here, so one run shows every answer. */
function check(name, condition, detail) {
  results.push({ name, ok: Boolean(condition), detail });
}

/* The file the application is asked to download. */
const PAYLOAD = Buffer.alloc(96 * 1024, 0b01001100);
const server = createServer((request, response) => {
  if (request.url !== "/probe.bin") {
    response.writeHead(404).end("no");
    return;
  }
  const range = /bytes=(\d+)-(\d*)/.exec(request.headers.range || "");
  if (range) {
    const start = Number(range[1]);
    const end = range[2] === "" ? PAYLOAD.length - 1 : Math.min(Number(range[2]), PAYLOAD.length - 1);
    response.writeHead(206, {
      "Content-Range": `bytes ${start}-${end}/${PAYLOAD.length}`,
      "Content-Length": end - start + 1,
      "Accept-Ranges": "bytes",
    });
    response.end(PAYLOAD.subarray(start, end + 1));
    return;
  }
  response.writeHead(200, { "Content-Length": PAYLOAD.length, "Accept-Ranges": "bytes" });
  response.end(PAYLOAD);
});

await new Promise((done) => server.listen(0, "127.0.0.1", done));
const source = `http://127.0.0.1:${server.address().port}/probe.bin`;

try {
  console.log(`the app is on ${BRIDGE}`);

  /* 1. The ping names the app, and an extension is recognised at once. */
  const ping = await call({ path: "/v1/ping", headers: EXTENSION });
  check(
    "the application is running on the bridge port",
    ping.payload?.data?.app === "b-download-manager",
    `it answered ${ping.status}`,
  );
  check("an extension is connected without any code", ping.payload?.data?.connected === true);

  const list = await call({ path: "/v1/downloads", headers: EXTENSION });
  check("the extension can read the list", list.status === 200, `got ${list.status}`);

  /* 2. A caller that is not the extension is refused. */
  const noClient = await call({ path: "/v1/downloads", headers: { Origin: PROBE_ORIGIN } });
  check("the client header is required", noClient.status === 403, `got ${noClient.status}`);

  const handOverWithout = await call({
    method: "POST",
    path: "/v1/downloads",
    headers: { Origin: PROBE_ORIGIN },
    body: { url: source },
  });
  check("a hand-over without the header is refused", handOverWithout.status === 403);

  /* 3. A page's origin, a borrowed host, a body that is not JSON. */
  const page = await call({
    path: "/v1/downloads",
    headers: { ...CLIENT, Origin: "https://example.org" },
  });
  check("a web page origin is refused", page.status === 403);
  check("and a page is never echoed as allowed", !page.headers["access-control-allow-origin"]);

  const rebinding = await raw({
    method: "GET",
    path: "/v1/ping",
    // A browser that followed a name pointing back here would send this host.
    host: `evil.example:${PORT}`,
    headers: { "X-BDM-Client": "browser-extension" },
  });
  check("a host that is not the bridge is refused", rebinding.status === 403, `got ${rebinding.status}`);

  const form = await fetch(`${BRIDGE}/v1/downloads`, {
    method: "POST",
    headers: { ...EXTENSION, "Content-Type": "text/plain" },
    body: JSON.stringify({ url: source }),
  });
  check("a body that is not JSON is refused", form.status === 415, `got ${form.status}`);

  const preflight = await call({ method: "OPTIONS", path: "/v1/downloads", headers: { Origin: "https://example.org" } });
  check("a preflight from a page is refused", preflight.status === 403);

  /* 4. A real hand-over, then the download itself.

     A name of its own each run: a direct hand-over that finds the file already
     there is turned into a question for the window, which is right for a person
     and wrong for a probe that wants to watch the byte path. */
  const name = `bdm-bridge-probe-${Date.now().toString(36)}.bin`;
  const send = await call({
    method: "POST",
    path: "/v1/downloads",
    headers: EXTENSION,
    body: { url: source, name, referer: "https://example.org/", mode: "direct" },
  });
  check("the extension can hand a link over", send.status === 200, JSON.stringify(send.payload));
  const handed = send.payload?.data || {};
  check("the app took the link directly", handed.mode === "direct", `it answered ${handed.mode}`);
  check("the app answered with a destination", typeof handed.dest === "string" && handed.dest.length > 0);

  const finished = /complete|finished|done/i;
  let row = null;
  for (let attempt = 0; attempt < 60 && !finished.test(row?.status || ""); attempt += 1) {
    await new Promise((done) => setTimeout(done, 250));
    const listed = await call({ path: "/v1/downloads", headers: EXTENSION });
    row = (listed.payload?.data?.downloads || []).find((item) => item.id === handed.id) || row;
  }
  check("the download finished", finished.test(row?.status || ""), `status was ${row?.status}`);
  check("and the engine says it wrote every byte", row?.downloaded === PAYLOAD.length, `it says ${row?.downloaded}`);
  if (handed.dest) {
    const bytes = existsSync(handed.dest) ? readFileSync(handed.dest) : null;
    check("the file is on disk with the bytes it was given", bytes?.length === PAYLOAD.length, handed.dest);
  }

  /* 5. Control over that download. */
  const pause = await call({
    method: "POST",
    path: `/v1/downloads/${handed.id}/pause`,
    headers: EXTENSION,
    body: {},
  });
  check("a control from the extension is answered", pause.status === 200 || pause.status === 409, `got ${pause.status}`);

  const strangerPause = await call({
    method: "POST",
    path: `/v1/downloads/${handed.id}/pause`,
    headers: CLIENT,
    body: {},
  });
  check("and the same control without the header is refused", strangerPause.status === 403);

  if (handed.dest) {
    console.log(`\nthe file it wrote: ${handed.dest}`);
    if (!keep) {
      rmSync(handed.dest, { force: true });
      console.log("  removed again; re-run with --keep to leave it");
    }
  }

  const failed = results.filter((result) => !result.ok);
  console.log(`\nbridge probe: ${results.length - failed.length}/${results.length} checks pass`);
  for (const result of results) {
    console.log(`  ${result.ok ? "ok  " : "FAIL"} ${result.name}${result.ok || !result.detail ? "" : ` — ${result.detail}`}`);
  }
  assert.equal(failed.length, 0, "the live bridge did not answer as the rules say");
} finally {
  server.close();
}
