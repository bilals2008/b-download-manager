import { createServer } from "node:http";

const PORT = 27500;
const BRIDGE = `http://127.0.0.1:${PORT}`;
const EXT = { "X-BDM-Client": "browser-extension", Origin: "moz-extension://probe-0000-0000" };

const CHUNKS = 40;
const CHUNK_SIZE = 64 * 1024;
const PAYLOAD = Buffer.alloc(CHUNKS * CHUNK_SIZE, 0b01001100);

const server = createServer(async (req, res) => {
  const range = /bytes=(\d+)-(\d*)/.exec(req.headers.range || "");
  const start = range ? Number(range[1]) : 0;
  const end = range ? Math.min(Number(range[2] || PAYLOAD.length - 1), PAYLOAD.length - 1) : PAYLOAD.length - 1;
  const length = end - start + 1;
  res.writeHead(range ? 206 : 200, {
    ...(range ? { "Content-Range": `bytes ${start}-${end}/${PAYLOAD.length}` } : {}),
    "Content-Length": length,
    "Accept-Ranges": "bytes",
  });
  let written = 0;
  while (written < length) {
    const take = Math.min(CHUNK_SIZE, length - written);
    if (!res.write(PAYLOAD.subarray(start + written, start + written + take))) {
      await new Promise((d) => res.once("drain", d));
    }
    written += take;
    await new Promise((d) => setTimeout(d, 25));
  }
  res.end();
});
await new Promise((d) => server.listen(0, "127.0.0.1", d));
const source = `http://127.0.0.1:${server.address().port}/freeze.bin`;

const call = async (method, path, body) => {
  const r = await fetch(`${BRIDGE}${path}`, {
    method,
    headers: { ...EXT, ...(body ? { "Content-Type": "application/json" } : {}) },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  return r.json().catch(() => null);
};

const handed = [];
for (let i = 0; i < 8; i++) {
  const name = `tl-${Date.now().toString(36)}-${i}.bin`;
  const sent = await call("POST", "/v1/downloads", { url: source, name, referer: "https://example.org/", mode: "direct" });
  if (sent?.data?.id !== undefined) handed.push(sent.data.id);
}
console.log(`t=0 handed ${handed.length} downloads`);

const t0 = Date.now();
let lastLine = "";
const deadline = t0 + 30000;
while (Date.now() < deadline) {
  const h = await call("GET", "/v1/health");
  const d = await call("GET", "/v1/downloads");
  const rows = d?.data?.downloads || [];
  const mine = rows.filter((r) => handed.includes(r.id));
  const done = mine.filter((r) => /complete|finished|done/i.test(r.status || "")).length;
  const running = mine.filter((r) => /running/i.test(r.status || "")).length;
  const ms = h?.data?.mainThreadMs ?? -1;
  const frozen = h?.data?.frozen ?? false;
  const line = `t=${((Date.now() - t0) / 1000).toFixed(1)}s mainThread=${ms}ms frozen=${frozen} running=${running} done=${done}`;
  if (line !== lastLine) {
    console.log(line);
    lastLine = line;
  }
  if (mine.length === handed.length && done === handed.length) break;
  await new Promise((d) => setTimeout(d, 100));
}
server.close();
console.log("done");
