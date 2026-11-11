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
let worst = 0, samples = 0;
const t0 = Date.now();
for (let i = 0; i < 8; i++) {
  const before = Date.now();
  const sent = await call("POST", "/v1/downloads", { url: source, name: `stagger-${i}.bin`, referer: "https://example.org/", mode: "direct" });
  const postMs = Date.now() - before;
  console.log(`POST ${i} took ${postMs}ms`);
  // sample health right after each post
  const h = await call("GET", "/v1/health");
  const ms = h?.data?.mainThreadMs ?? -1;
  worst = Math.max(worst, ms); samples++;
  console.log(`  health after POST ${i}: ${ms}ms (elapsed ${((Date.now()-t0)/1000).toFixed(1)}s)`);
  await new Promise((d) => setTimeout(d, 500));
}
console.log(`worst=${worst}ms samples=${samples}`);
server.close();
