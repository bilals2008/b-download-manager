// The pages: no inline script or handler, and every reference resolves locally.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { manifest, rootUrl } from "./common.mjs";

for (const page of [
  manifest.action.default_popup,
  manifest.options_ui.page,
  "links/links.html",
]) {
  const htmlPath = new URL(page, rootUrl);
  const html = readFileSync(htmlPath, "utf8");

  assert.ok(
    !/<script(?![^>]*\bsrc=)[^>]*>[\s\S]*?<\/script>/i.test(html),
    `${page} has no inline script (the extension CSP would refuse it)`,
  );
  assert.ok(!/\son[a-z]+\s*=/i.test(html), `${page} has no inline event handler`);

  for (const match of html.matchAll(/<(\w+)\b[^>]*?\s(?:src|href)="([^"]+)"/g)) {
    const [, tag, ref] = match;
    if (ref.startsWith("#") || ref.startsWith("data:")) continue;
    // An anchor navigates away on a click; it loads nothing into the page.
    if (tag.toLowerCase() === "a" && /^https?:/i.test(ref)) continue;
    assert.ok(!/^https?:/i.test(ref), `${page} loads ${ref} from the network`);
    const target = new URL(ref, htmlPath);
    assert.ok(existsSync(fileURLToPath(target)), `${page} -> ${ref} exists`);
  }
}