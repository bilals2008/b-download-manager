#!/usr/bin/env node
/*
  A live check of the loopback bridge, against the application as it runs.

  The unit tests in `src-tauri/src/bridge` cover the rule set as a function; this
  covers the same rules over a real socket, and finishes with a real download:
  the probe serves one file of its own, hands its address to the application, and
  waits for the answer to say where the bytes went.

  It needs the desktop application running, and it reads the pairing code from the
  application's own file, so it must be run on the machine that has it.

    pnpm probe:bridge            # the rule matrix and one download
    pnpm probe:bridge --keep     # leave the downloaded file where it landed

  Pairing is trust-on-first-use: this probe presents its own extension origin, so
  it is the first caller the running application pins. After a run, press "Make a
  new code" in Settings > Browser integration, or start the probe before pairing a
  browser, so the pin belongs to the browser and not to the probe.
*/

import assert from "node:assert/strict";
import { createServer } from "node:http";
import { connect } from "node:net";
import { existsSync, readFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const PORT = 27500;
const BRIDGE = `http://127.0.0.1:${PORT}`;
/** The origin this probe asks with, as an installed extension would. */
const PROBE_ORIGIN = "moz-extension://probe-0000-0000";
const CLIENT = { "X-BDM-Client": "browser-extension" };
const keep = process.argv.includes("--keep");

/** The application's identifier, read from its own config rather than guessed. */
function appId() {
  const config = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
  return config.identifier;
}

/** Where the operating system keeps that application's files. */
function dataDir() {
  const id = appId();
  if (process.platform === "win32") return join(process.env.APPDATA, id);
  if (process.platform === "darwin") return join(process.env.HOME, "Library/Application Support", id);
  return join(process.env.XDG_CONFIG_HOME || join(process.env.HOME, ".config"), id);
}

function code() {
  const file = process.env.BDM_PAIRING_FILE || join(dataDir(), "browser-pairing.json");
  const pairing = JSON.parse(readFileSync(file, "utf8"));
  assert.ok(pairing.token?.length >= 32, `${file} holds no pairing code`);
  return { file, token: pairing.token };
}

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
  const { file, token } = code();
  console.log(`the app is on ${BRIDGE}, its pairing file is ${file}`);

  /* 1. The ping answers a caller that is not paired, and says so. */
  const ping = await call({ path: "/v1/ping", headers: CLIENT });
  check(
    "the application is running on the bridge port",
    ping.payload?.data?.app === "b-download-manager",
    `it answered ${ping.status}`,
  );
  check("ping without a code is answered", ping.status === 200, `got ${ping.status}`);
  check("and says the caller is not paired", ping.payload?.data?.paired === false);

  /* 2. Nothing else is. */
  const noCode = await call({
    method: "POST",
    path: "/v1/downloads",
    headers: CLIENT,
    body: { url: source },
  });
  check("a hand-over without the code is refused", noCode.status === 401, `got ${noCode.status}`);
  check("with the code the extension can read", noCode.payload?.error?.code === "unauthorized");

  const listWithout = await call({ path: "/v1/downloads", headers: CLIENT });
  check("the list without the code is refused", listWithout.status === 401);

  const focusWithout = await call({ method: "POST", path: "/v1/focus", headers: CLIENT, body: {} });
  check("the window cannot be raised without the code", focusWithout.status === 401);

  /* 3. A guessed code, and a header that is not the extension's. */
  const guessed = await call({
    path: "/v1/downloads",
    headers: { ...CLIENT, Authorization: `Bearer ${"0".repeat(64)}` },
  });
  check("a guessed code is refused", guessed.status === 401);

  const noClient = await call({
    path: "/v1/downloads",
    headers: { Authorization: `Bearer ${token}` },
  });
  check("the client header is still required", noClient.status === 403, `got ${noClient.status}`);

  /* 4. A page's origin, a borrowed host, a body that is not JSON. */
  const page = await call({
    path: "/v1/downloads",
    headers: { ...CLIENT, Origin: "https://example.org", Authorization: `Bearer ${token}` },
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
    headers: { ...CLIENT, Authorization: `Bearer ${token}`, "Content-Type": "text/plain" },
    body: JSON.stringify({ url: source }),
  });
  check("a body that is not JSON is refused", form.status === 415, `got ${form.status}`);

  const preflight = await call({ method: "OPTIONS", path: "/v1/downloads", headers: { Origin: "https://example.org" } });
  check("a preflight from a page is refused", preflight.status === 403);

  /* 5. The paired extension gets an answer, and its origin is the one pinned. */
  const prePin = JSON.parse(readFileSync(file, "utf8")).origin || "";
  if (prePin && prePin !== PROBE_ORIGIN) {
    console.log(
      `\nthe running app is already paired with ${prePin}; this probe will not be answered.\n` +
        "Press \"Make a new code\" in Settings > Browser integration and run it again.",
    );
  }

  const pairedPing = await call({
    path: "/v1/ping",
    headers: { ...CLIENT, Origin: PROBE_ORIGIN, Authorization: `Bearer ${token}` },
  });
  check(
    "the code unlocks the ping",
    pairedPing.payload?.data?.paired === true,
    !prePin || prePin === PROBE_ORIGIN ? `it answered ${pairedPing.status}` : "another extension holds the pairing",
  );

  const otherOrigin = await call({
    method: "POST",
    path: "/v1/downloads",
    headers: { ...CLIENT, Origin: "moz-extension://a-second-extension", Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: { url: source, name: "probe-second.bin" },
  });
  const pinned = await call({ path: "/v1/ping", headers: { ...CLIENT, Origin: PROBE_ORIGIN, Authorization: `Bearer ${token}` } });
  const pinFile = JSON.parse(readFileSync(file, "utf8"));
  check("the pairing file names one extension", pinFile.origin === PROBE_ORIGIN, `it says ${pinFile.origin}`);
  if (pinFile.origin === PROBE_ORIGIN) {
    check(
      "a second extension with the same code is refused",
      otherOrigin.status === 403,
      `got ${otherOrigin.status}`,
    );
    check("and the paired one is not disturbed", pinned.payload?.data?.paired === true);
  }

  /* 6. A real hand-over, then the download itself.

     A name of its own each run: a direct hand-over that finds the file already
     there is turned into a question for the window, which is right for a person
     and wrong for a probe that wants to watch the byte path. */
  const name = `bdm-bridge-probe-${Date.now().toString(36)}.bin`;
  const send = await call({
    method: "POST",
    path: "/v1/downloads",
    headers: { ...CLIENT, Origin: PROBE_ORIGIN, Authorization: `Bearer ${token}` },
    body: { url: source, name, referer: "https://example.org/", mode: "direct" },
  });
  check("the paired extension can hand a link over", send.status === 200, JSON.stringify(send.payload));
  const handed = send.payload?.data || {};
  check("the app took the link directly", handed.mode === "direct", `it answered ${handed.mode}`);
  check("the app answered with a destination", typeof handed.dest === "string" && handed.dest.length > 0);

  const finished = /complete|finished|done/i;
  let row = null;
  for (let attempt = 0; attempt < 60 && !finished.test(row?.status || ""); attempt += 1) {
    await new Promise((done) => setTimeout(done, 250));
    const list = await call({
      path: "/v1/downloads",
      headers: { ...CLIENT, Origin: PROBE_ORIGIN, Authorization: `Bearer ${token}` },
    });
    row = (list.payload?.data?.downloads || []).find((item) => item.id === handed.id) || row;
  }
  check("the download finished", finished.test(row?.status || ""), `status was ${row?.status}`);
  check("and the engine says it wrote every byte", row?.downloaded === PAYLOAD.length, `it says ${row?.downloaded}`);
  if (handed.dest) {
    const bytes = existsSync(handed.dest) ? readFileSync(handed.dest) : null;
    check("the file is on disk with the bytes it was given", bytes?.length === PAYLOAD.length, handed.dest);
  }

  /* 7. Control over that download, which a stranger must not have. */
  const pause = await call({
    method: "POST",
    path: `/v1/downloads/${handed.id}/pause`,
    headers: { ...CLIENT, Origin: PROBE_ORIGIN, Authorization: `Bearer ${token}` },
    body: {},
  });
  check("a paired control is answered", pause.status === 200 || pause.status === 409, `got ${pause.status}`);

  const strangerPause = await call({
    method: "POST",
    path: `/v1/downloads/${handed.id}/pause`,
    headers: CLIENT,
    body: {},
  });
  check("and the same control without the code is refused", strangerPause.status === 401);

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
