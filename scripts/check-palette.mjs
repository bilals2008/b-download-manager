// Palette gate. Reads the colour tokens straight out of src/index.css and
// checks the foreground/background pairs the product actually uses against
// WCAG 2.2, once per theme. Run it with `npm run check:palette`.
//
// Exits non-zero when a required pair fails, so a colour change cannot land
// without a contrast check in either appearance.

import { readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

const css = readFileSync(
  fileURLToPath(new URL("../src/index.css", import.meta.url)),
  "utf8",
);

function tokensIn(text) {
  const map = {};
  for (const match of text.matchAll(/--([a-z-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)) {
    map[match[1]] = match[2];
  }
  return map;
}

// The dark tokens are the base; the light block overrides a subset of them.
const lightStart = css.indexOf(':root[data-theme="light"]');
if (lightStart === -1) {
  console.error('MISSING  :root[data-theme="light"] theme block');
  process.exit(1);
}
const dark = tokensIn(css.slice(0, lightStart));
const light = { ...dark, ...tokensIn(css.slice(lightStart)) };
const THEMES = [
  ["dark", dark],
  ["light", light],
];

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
  ["icon-green", "chrome-canvas", 3, "finished icon"],
  ["icon-red-bright", "chrome-canvas", 3, "error icon and bar"],
  ["icon-blue-bright", "chrome-canvas", 3, "paused icon and bar"],
  ["icon-purple-bright", "chrome-canvas", 3, "music category icon"],
  ["icon-amber", "chrome-canvas", 3, "queued and document icons"],
  ["icon-silver", "chrome-canvas", 3, "archive and other icons"],
];

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
    ? `\nall ${REQUIRED.length} pairs pass in both appearances`
    : `\n${failed} of ${REQUIRED.length * THEMES.length} pairs fail`,
);
process.exit(failed === 0 ? 0 : 1);
