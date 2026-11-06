// Self-check for What's New: the file parses into friendly releases, the
// version decision shows a newer build once and never a first install, the
// record survives a restart, and the real file carries notes for the version
// this package ships. Node strips the types, so this runs against the same
// module the app imports. Run it with `ppnpm check:whats-new`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const store = new Map();
globalThis.window = {
  localStorage: {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => store.set(key, String(value)),
    removeItem: (key) => store.delete(key),
  },
};

const {
  WHATS_NEW_STORAGE_KEY,
  clearSeenVersion,
  compareVersions,
  hasUsedTheAppBefore,
  latestRelease,
  loadSeenVersion,
  normalizeVersion,
  parseWhatsNew,
  releaseForVersion,
  saveSeenVersion,
  whatsNewDecision,
  whatsNewStartupDecision,
} = await import("../src/lib/whats-new.ts");

let checks = 0;

function is(actual, expected, label) {
  assert.deepEqual(actual, expected, label);
  checks += 1;
}

function ok(value, label) {
  assert.ok(value, label);
  checks += 1;
}

function reset() {
  store.clear();
}

/* --------------------------------------------------------------- the file */

const sample = `# What's new

<!-- A note for whoever edits this next. -->

## 0.2.0 — 31 October 2026

A friendlier window.

### Toolbar

- **Five looks.** Pick one.
- A \`custom\` style shared as a file.
  - This nested line is not a new item.

### Sharing

- Tell a friend.

## 0.1.3 - 30 October 2026

- Save folders.

## Unreleased

- Not ready for readers.
`;

const releases = parseWhatsNew(sample);
is(releases.map((release) => release.version), ["0.2.0", "0.1.3"], "newest first, unreleased left out");
is(releases[0].date, "31 October 2026", "the day is read from the heading");
is(releases[1].date, "30 October 2026", "a plain hyphen reads as the same heading");
is(releases[0].intro, ["A friendlier window."], "the opening line is kept as an introduction");
is(
  releases[0].sections,
  [
    { title: "Toolbar", items: ["Five looks. Pick one.", "A custom style shared as a file."] },
    { title: "Sharing", items: ["Tell a friend."] },
  ],
  "groups keep their lines, with markdown marks removed",
);
is(latestRelease(releases)?.version, "0.2.0", "the newest section is the latest");
is(latestRelease([]), null, "an empty file has no latest section");
is(releaseForVersion(releases, "v0.1.3")?.version, "0.1.3", "a stored v prefix still finds the section");
is(releaseForVersion(releases, "9.9.9"), null, "a version with no section is not invented");
is(releaseForVersion(releases, null), null, "no version has no section");

reset();
is(parseWhatsNew("# What's new\n\nNothing yet."), [], "a file with no release stays empty");
is(
  parseWhatsNew("## 0.3.0\r\n\r\n### New\r\n\r\n- Windows line endings work.\r\n")[0].sections[0].items,
  ["Windows line endings work."],
  "Windows line endings parse",
);
is(
  parseWhatsNew("## 0.3.0\n\n### Empty\n\n## 0.2.0\n\n- A real line.")[0].version,
  "0.2.0",
  "a group with no lines does not become a release",
);

/* ------------------------------------------------------------ the versions */

is(normalizeVersion("v0.2.0") , "0.2.0", "a leading v is not part of the number");
is(normalizeVersion(" 1.2.3+build.4 "), "1.2.3+build.4", "build metadata is kept for the file");
is(normalizeVersion("one.two.three"), null, "words are not a version");
is(normalizeVersion(""), null, "an empty number is no number");

ok(compareVersions("0.2.0", "0.10.0") < 0, "0.10 comes after 0.2");
ok(compareVersions("1.0.0", "1.0.0") === 0, "the same version is equal");
ok(compareVersions("1.0.0", "1.0.0-rc.1") > 0, "a release comes after its release candidate");
ok(compareVersions("1.0.0-rc.2", "1.0.0-rc.10") < 0, "prerelease numbers are numbers");
ok(compareVersions("1.0.0+one", "1.0.0+two") === 0, "build metadata does not order releases");
ok(compareVersions("junk", "1.0.0") === 0, "an unreadable version compares as equal");

/* ------------------------------------------------------------- the decision */

is(whatsNewDecision("0.2.0", null), "baseline", "a first install is only a starting point");
is(whatsNewDecision("0.2.0", undefined), "baseline", "a missing record is a first install");
is(whatsNewDecision(null, "0.1.0"), "baseline", "a build with no version makes no offer");
is(whatsNewDecision("0.2.0", "0.2.0"), "none", "the same version never opens again");
is(whatsNewDecision("0.3.0", "0.2.0"), "show", "a newer version opens its notes");
is(whatsNewDecision("1.0.0", "1.0.0-rc.1"), "show", "leaving a release candidate is an update");
is(whatsNewDecision("0.2.0", "0.3.0"), "baseline", "a downgrade is recorded quietly");
is(whatsNewDecision("not a version", "0.2.0"), "baseline", "junk never opens a window");

/* ------------------------------------------- an install from before notes */

is(
  whatsNewStartupDecision("0.2.0", null, false),
  "baseline",
  "a fresh profile only records the starting point",
);
is(
  whatsNewStartupDecision("0.2.0", null, true),
  "show",
  "an existing install sees the first release that carries notes",
);
is(
  whatsNewStartupDecision("0.2.0", "0.2.0", true),
  "none",
  "an existing install does not see the same version twice",
);
is(
  whatsNewStartupDecision("0.3.0", "0.2.0", true),
  "show",
  "an existing install still sees a later version",
);
is(
  whatsNewStartupDecision("0.2.0", "0.3.0", true),
  "baseline",
  "a downgrade stays quiet even with history",
);
is(
  whatsNewStartupDecision(null, null, true),
  "baseline",
  "history cannot invent a version",
);
is(
  whatsNewStartupDecision("0.2.0", "junk", true),
  "show",
  "a damaged record on an existing install is treated as the first notes run",
);

reset();
is(hasUsedTheAppBefore(), false, "a fresh profile has no saved history");
for (const key of [
  "idm-clone.settings.v1",
  "idm-clone.categories.v1",
  "idm-clone.grabber.v1",
  "idm-clone.last-folder.v1",
  "idm-clone.tips.v1",
  "idm-clone.toolbar-size.v1",
  "idm-clone.toolbar.v1",
]) {
  reset();
  store.set(key, "kept");
  is(hasUsedTheAppBefore(), true, `${key} means the application ran before`);
}
reset();
store.set(WHATS_NEW_STORAGE_KEY, JSON.stringify({ version: "0.2.0" }));
is(
  hasUsedTheAppBefore(),
  false,
  "the notes record alone is not evidence of an earlier run",
);

/* --------------------------------------------------------------- the record */

reset();
is(loadSeenVersion(), null, "a fresh install remembers no version");
saveSeenVersion("0.2.0");
is(loadSeenVersion(), "0.2.0", "the record survives a restart");
is(
  whatsNewDecision("0.2.0", loadSeenVersion()),
  "none",
  "the next launch of the same version stays quiet",
);
saveSeenVersion("v0.3.0");
is(loadSeenVersion(), "0.3.0", "a saved v prefix is stored in the file's form");
saveSeenVersion("nonsense");
is(loadSeenVersion(), "0.3.0", "an invalid version is not written over a good one");
store.set(WHATS_NEW_STORAGE_KEY, "{ not json");
is(loadSeenVersion(), null, "a damaged record reads as no record");
store.set(WHATS_NEW_STORAGE_KEY, JSON.stringify({ version: 42 }));
is(loadSeenVersion(), null, "a record without a version reads as no record");
clearSeenVersion();
is(loadSeenVersion(), null, "the record can be cleared");

/* --------------------------------------------------------- the real content */

const file = readFileSync(new URL("../WHATS_NEW.md", import.meta.url), "utf8");
const packageJson = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
);
const shipped = parseWhatsNew(file);
ok(shipped.length >= 3, "the file carries more than one release");
is(
  shipped.map((release) => release.version),
  [...shipped.map((release) => release.version)].sort((a, b) => compareVersions(b, a)),
  "the real file lists versions newest first",
);

const seenVersions = new Set();
const seenLines = new Set();
const developerWords =
  /\b(commit|changelog|refactor|regression|api|ipc|tauri|rust|serde|cargo|json|backend|frontend|repository|module)\b/i;

for (const release of shipped) {
  ok(normalizeVersion(release.version) === release.version, `${release.version} is a plain version`);
  ok(!seenVersions.has(release.version), `${release.version} has one section`);
  seenVersions.add(release.version);
  ok(release.date, `${release.version} says when it shipped`);
  ok(release.sections.length > 0, `${release.version} has at least one group`);

  let count = 0;
  for (const section of release.sections) {
    ok(
      section.title === null || section.title.length <= 48,
      `${release.version} keeps its group names short`,
    );
    for (const item of section.items) {
      count += 1;
      ok(item.length <= 180, `${release.version} keeps each line to two sentences`);
      ok(!seenLines.has(item.toLowerCase()), `${release.version} repeats no line`);
      seenLines.add(item.toLowerCase());
      ok(!developerWords.test(item), `${release.version} uses no developer words: ${item}`);
      ok(!/[*_`<>\[\]|]/.test(item), `${release.version} carries no markdown marks: ${item}`);
      ok(/^[A-Z0-9]/.test(item), `${release.version} starts its lines like a sentence: ${item}`);
    }
  }
  ok(count >= 1, `${release.version} says something`);
  ok(count <= 8, `${release.version} stays compact`);
}

const current = releaseForVersion(shipped, packageJson.version);
ok(
  current,
  `WHATS_NEW.md has a section for the version this package ships (${packageJson.version})`,
);
ok(
  current && current.sections.some((section) => section.items.length > 0),
  `the ${packageJson.version} section is readable`,
);

console.log(
  `whats-new: ${checks} checks passed (${shipped.length} releases, current ${packageJson.version})`,
);
