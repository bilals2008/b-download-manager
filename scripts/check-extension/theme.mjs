// Theme tokens: the overlay's dark and light tokens match theme.css.

import assert from "node:assert/strict";
import vm from "node:vm";
import { read } from "./common.mjs";

function cssVariables(text, selector) {
  const start = text.indexOf(selector);
  assert.ok(start >= 0, `the stylesheet has ${selector}`);
  const open = text.indexOf("{", start);
  const close = text.indexOf("}", open);
  const body = text.slice(open + 1, close);
  const map = new Map();
  for (const match of body.matchAll(/--([a-z0-9-]+):\s*([^;]+);/gi)) {
    map.set(match[1].toLowerCase(), match[2].replace(/\s+/g, " ").trim().toLowerCase());
  }
  return map;
}

const themeCss = read("theme.css");
const themeDark = cssVariables(themeCss, ":root {");
const themeLight = cssVariables(themeCss, ':root[data-bdm-theme="light"] {');

const overlayContext = vm.createContext({});
vm.runInContext(read("content/overlay-style.js"), overlayContext, {
  filename: "overlay-style.js",
});
const overlayCss = vm.runInContext("BDM_OVERLAY_STYLE", overlayContext);
const overlayDark = cssVariables(overlayCss, ":host {");
const overlayLight = cssVariables(overlayCss, ':host([data-theme="light"]) {');

const shared = (overlayName, themeName) => [overlayName, themeName];

const darkPairs = [
  shared("canvas", "canvas"),
  shared("text", "text"),
  shared("muted", "text-muted"),
  shared("line", "line"),
  shared("hover", "hover"),
  shared("active", "active"),
  shared("primary", "primary"),
  shared("on-primary", "on-primary"),
  shared("progress", "progress"),
  shared("success", "success"),
  shared("danger", "danger"),
  shared("focus", "focus"),
];

for (const [overlayName, themeName] of darkPairs) {
  assert.equal(
    overlayDark.get(`bdm-${overlayName}`),
    themeDark.get(themeName),
    `the dark overlay token --bdm-${overlayName} matches theme.css --${themeName}`,
  );
}

const lightPairs = [
  // The floating panel is a raised surface over a video, so its canvas is the
  // light surface, not the window canvas behind it.
  shared("canvas", "surface"),
  ...darkPairs.slice(1),
];

for (const [overlayName, themeName] of lightPairs) {
  assert.equal(
    overlayLight.get(`bdm-${overlayName}`),
    themeLight.get(themeName),
    `the light overlay token --bdm-${overlayName} matches theme.css --${themeName}`,
  );
}