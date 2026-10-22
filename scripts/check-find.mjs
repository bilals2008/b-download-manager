// Self-check for Find Next: the walk, the wrap and the first press. Node
// strips the types, so the check runs against the same file the app imports.
// Run it with `npm run check:find`.

import assert from "node:assert/strict";

import { nextMatch } from "../src/lib/find.ts";

// Ids are deliberately out of order: the list order, not the id order, rules.
const ids = [7, 3, 9];

assert.equal(nextMatch(ids, null, 1), 7, "first press lands on the first match");
assert.equal(nextMatch(ids, null, -1), 9, "backwards starts at the last match");
assert.equal(nextMatch(ids, 7, 1), 3, "walks down the list");
assert.equal(nextMatch(ids, 3, -1), 7, "walks up the list");
assert.equal(nextMatch(ids, 9, 1), 7, "wraps at the end");
assert.equal(nextMatch(ids, 7, -1), 9, "wraps at the start");
assert.equal(nextMatch(ids, 42, 1), 7, "the match was filtered out");
assert.equal(nextMatch([], null, 1), null, "no match at all");

console.log("find next: 8 checks pass");
