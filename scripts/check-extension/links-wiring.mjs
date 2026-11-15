// The link window's wiring: every control it names exists in the markup.

import assert from "node:assert/strict";
import { read } from "./common.mjs";

// The window is a page of its own; a control it names but the markup does not
// have would break it only once a page of links was open.
const linksHtml = read("links/links.html");
const linkIds = new Set(
  [...linksHtml.matchAll(/\sid="([^"]+)"/g)].map((match) => match[1]),
);
for (const id of [
  "send",
  "folder",
  "queue",
  "connections",
  "category",
  "extension",
  "pages",
  "links",
  "report",
  "selection",
]) {
  assert.ok(linkIds.has(id), `links.html has #${id}`);
}
for (const match of read("links/links.js").matchAll(/getElementById\("([^"]+)"\)/g)) {
  assert.ok(
    linkIds.has(match[1]),
    `links.js asks for #${match[1]}, which links.html does not define`,
  );
}
assert.ok(
  linksHtml.includes('../lib/link-rules.js') && linksHtml.includes("links.js"),
  "the window loads the rules and its own script",
);