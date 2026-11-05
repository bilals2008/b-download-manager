// Palette gate. Reads the colour tokens straight out of src/index.css and
// checks the foreground/background pairs the product actually uses against
// WCAG 2.2, once per theme. Run it with `npm run check:palette`.
//
// Exits non-zero when a required pair fails, so a colour change cannot land
// without a contrast check in either appearance.

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, URL } from "node:url";

// The base block lives in `index.css`; every appearance block lives in its own
// file under `src/styles/themes/`.
const css = readFileSync(
  fileURLToPath(new URL("../src/index.css", import.meta.url)),
  "utf8",
);
const themeDir = fileURLToPath(
  new URL("../src/styles/themes/", import.meta.url),
);
const themeCss = readdirSync(themeDir)
  .filter((file) => file.endsWith(".css"))
  .sort()
  .map((file) => readFileSync(join(themeDir, file), "utf8"))
  .join("\n");

function tokensIn(text) {
  const map = {};
  for (const match of text.matchAll(/--([a-z-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)) {
    map[match[1]] = match[2];
  }
  return map;
}

// The base `:root` block is the dark appearance; every `:root[data-theme="…"]`
// block overrides a subset of it. Each selector is read on its own, so a block
// that comes later can never leak its values into an earlier appearance.
const blocks = [
  ...`${css}\n${themeCss}`.matchAll(
    /:root(?:\[data-theme="([a-z0-9-]+)"\])?\s*\{([^}]*)\}/g,
  ),
];
const baseBlock = blocks.find(([, name]) => !name);
if (!baseBlock) {
  console.error("MISSING  :root theme block");
  process.exit(1);
}
const base = tokensIn(baseBlock[2]);
const overrides = new Map(
  blocks
    .filter(([, name]) => name)
    .map(([, name, body]) => [name, tokensIn(body)]),
);

const APPEARANCES = [
  "dark",
  "light",
  "bloom",
  "cyberpunk",
  "volt",
  "softblack",
  "ayu",
  "graphite",
  "hive",
];
for (const name of APPEARANCES) {
  if (name !== "dark" && !overrides.has(name)) {
    console.error(`MISSING  :root[data-theme="${name}"] theme block`);
    process.exit(1);
  }
}
const THEMES = APPEARANCES.map((name) => [
  name,
  name === "dark" ? base : { ...base, ...overrides.get(name) },
]);

function channel(value) {
  const srgb = value / 255;
  return srgb <= 0.03928 ? srgb / 12.92 : ((srgb + 0.055) / 1.055) ** 2.4;
}

function luminance(hex) {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function ratio(a, b) {
  const la = luminance(a);
  const lb = luminance(b);
  const [hi, lo] = la > lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
}

// Foreground, background, minimum ratio, label.
const REQUIRED = [
  ["foreground", "background", 4.5, "body text on the page"],
  ["foreground", "card", 4.5, "body text on a card"],
  ["muted-foreground", "background", 4.5, "secondary text on the page"],
  ["muted-foreground", "card", 4.5, "secondary text on a card"],
  ["primary-foreground", "primary", 4.5, "text on the primary action"],
  ["secondary-foreground", "secondary", 4.5, "text on a secondary surface"],
  ["accent-foreground", "accent", 4.5, "text on an accent surface"],
  ["success-foreground", "success", 4.5, "text on the finished badge"],
  ["destructive-foreground", "destructive", 4.5, "text on a destructive action"],
  ["input", "background", 3, "control border on the page"],
  ["ring", "background", 3, "focus ring on the page"],
  ["chrome-title-text", "chrome-title", 4.5, "title bar text"],
  ["chrome-menu-text", "chrome-menu", 4.5, "menu bar text"],
  ["chrome-menu-text", "chrome-menu-hover", 4.5, "menu bar text, hovered"],
  ["chrome-menu-text", "chrome-menu-active", 4.5, "menu bar text, open"],
  ["chrome-toolbar-text", "chrome-toolbar", 4.5, "toolbar label"],
  ["chrome-toolbar-text", "chrome-toolbar-hover", 4.5, "toolbar label, hovered"],
  ["chrome-toolbar-text", "chrome-toolbar-active", 4.5, "toolbar label, pressed"],
  ["chrome-menu-text", "chrome-popup", 4.5, "menu item text"],
  ["chrome-menu-text", "chrome-popup-hover", 4.5, "menu item text, hovered"],
  ["chrome-muted", "chrome-popup", 4.5, "menu shortcut text"],
  ["chrome-muted", "chrome-popup-hover", 4.5, "menu shortcut text, hovered"],
  ["chrome-menu-text", "chrome-row-alt", 4.5, "table row text, zebra stripe"],
  ["chrome-muted", "chrome-row-alt", 4.5, "table row secondary text, zebra stripe"],
  ["chrome-focus", "chrome-toolbar", 3, "focus ring on the toolbar"],
  ["chrome-focus", "chrome-menu", 3, "focus ring on the menu bar"],
  ["chrome-focus", "chrome-popup", 3, "focus ring in a menu"],
  ["chrome-danger-text", "chrome-danger", 4.5, "close button text, hovered"],
  ["chrome-menu-text", "chrome-panel", 4.5, "category name in the categories pane"],
  ["chrome-muted", "chrome-panel", 4.5, "category count in the categories pane"],
  ["chrome-menu-text", "chrome-canvas", 4.5, "download list text"],
  ["chrome-muted", "chrome-canvas", 4.5, "download list secondary text"],
  ["chrome-menu-text", "chrome-menu-active", 4.5, "selected row text"],
  ["chrome-muted", "chrome-menu-active", 4.5, "selected row secondary text"],
  ["chrome-menu-text", "chrome-menu-hover", 4.5, "hovered row text"],
  ["chrome-muted", "chrome-menu-hover", 4.5, "hovered row secondary text"],
  ["chrome-focus", "chrome-panel", 3, "focus ring in the categories pane"],
  ["chrome-focus", "chrome-canvas", 3, "focus ring in the download list"],
  ["chrome-error", "chrome-popup", 4.5, "validation message in a dialog"],
  ["chrome-error", "chrome-panel", 4.5, "delete label in the selection bar"],
  ["input", "chrome-canvas", 3, "selection checkbox border in a row"],
  ["input", "chrome-toolbar", 3, "selection checkbox border in the header"],
  ["primary-foreground", "primary", 4.5, "tick in a picked checkbox"],
  ["chrome-menu-text", "chrome-popup", 4.5, "dialog text"],
  ["icon-green-bright", "chrome-canvas", 3, "downloading icon and bar"],
  ["progress", "chrome-canvas", 3, "downloading progress bar"],
  ["icon-green", "chrome-canvas", 3, "finished icon"],
  ["icon-red-bright", "chrome-canvas", 3, "error icon and bar"],
  ["icon-blue-bright", "chrome-canvas", 3, "paused icon and bar"],
  ["icon-purple-bright", "chrome-canvas", 3, "music category icon"],
  ["icon-amber", "chrome-canvas", 3, "queued and document icons"],
  ["icon-silver", "chrome-canvas", 3, "archive and other icons"],
];

// Every stop of the download meter's gradient must read as a graphical object
// on the list surface, in the plain row and in the selected row.
const METER_STOPS = [
  ["progress-active-from", "progress bar, emerald"],
  ["progress-active-via", "progress bar, cyan"],
  ["progress-active-to", "progress bar, blue"],
  ["progress-complete-from", "finished bar, start"],
  ["progress-complete-to", "finished bar, end"],
  ["progress-paused-from", "paused bar, start"],
  ["progress-paused-to", "paused bar, end"],
  ["progress-failed-from", "failed bar, start"],
  ["progress-failed-to", "failed bar, end"],
];
for (const [token, label] of METER_STOPS) {
  REQUIRED.push([token, "chrome-canvas", 3, label]);
  REQUIRED.push([token, "chrome-menu-active", 3, `${label}, selected row`]);
}

let failed = 0;
for (const [theme, tokens] of THEMES) {
  console.log(`\n${theme} appearance`);
  for (const [fg, bg, min, label] of REQUIRED) {
    const a = tokens[fg];
    const b = tokens[bg];
    if (!a || !b) {
      console.error(`MISSING  ${label}: --${fg} / --${bg}`);
      failed += 1;
      continue;
    }
    const value = ratio(a, b);
    const ok = value >= min;
    if (!ok) failed += 1;
    console.log(
      `${ok ? "PASS" : "FAIL"}  ${value.toFixed(2).padStart(5)}:1  (min ${min})  ${label}`,
    );
  }
}

console.log(
  failed === 0
    ? `\nall ${REQUIRED.length} pairs pass in all ${THEMES.length} appearances`
    : `\n${failed} of ${REQUIRED.length * THEMES.length} pairs fail`,
);
process.exit(failed === 0 ? 0 : 1);
