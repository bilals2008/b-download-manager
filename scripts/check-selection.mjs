// Self-check for the Shift range rule, in the order the list shows. Node
// strips the types, so the check runs against the same file the app imports.
// Run it with `npm run check:selection`.

import assert from "node:assert/strict";

import { rangeIds } from "../src/lib/selection.ts";

// Ids are deliberately out of order: the list order, not the id order, rules.
const ids = [7, 3, 9, 1];

assert.deepEqual(rangeIds(ids, 3, 1), [3, 9, 1], "down the list");
assert.deepEqual(rangeIds(ids, 1, 7), [7, 3, 9, 1], "up the list");
assert.deepEqual(rangeIds(ids, 9, 9), [9], "one row");
assert.deepEqual(rangeIds(ids, null, 9), [9], "no anchor yet");
assert.deepEqual(rangeIds(ids, 42, 9), [9], "anchor filtered out");

console.log("selection range: 5 checks pass");
