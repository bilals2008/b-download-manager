// lib/link-rules.js: extension, category, kind, classify, key and skip label.

import assert from "node:assert/strict";
import { backgroundContext } from "./sandbox.mjs";

const {
  bdmLinkClassify,
  bdmLinkCategory,
  bdmLinkExtension,
  bdmLinkKind,
  bdmLinkKey,
  bdmLinkAppCategory,
  bdmLinkSkipLabel,
} = backgroundContext;

assert.equal(bdmLinkExtension("https://example.org/a.MP4?x=1"), "mp4");
assert.equal(bdmLinkExtension("https://example.org/get?id=1", "report.pdf"), "pdf");
assert.equal(bdmLinkExtension("https://example.org/get?id=1"), "");
assert.equal(bdmLinkExtension("https://example.org/archive.tar.gz"), "gz");
assert.equal(bdmLinkExtension("https://example.org/.env"), "");
assert.equal(
  bdmLinkExtension("https://example.org/My%20Film%2Emp4"),
  "mp4",
  "an encoded dot still names the extension",
);

assert.equal(bdmLinkCategory("zip"), "Compressed");
assert.equal(bdmLinkCategory("mp4"), "Video");
assert.equal(bdmLinkCategory("jpg"), "Images");
assert.equal(bdmLinkCategory("exe"), "Programs");
assert.equal(bdmLinkCategory("pdf"), "Documents");
assert.equal(bdmLinkCategory("opus"), "Other", "a type the app has no rule for is Other");
assert.equal(bdmLinkAppCategory("Images"), "Other", "the app's tree has no Images category");
assert.equal(bdmLinkAppCategory("Video"), "Video");

assert.equal(bdmLinkKind("zip"), "file");
assert.equal(bdmLinkKind(""), "page");
assert.equal(bdmLinkKind("html"), "page");
assert.equal(bdmLinkKind("m3u8"), "unsupported", "a manifest is not a file");
assert.equal(bdmLinkKind("ts"), "unsupported");

const zipLink = bdmLinkClassify({ href: "https://example.org/files/a.zip" }, "https://example.org/");
assert.equal(zipLink.ok, true);
assert.equal(zipLink.item.category, "Compressed");
assert.equal(zipLink.item.badge, "ZIP");
assert.equal(zipLink.item.name, "a.zip");

const namedLink = bdmLinkClassify(
  { href: "https://example.org/get?id=7", download: "report.docx" },
  "https://example.org/",
);
assert.equal(namedLink.ok, true, "the download name is what makes an extensionless link a file");
assert.equal(namedLink.item.extension, "docx");
assert.equal(namedLink.item.name, "report.docx");
assert.equal(namedLink.item.kind, "file");

const pageLink = bdmLinkClassify({ href: "https://example.org/about" }, "https://example.org/");
assert.equal(pageLink.ok, true);
assert.equal(pageLink.item.kind, "page");
assert.equal(pageLink.item.badge, "PAGE");

assert.equal(
  bdmLinkClassify({ href: "https://example.org/files/page.html#top" }, "https://example.org/files/page.html").reason,
  "self",
  "a link to this page is not a download",
);
assert.equal(
  bdmLinkClassify({ href: "https://example.org/stream.m3u8" }, "https://example.org/").reason,
  "stream",
  "a manifest the engine cannot save is reported, not offered",
);
assert.equal(
  bdmLinkClassify({ href: "mailto:someone@example.org" }, "https://example.org/").reason,
  "scheme",
);
assert.equal(
  bdmLinkClassify({ href: "javascript:alert(1)" }, "https://example.org/").reason,
  "scheme",
);
assert.equal(bdmLinkClassify({ href: "" }, "https://example.org/").reason, "unusable");

assert.equal(
  bdmLinkKey("https://example.org/a.zip#again"),
  bdmLinkKey("https://example.org/a.zip"),
  "a fragment is not a second file",
);
assert.notEqual(
  bdmLinkKey("https://example.org/a.zip?v=1"),
  bdmLinkKey("https://example.org/a.zip?v=2"),
  "a different query may be a different file",
);
assert.equal(bdmLinkSkipLabel("stream"), "stream manifests the engine cannot save");