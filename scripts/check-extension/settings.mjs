// lib/settings.js: normalisation, interception, site/type cleaning, overlay.

import assert from "node:assert/strict";
import { backgroundContext, store } from "./sandbox.mjs";

const {
  bdmNormalizeSettings,
  bdmShouldIntercept,
  bdmCleanSite,
  bdmCleanType,
  bdmLoadSettings,
  bdmAllowsOverlay,
} = backgroundContext;

const defaults = bdmNormalizeSettings({});
assert.equal(defaults.intercept, true, "interception is on by default");
assert.equal(defaults.overlay, true, "the video overlay is on by default");
assert.equal(defaults.ask, false, "a download starts without asking by default");
assert.equal(defaults.theme, "auto");
assert.deepEqual([...defaults.types], []);
assert.deepEqual([...defaults.skipSites], []);

const legacy = bdmNormalizeSettings({ takeover: false, panel: false });
assert.equal(legacy.intercept, false, "the old takeover switch is understood");
assert.equal(legacy.overlay, false, "the old panel switch is understood");

const typed = bdmNormalizeSettings({
  intercept: true,
  types: [".MP4", "mp4", " mkv ", "*.zip", ""],
  skipSites: ["https://Example.org/some/page", "example.net", "not a host"],
  theme: "dark",
});
assert.deepEqual([...typed.types], ["mp4", "mkv", "zip"], "types are cleaned and deduped");
assert.deepEqual([...typed.skipSites], ["example.org", "example.net"], "a site is a host");
assert.equal(typed.theme, "dark");
assert.equal(bdmNormalizeSettings({ theme: "sepia" }).theme, "auto", "an unknown theme falls back");

assert.ok(bdmShouldIntercept(defaults, "https://example.org/a.bin", "https://example.org/page"));
assert.ok(!bdmShouldIntercept(defaults, "blob:https://example.org/abc", "https://example.org/"));
assert.ok(!bdmShouldIntercept({ ...defaults, intercept: false }, "https://example.org/a.bin", ""));

const filtered = { ...defaults, types: ["mp4"], skipSites: ["example.org"] };
assert.ok(bdmShouldIntercept(filtered, "https://files.org/a.mp4", "https://files.org/"));
assert.ok(!bdmShouldIntercept(filtered, "https://files.org/a.exe", "https://files.org/"));
assert.ok(!bdmShouldIntercept(filtered, "https://example.org/a.mp4", "https://example.org/"));
assert.ok(!bdmShouldIntercept(filtered, "https://cdn.example.org/a.mp4", "https://www.example.org/"));
assert.ok(bdmShouldIntercept(filtered, "https://files.org/a.mp4", "https://notexample.org/"));
assert.ok(!bdmShouldIntercept(filtered, "https://example.org/a.mp4", ""));

assert.equal(bdmCleanSite("HTTP://USER:PASS@Example.org:8080/x"), "example.org");
assert.equal(bdmCleanSite("*.example.org"), "example.org");
assert.equal(bdmCleanSite("localhost"), "localhost");
assert.equal(bdmCleanSite("127.0.0.1"), "127.0.0.1");
assert.equal(bdmCleanSite("not a host"), "");
assert.equal(bdmCleanType("*.MP4"), "mp4");

assert.ok(bdmAllowsOverlay(defaults, "example.org"), "the overlay is on by default");
assert.ok(!bdmAllowsOverlay({ ...defaults, overlay: false }, "example.org"));
assert.ok(
  !bdmAllowsOverlay({ ...defaults, skipSites: ["example.org"] }, "cdn.example.org"),
  "an excluded site hides the overlay on its subdomains too",
);
assert.ok(
  bdmAllowsOverlay({ ...defaults, skipSites: ["example.org"] }, "notexample.org"),
  "an excluded site does not match a host that merely ends the same way",
);

store.intercept = false;
assert.equal((await bdmLoadSettings()).intercept, false, "the saved settings are read back");
delete store.intercept;