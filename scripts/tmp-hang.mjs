import { createServer } from "node:http";
const BRIDGE = "http://127.0.0.1:27500";
const EXT = { "X-BDM-Client": "browser-extension", Origin: "moz-extension://probe-0000-0000" };
const server = createServer(() => {});
await new Promise((d) => server.listen(0, "127.0.0.1", d));
const source = `http://127.0.0.1:${server.address().port}/hang.bin`;
const call = async (method, path, body) => {
  const r = await fetch(`${BRIDGE}${path}`, { method, headers: { ...EXT, ...(body ? { "Content-Type": "application/json" } : {}) }, ...(body ? { body: JSON.stringify(body) } : {}) });
  return r.json().catch(() => null);
};
for (let i = 0; i < 8; i++) {
  await call("POST", "/v1/downloads", { url: source, name: `hang-${i}.bin`, referer: "https://example.org/", mode: "direct" });
}
console.log("handed 8");
let worst = 0, samples = 0;
for (let s = 0; s < 20; s++) {
  const h = await call("GET", "/v1/health");
  const ms = h?.data?.mainThreadMs ?? -1;
  worst = Math.max(worst, ms); samples++;
  await new Promise((d) => setTimeout(d, 100));
}
console.log(`worst mainThread=${worst}ms over ${samples} samples`);
process.exit(0);
