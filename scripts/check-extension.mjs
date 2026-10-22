// Self-check for the Firefox extension: the manifest points at files that
// exist, and the takeover rules do what the settings page promises. Node runs
// the extension's own settings.js in a sandbox, so the check tests the file
// the browser loads. Run it with `npm run check:extension`.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import vm from "node:vm";

const root = new URL("../extension-firefox/", import.meta.url);
const manifest = JSON.parse(readFileSync(new URL("manifest.json", root), "utf8"));

assert.equal(manifest.manifest_version, 3);
assert.ok(!manifest.background.service_worker, "Firefox has no MV3 service worker");
assert.deepEqual(manifest.background.scripts.slice(-1), ["background.js"], "the last background script is the entry");
for (const file of [
  ...manifest.background.scripts,
  manifest.action.default_popup,
  manifest.options_ui.page,
  manifest.action.default_icon,
]) {
  assert.ok(existsSync(new URL(file, root)), `${file} is in the manifest but not in the folder`);
}
for (const permission of ["downloads", "scripting", "activeTab", "contextMenus", "storage"]) {
  assert.ok(manifest.permissions.includes(permission), `the ${permission} permission is missing`);
}

const contentScripts = manifest.content_scripts ?? [];
assert.ok(contentScripts.length > 0, "a content script shows the download panel");
for (const entry of contentScripts) {
  assert.ok(entry.js.includes("panel.js"), "the panel is part of the content script");
  assert.ok(
    entry.matches.some((match) => match.startsWith("http")),
    "the panel runs on web pages",
  );
  for (const file of entry.js) {
    assert.ok(existsSync(new URL(file, root)), `${file} is in the manifest but not in the folder`);
  }
}
new vm.Script(readFileSync(new URL("panel.js", root), "utf8"), { filename: "panel.js" });

const missing = ["popup.html", "options.html"].flatMap((page) => {
  const html = readFileSync(new URL(page, root), "utf8");
  return [...html.matchAll(/(?:src|href)="([^"]+)"/g)]
    .map((match) => match[1])
    .filter((ref) => !existsSync(new URL(ref, root)))
    .map((ref) => `${page} -> ${ref}`);
});
assert.deepEqual(missing, [], "every script and stylesheet a page loads exists");

const context = vm.createContext({ URL });
vm.runInContext(readFileSync(new URL("settings.js", root), "utf8"), context);
const { normalizeSettings, shouldTakeOver, cleanSite } = context;

const settings = normalizeSettings({});
assert.equal(settings.takeover, true, "takeover is on by default");
assert.equal(settings.panel, true, "the page panel is on by default");
assert.deepEqual([...settings.types], [], "an empty list takes every type");
assert.deepEqual([...settings.skipSites], []);

assert.equal(normalizeSettings({ panel: false }).panel, false, "the page panel switch is honoured");

assert.ok(shouldTakeOver(settings, "https://example.org/a.bin", "https://example.org/page"));
assert.ok(!shouldTakeOver(settings, "blob:https://example.org/abc", "https://example.org/page"));

const typed = normalizeSettings({
  takeover: false,
  types: [".MP4", "mp4", " mkv ", "*.zip", ""],
  skipSites: ["https://Example.org/some/page", "example.net"],
});
assert.deepEqual([...typed.types], ["mp4", "mkv", "zip"], "dots, stars, case and duplicates are cleaned up");
assert.deepEqual([...typed.skipSites], ["example.org", "example.net"], "a full address is reduced to its host");
assert.ok(!shouldTakeOver(typed, "https://example.org/a.mp4", "https://example.org/"), "the switch wins");

const typedOn = { ...typed, takeover: true };
assert.ok(shouldTakeOver(typedOn, "https://files.org/a.mp4", "https://files.org/"), "a listed type from a site not skipped");
assert.ok(!shouldTakeOver(typedOn, "https://files.org/a.exe", "https://files.org/"), "a type off the list stays in the browser");
assert.ok(!shouldTakeOver(typedOn, "https://cdn.example.org/a.mp4", "https://www.example.org/"), "a subdomain follows its domain");
assert.ok(shouldTakeOver(typedOn, "https://files.org/a.mp4", "https://notexample.org/"), "a lookalike host is not skipped");
assert.ok(!shouldTakeOver(typedOn, "https://example.org/a.mp4", ""), "with no referrer the file host decides");
assert.ok(shouldTakeOver(typedOn, "https://cdn.files.org/a.mp4", ""), "and a file host not skipped is taken");

assert.equal(cleanSite("example.org/path"), "example.org");
assert.equal(cleanSite(""), "");
assert.equal(cleanSite("not a host"), "");
assert.deepEqual(
  ["!!!", "*.example.org", "HTTP://USER:PASS@Example.org:8080/x", "localhost", "127.0.0.1"].map(cleanSite),
  ["", "example.org", "example.org", "localhost", "127.0.0.1"],
  "only a real host survives, and a star is dropped",
);

console.log("extension: all checks pass");
