// Self-check for the update surface: the first-run setting round-trips and
// defaults like every other field, and the release date reads as a day or
// says nothing. Node strips the types, so this runs against the same modules
// the app imports. Run it with `pnpm run check:updates`.

import assert from "node:assert/strict";

const store = new Map();
globalThis.window = {
  localStorage: {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => store.set(key, String(value)),
    removeItem: (key) => store.delete(key),
  },
};

const { DEFAULT_SETTINGS, loadSettings, saveSettings } = await import(
  "../src/lib/settings.ts"
);
const { formatReleaseDate, releaseNoteLines } = await import(
  "../src/lib/updates.ts"
);

assert.equal(
  DEFAULT_SETTINGS.firstRunDone,
  false,
  "a fresh install has not seen the first-run sentence yet",
);

// A first load on a new machine shows the notice field as unseen.
assert.equal(loadSettings().firstRunDone, false);

// The add dialog's retirement of the flag survives a reload.
saveSettings({ ...loadSettings(), firstRunDone: true });
assert.equal(loadSettings().firstRunDone, true);

// Saved data from before the field exists falls back, field by field, like
// every other key: such a user has not seen the sentence either.
store.set("idm-clone.settings.v1", JSON.stringify({ theme: "dark", closeToTray: false }));
const migrated = loadSettings();
assert.equal(migrated.theme, "dark", "the older keys still arrive");
assert.equal(migrated.firstRunDone, false, "a missing key reads as unseen");

// Junk in the key does not throw and does not claim the notice was seen.
store.set("idm-clone.settings.v1", "{ not json");
assert.equal(loadSettings().firstRunDone, false);

// The release date: a real timestamp reads as a day; junk says nothing.
const day = formatReleaseDate("2026-10-27T12:00:00Z");
assert.match(day ?? "", /2026/, "a real date carries the year");
assert.equal(formatReleaseDate(null), null);
assert.equal(formatReleaseDate("not a date"), null);

// The notes box shows the words a release body means, not its markdown: the
// hashes, the dashes and the bold marks stay out of the reading.
assert.deepEqual(
  releaseNoteLines("## [0.1.2] - 29 October 2026\n\n### Added\n\n- **icons:** Draw file icons\n- A light page"),
  [
    { kind: "heading", text: "[0.1.2] - 29 October 2026" },
    { kind: "heading", text: "Added" },
    { kind: "bullet", text: "icons: Draw file icons" },
    { kind: "bullet", text: "A light page" },
  ],
);
assert.equal(
  releaseNoteLines("See [the notes](https://example.com) for `ERR_INVALID_ARG_TYPE`.")[0]
    .text,
  "See the notes for ERR_INVALID_ARG_TYPE.",
  "a link keeps its words and a code span keeps its underscores",
);
assert.equal(releaseNoteLines("---")[0], undefined, "a rule is not a note");

/* ------------------------------------------------------------ outcomes */

const { outcomeOf, formatCheckTime, describeFailure } = await import(
  "../src/lib/updates.ts"
);

// The three answers a check can give, whichever way the reply came back.
assert.deepEqual(
  outcomeOf({ available: true, currentVersion: "0.1.0", version: "0.2.0", date: null, notes: null, error: null }),
  { kind: "found", update: { available: true, currentVersion: "0.1.0", version: "0.2.0", date: null, notes: null, error: null } },
  "a newer release is the answer that offers something",
);
assert.deepEqual(
  outcomeOf({ available: false, currentVersion: "0.1.0", version: null, date: null, notes: null, error: null }),
  { kind: "latest", version: "0.1.0" },
  "no newer release names the version being run",
);
assert.deepEqual(
  outcomeOf({ available: false, currentVersion: "0.1.0", version: null, date: null, notes: null, error: "unreachable" }),
  { kind: "error", message: "unreachable" },
  "an error outranks the version fields",
);

assert.equal(describeFailure(new Error("boom")), "boom", "an Error keeps its sentence");
assert.equal(describeFailure("plain"), "plain", "a string is already a sentence");

// The one line the About page prints under the check.
assert.match(formatCheckTime(null), /Never/, "no check has run yet");
assert.match(formatCheckTime(new Date("2026-10-27T09:30:00").getTime()), /2026/, "a check is dated");

/* -------------------------------------------------------- new settings */

store.clear();
assert.equal(loadSettings().updateAutoCheck, false, "a check at startup is opt-in");
assert.equal(loadSettings().lastUpdateCheck, null, "nothing has been checked yet");
saveSettings({ ...DEFAULT_SETTINGS, updateAutoCheck: true, lastUpdateCheck: 1_700_000_000_000 });
assert.equal(loadSettings().updateAutoCheck, true, "the agreement survives");
assert.equal(loadSettings().lastUpdateCheck, 1_700_000_000_000, "so does the moment of the last check");
store.set("idm-clone.settings.v1", JSON.stringify({ ...DEFAULT_SETTINGS, lastUpdateCheck: -12 }));
assert.equal(loadSettings().lastUpdateCheck, null, "a stamp from before the epoch is no stamp");
store.set("idm-clone.settings.v1", JSON.stringify({ ...DEFAULT_SETTINGS, lastUpdateCheck: "yesterday" }));
assert.equal(loadSettings().lastUpdateCheck, null, "a stamp that is not a moment is no stamp");

console.log("check:updates ok");
