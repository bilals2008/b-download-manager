// Self-check for the toolbar model: the layout rules, the remembered choice,
// and the pack reader. Node strips the types, so the check runs against the
// same modules the window imports. Run it with `pnpm run check:toolbars`.

import assert from "node:assert/strict";

const store = new Map();
globalThis.window = {
  localStorage: {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => store.set(key, String(value)),
    removeItem: (key) => store.delete(key),
  },
};

const {
  DEFAULT_TOOLBAR_ID,
  DEFAULT_TOOLBAR_LAYOUT,
  TOOLBAR_CATALOG,
  TOOLBAR_PREFERENCE_KEY,
  defaultToolbarPreference,
  loadToolbarPreference,
  normalizeToolbarLayout,
  resolveToolbarLayout,
  saveToolbarPreference,
  withActiveToolbar,
  withToolbarLabels,
  withToolbarLayout,
} = await import("../src/lib/toolbars.ts");

const {
  BUILTIN_TOOLBAR_PACKS,
  DEFAULT_TOOLBAR_PACK,
  defaultToolbarLayoutFor,
  normalizeToolbarPack,
} = await import("../src/lib/toolbar-packs.ts");

let checks = 0;

function is(actual, expected, label) {
  assert.deepEqual(actual, expected, label);
  checks += 1;
}

function reset() {
  store.clear();
}

/* --------------------------------------------------------------- catalog */

is(
  TOOLBAR_CATALOG.map((item) => item.command),
  [...DEFAULT_TOOLBAR_LAYOUT],
  "the default layout draws every catalogue button, in catalogue order",
);
is(new Set(DEFAULT_TOOLBAR_LAYOUT).size, DEFAULT_TOOLBAR_LAYOUT.length, "no button repeats");
is(
  TOOLBAR_CATALOG.every((item) => item.label && item.command),
  true,
  "every button has a command and a label",
);

/* ---------------------------------------------------------------- layout */

is(
  normalizeToolbarLayout(["resume", "nope", "resume", "stop"]),
  ["resume", "stop"],
  "unknown and repeated buttons are dropped",
);
is(
  normalizeToolbarLayout(["separator", "resume", "separator", "separator", "stop", "separator"]),
  ["resume", "separator", "stop"],
  "separators are kept between groups, never at an edge or beside each other",
);
is(normalizeToolbarLayout([], ["resume"]), ["resume"], "an empty draft falls back");
is(normalizeToolbarLayout("junk"), [...DEFAULT_TOOLBAR_LAYOUT], "junk falls back to the default bar");
is(
  normalizeToolbarLayout(["separator", "separator"]),
  [...DEFAULT_TOOLBAR_LAYOUT],
  "separators alone are not a toolbar",
);
is(
  normalizeToolbarLayout(new Array(80).fill("resume").map((_, index) => index % 2 ? "stop" : "resume")).length,
  2,
  "a long list cannot repeat its way past the catalogue",
);
is(normalizeToolbarLayout(["separator", "resume"]), ["resume"], "a leading separator is dropped");

/* ------------------------------------------------------------- storage */

reset();
is(
  loadToolbarPreference(),
  { active: DEFAULT_TOOLBAR_ID, layouts: {}, labels: true },
  "an empty profile uses the default bar with labels",
);

const saved = {
  active: "classic",
  layouts: { classic: ["resume", "separator", "stop"] },
  labels: false,
};
saveToolbarPreference(saved);
is(loadToolbarPreference(), saved, "the saved pack and layout read back");

store.set(
  TOOLBAR_PREFERENCE_KEY,
  JSON.stringify({
    active: "BAD ID",
    layouts: {
      classic: ["resume", "nope", "resume"],
      "../escape": ["resume"],
      broken: [],
    },
  }),
);
is(
  loadToolbarPreference(),
  { active: DEFAULT_TOOLBAR_ID, layouts: { classic: ["resume"] }, labels: true },
  "a bad pack id, a traversal name and an empty layout are dropped",
);

store.set(TOOLBAR_PREFERENCE_KEY, "not json");
is(loadToolbarPreference(), defaultToolbarPreference(), "broken storage falls back");

reset();
const preference = defaultToolbarPreference();
is(withActiveToolbar(preference, "classic").active, "classic", "switching packs changes the active id");
is(withActiveToolbar(preference, "bad id"), preference, "a malformed id is refused");
is(withToolbarLabels(preference, false), { ...preference, labels: false }, "labels can be turned off");
is(withToolbarLabels(preference, true), preference, "an unchanged label choice is not a new object");
is(withToolbarLayout(preference, "classic", []), { ...preference, layouts: { classic: [...DEFAULT_TOOLBAR_LAYOUT] } }, "an empty customization becomes the default bar");

is(
  resolveToolbarLayout({ active: "3d", layouts: {} }, "3d", ["resume", "stop"]),
  ["resume", "stop"],
  "a pack's own buttons are used before the default",
);
is(
  resolveToolbarLayout(
    { active: "3d", layouts: { "3d": ["stop"] } },
    "3d",
    ["resume", "stop"],
  ),
  ["stop"],
  "the reader's order wins over the pack's suggestion",
);

/* ----------------------------------------------------------------- packs */

is(
  BUILTIN_TOOLBAR_PACKS.map((pack) => pack.id),
  ["default", "3d", "classic", "isometric", "blueprint"],
  "five bars ship with the app",
);
is(DEFAULT_TOOLBAR_PACK.id, DEFAULT_TOOLBAR_ID, "the fallback pack is the default one");

for (const builtin of BUILTIN_TOOLBAR_PACKS.slice(1)) {
  is(builtin.style.glow, undefined, `${builtin.name} is a plain bar, not a glow`);
  is(
    defaultToolbarLayoutFor(builtin),
    [...DEFAULT_TOOLBAR_LAYOUT],
    `${builtin.name} keeps the standard buttons`,
  );
  for (const command of DEFAULT_TOOLBAR_LAYOUT) {
    const source = builtin.icons[command];
    assert.ok(
      typeof source === "string" && source.startsWith("data:image/svg+xml"),
      `${builtin.name} draws ${command}`,
    );
    checks += 1;
  }
}

const pack = normalizeToolbarPack({
  id: "my-bar",
  name: "  My Bar  ",
  version: "1.2.0",
  author: "Someone",
  description: "A test bar.",
  style: { accent: "#22e7ff", background: "red", glow: true },
  buttons: ["resume", "resume", "separator", "stop"],
});
is(pack.name, "My Bar", "the pack name is trimmed");
is(pack.style, { accent: "#22e7ff", glow: true }, "only hex colours reach the style");
is(pack.buttons, ["resume", "separator", "stop"], "pack buttons follow the layout rules");
is(pack.builtin, false, "an installed pack is not built in");

is(normalizeToolbarPack({ id: "Bad Id", name: "Bad" }), null, "a malformed pack id is refused");
is(normalizeToolbarPack({ id: "empty-name", name: "   " }), null, "a pack without a name is refused");
is(
  normalizeToolbarPack({ id: "no-buttons", name: "No Buttons", buttons: [] }).buttons,
  null,
  "a pack without buttons leaves the layout alone",
);

console.log(`toolbars: ${checks} checks pass`);
