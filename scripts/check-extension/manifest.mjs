// The Firefox manifest: version, permissions, hosts, icons and content scripts.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { manifest, read, exists, rootUrl, pngSize } from "./common.mjs";

assert.equal(manifest.manifest_version, 3, "Firefox uses MV3 here");
assert.ok(!manifest.background.service_worker, "Firefox has no MV3 service worker");
assert.equal(
  manifest.background.scripts.at(-1),
  "background.js",
  "the background entry runs last",
);
assert.deepEqual(
  manifest.background.scripts.slice(0, 4),
  ["lib/media.js", "lib/link-rules.js", "lib/settings.js", "lib/bridge.js"],
  "the background gets its media, link, settings and bridge helpers first",
);

assert.deepEqual(
  [...manifest.permissions].sort(),
  ["alarms", "contextMenus", "cookies", "downloads", "storage"],
  "the extension asks for the permissions it uses, and no more",
);
assert.ok(
  read("background.js").includes("alarms.create("),
  "the alarms permission is there for the settings check that reads it",
);
assert.deepEqual(
  manifest.host_permissions,
  ["http://127.0.0.1:27500/*", "http://*/*", "https://*/*"],
  "the bridge, plus the sites whose cookies and player requests it reads",
);
assert.ok(!manifest.permissions.includes("scripting"));
assert.ok(!manifest.permissions.includes("activeTab"));
assert.ok(!manifest.permissions.includes("tabs"));

assert.equal(manifest.browser_specific_settings.gecko.id, "idm-bridge@my-idm-clone.local");
assert.ok(Number(manifest.version.split(".")[0]) >= 1, "the upgrade is a major version");

// The add-on store and MV3 both reject or warn about these, so keep them out
// of the Firefox build rather than discovering it at submission time.
assert.equal(manifest.background.service_worker, undefined, "Firefox has no service worker");
assert.equal(manifest.chrome_settings_overrides, undefined, "no Chrome-only settings key");
assert.equal(manifest.event_pages, undefined, "no legacy event page key");
const gecko = manifest.browser_specific_settings.gecko;
assert.ok(
  Number.parseInt(gecko.strict_min_version, 10) >= 121,
  "MV3 Firefox needs strict_min_version 121 or newer",
);
assert.ok(gecko.data_collection_permissions, "declare data_collection_permissions");
assert.ok(!(manifest.host_permissions ?? []).includes("<all_urls>"), "list explicit hosts");

for (const file of [
  ...manifest.background.scripts,
  manifest.action.default_popup,
  manifest.options_ui.page,
  ...Object.values(manifest.action.default_icon),
  ...Object.values(manifest.icons),
]) {
  assert.ok(exists(file), `${file} is in the manifest but not in the folder`);
}

// A resized or mis-keyed icon is a real submission failure; the file name is
// not evidence, the bytes are.
for (const [size, file] of Object.entries({
  ...manifest.icons,
  ...manifest.action.default_icon,
})) {
  const pixels = pngSize(readFileSync(new URL(file, rootUrl)));
  assert.deepEqual(
    pixels,
    { width: Number(size), height: Number(size) },
    `${file} should be ${size}x${size}`,
  );
}

const contentScripts = manifest.content_scripts ?? [];
assert.equal(contentScripts.length, 1, "one content script");
const contentScript = contentScripts[0];
assert.deepEqual(contentScript.matches, ["http://*/*", "https://*/*"]);
assert.equal(contentScript.run_at, "document_idle");
assert.ok(!contentScript.all_frames, "the overlay runs in the top frame only");
assert.ok(contentScript.js.includes("content/overlay.js"), "the overlay is injected");
assert.ok(
  contentScript.js.includes("content/page-sources.js"),
  "the page source reader is injected",
);
assert.ok(
  contentScript.js.indexOf("content/page-media.js") <
    contentScript.js.indexOf("content/page-sources.js") &&
    contentScript.js.indexOf("content/page-sources.js") <
      contentScript.js.indexOf("content/overlay.js"),
  "page sources load after the capture store and before the overlay",
);
assert.ok(
  contentScript.js.includes("content/video-registry.js"),
  "the video registry is injected",
);
assert.ok(
  contentScript.js.includes("content/page-links.js"),
  "the page link reader is injected",
);
assert.ok(
  contentScript.js.includes("lib/link-rules.js"),
  "the link rules reach the content script that classifies links",
);
assert.ok(
  contentScript.js.indexOf("lib/link-rules.js") <
    contentScript.js.indexOf("content/page-links.js"),
  "the rules load before the reader that uses them",
);
assert.ok(
  contentScript.js.indexOf("content/video-registry.js") <
    contentScript.js.indexOf("content/overlay.js"),
  "the registry loads before the overlay that asks it for a video",
);
for (const file of contentScript.js) {
  assert.ok(exists(file), `${file} is in the manifest but not in the folder`);
}