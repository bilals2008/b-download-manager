import { createServer } from "node:http";
const BRIDGE = "http://127.0.0.1:27500";
const EXT = { "X-BDM-Client": "browser-extension", Origin: "moz-extension://probe-0000-0000" };
const server = createServer((req, res) => { res.writeHead(404); res.end(); });
await new Promise((d) => server.listen(0, "127.0.0.1", d));
const source = `http://127.0.0.1:${server.address().port}/missing.bin`;
const call = async (method, path, body) => {
  const r = await fetch(`${BRIDGE}${path}`, { method, headers: { ...EXT, ...(body ? { "Content-Type": "application/json" } : {}) }, ...(body ? { body: JSON.stringify(body) } : {}) });
  return r.json().catch(() => null);
};
const ids = [];
for (let i = 0; i < 8; i++) {
  const sent = await call("POST", "/v1/downloads", { url: source, name: `fail-${Date.now().toString(36)}-${i}.bin`, referer: "https://example.org/", mode: "direct" });
  if (sent?.data?.id !== undefined) ids.push(sent.data.id);
}
console.log("handed", ids.length);
const t0 = Date.now();
let worst = 0, samples = 0;
for (let s = 0; s < 25; s++) {
  const h = await call("GET", "/v1/health");
  const ms = h?.data?.mainThreadMs ?? -1;
  worst = Math.max(worst, ms); samples++;
  await new Promise((d) => setTimeout(d, 100));
}
console.log(`worst mainThread=${worst}ms over ${samples} samples`);
server.close();
