// content/page-links.js: the page scan, its caps and its progress messages.

import assert from "node:assert/strict";
import vm from "node:vm";
import { read } from "./common.mjs";

const scanAnchors = [
  { href: "https://example.org/files/a.zip" },
  { href: "https://example.org/files/a.zip#again" },
  { href: "https://example.org/files/b.pdf" },
  { href: "https://example.org/files/clip.mp4" },
  { href: "https://example.org/get?id=7", download: "report.docx" },
  { href: "https://example.org/stream.m3u8" },
  { href: "mailto:someone@example.org" },
  { href: "https://example.org/files/page.html#top" },
  { href: "https://example.org/about" },
  { href: "https://example.org/files/photo.JPG" },
];

const scanMessages = [];
let scanListener;
const scanContext = vm.createContext({
  URL,
  console,
  performance,
  setTimeout,
  clearTimeout,
  location: {
    href: "https://example.org/files/page.html",
    hostname: "example.org",
  },
  document: {
    title: "Files",
    links: scanAnchors,
  },
  browser: {
    runtime: {
      sendMessage: async (message) => {
        scanMessages.push(message);
        return { ok: true };
      },
      onMessage: {
        addListener: (fn) => {
          scanListener = fn;
        },
      },
    },
  },
});
vm.runInContext(read("lib/link-rules.js"), scanContext, { filename: "link-rules.js" });
vm.runInContext(read("content/page-links.js"), scanContext, { filename: "page-links.js" });

const scan = await scanContext.bdmScanPageLinks("scan-1", () => {});
assert.equal(scan.ok, true);
assert.equal(scan.stats.scanned, scanAnchors.length);
assert.equal(scan.stats.files, 5, "five files are offered");
assert.equal(scan.stats.pages, 1, "the extensionless link is a page");
assert.equal(scan.stats.duplicates, 1, "the same address with a fragment is listed once");
assert.equal(scan.stats.skipped.stream, 1);
assert.equal(scan.stats.skipped.scheme, 1);
assert.equal(scan.stats.skipped.self, 1);
assert.equal(scan.stats.truncated, false);
assert.equal(scan.page.title, "Files");
assert.equal(scan.page.host, "example.org");
assert.equal(
  scan.links.filter((item) => item.url.endsWith("a.zip")).length,
  1,
  "a duplicate address is kept once",
);
const photo = scan.links.find((item) => item.name === "photo.JPG");
assert.equal(photo.category, "Images");
assert.equal(photo.badge, "JPG");
assert.ok(photo.key, "a link carries the key the window selects it by");
assert.ok(
  !scan.links.some((item) => item.url.startsWith("https://example.org/stream")),
  "a stream manifest is never offered",
);

// A page with thousands of links is capped, not copied whole. The anchor cap
// stops the walk; the result cap stops it earlier when every address is new.
scanContext.document.links = Array.from({ length: 9000 }, (_value, index) => ({
  href: `https://example.org/files/repeat${index % 2}.zip`,
}));
const repeated = await scanContext.bdmScanPageLinks("scan-repeat", () => {});
assert.equal(repeated.stats.total, 9000);
assert.equal(repeated.stats.scanned, 6000, "the scan stops at its anchor cap");
assert.equal(repeated.links.length, 2, "two addresses are kept however often they repeat");
assert.equal(repeated.stats.truncated, true, "a capped scan says it was capped");

scanContext.document.links = Array.from({ length: 9000 }, (_value, index) => ({
  href: `https://example.org/files/f${index}.zip`,
}));
const bigScan = await scanContext.bdmScanPageLinks("scan-big", () => {});
assert.equal(bigScan.links.length, 400, "only the first 400 addresses are kept");
assert.equal(bigScan.stats.scanned, 400, "the scan stops once the window is full");
assert.equal(bigScan.stats.truncated, true);
scanContext.document.links = scanAnchors;

const scanAnswer = await scanListener({ type: "bdm:links-scan", scanId: "scan-2" });
assert.equal(scanAnswer.ok, true);
assert.ok(
  scanMessages.some((message) => message.type === "bdm:links-progress"),
  "the reader reports its progress to the window",
);
assert.equal(scanListener({ type: "bdm:media-list" }), undefined, "other messages are left alone");