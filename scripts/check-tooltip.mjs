// Self-check for the tooltip placement: the preferred side, the flip when it
// does not fit, and the clamp that keeps the body and its caret inside the
// window. Node strips the types, so the check runs against the same helper the
// component imports. Run it with `npm run check:tooltip`.

import assert from "node:assert/strict";

import { placeTooltip } from "../src/lib/tooltip-position.ts";

const view = { width: 1000, height: 800 };
const tip = { left: 0, top: 0, width: 120, height: 28 };

assert.deepEqual(
  placeTooltip("top", { left: 400, top: 100, width: 100, height: 40 }, tip, view),
  { side: "top", x: 390, y: 62, arrow: 60 },
  "centred above the trigger when there is room",
);

assert.deepEqual(
  placeTooltip("top", { left: 400, top: 10, width: 100, height: 40 }, tip, view),
  { side: "bottom", x: 390, y: 60, arrow: 60 },
  "flips under the trigger when the top is too tight",
);

assert.deepEqual(
  placeTooltip("top", { left: 0, top: 100, width: 40, height: 40 }, tip, view),
  { side: "top", x: 8, y: 62, arrow: 16 },
  "clamps against the left edge and holds the caret inside",
);

assert.deepEqual(
  placeTooltip("top", { left: 960, top: 100, width: 40, height: 40 }, tip, view),
  { side: "top", x: 872, y: 62, arrow: 104 },
  "clamps against the right edge and holds the caret inside",
);

assert.deepEqual(
  placeTooltip("left", { left: 500, top: 300, width: 40, height: 40 }, tip, view),
  { side: "left", x: 370, y: 306, arrow: 14 },
  "sits beside the trigger when there is room",
);

assert.deepEqual(
  placeTooltip("left", { left: 10, top: 300, width: 40, height: 40 }, tip, view),
  { side: "right", x: 60, y: 306, arrow: 14 },
  "flips to the other side at the window edge",
);

assert.deepEqual(
  placeTooltip(
    "top",
    { left: 40, top: 40, width: 20, height: 20 },
    { left: 0, top: 0, width: 120, height: 80 },
    { width: 1000, height: 100 },
  ),
  { side: "top", x: 8, y: 8, arrow: 42 },
  "stays inside a window too short for either side",
);

console.log("tooltip placement: 7 checks pass");
