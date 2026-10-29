// Self-check for the tips: the collection is unique and readable, every entry
// only names something the app can do, and the saved position wraps cleanly.
// Node strips the types, so this runs against the same modules the dialog
// imports. Run it with `pnpm run check:tips`.

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
  TIPS,
  loadTipCursor,
  nextTipIndex,
  resetTipCursor,
  saveTipCursor,
  stepTipIndex,
} = await import("../src/lib/tips.ts");
const { DEFAULT_SETTINGS, loadSettings, saveSettings } = await import(
  "../src/lib/settings.ts"
);

let checks = 0;

function is(actual, expected, label) {
  assert.deepEqual(actual, expected, label);
  checks += 1;
}

function reset() {
  store.clear();
}

/* ----------------------------------------------------------- the collection */

assert.ok(TIPS.length >= 12, "there are enough tips to be worth a carousel");

const ids = new Set();
const titles = new Set();
const bodies = new Set();

// Features that are drawn but not built. A tip that mentions one of these is
// teaching a control that cannot act.
const UNBUILT =
  /batch|export|import|scheduler|drop target|coming soon|tell a friend/i;

for (const tip of TIPS) {
  assert.equal(typeof tip.id, "string", `${tip.id} has a name`);
  assert.ok(tip.id.length > 0, "an id is not empty");
  assert.ok(!ids.has(tip.id), `the id ${tip.id} is used once`);
  ids.add(tip.id);

  assert.ok(tip.category.trim().length > 0, `${tip.id} names a category`);
  assert.ok(tip.title.trim().length > 0, `${tip.id} has a title`);
  assert.ok(tip.title.length <= 60, `${tip.id} keeps its title short`);
  assert.ok(tip.body.trim().length > 0, `${tip.id} has a body`);
  assert.ok(
    tip.body.length <= 200,
    `${tip.id} keeps its body to two short sentences`,
  );
  assert.ok(
    !UNBUILT.test(`${tip.title} ${tip.body}`),
    `${tip.id} does not describe an unbuilt feature`,
  );
  if (tip.shortcut !== undefined) {
    assert.match(
      tip.shortcut,
      /^(Ctrl|Shift|Alt|F\d)/,
      `${tip.id} prints a real accelerator`,
    );
  }

  const title = tip.title.toLowerCase();
  const body = tip.body.toLowerCase();
  assert.ok(!titles.has(title), `${tip.id} repeats no other title`);
  assert.ok(!bodies.has(body), `${tip.id} repeats no other body`);
  titles.add(title);
  bodies.add(body);
}

/* -------------------------------------------------------------- the position */

reset();
is(loadTipCursor(), -1, "a fresh install has seen no tip");
is(nextTipIndex(), 0, "the first start shows the first tip");
is(loadTipCursor(), 0, "the position is remembered before the tip is shown");
is(nextTipIndex(), 1, "the next start moves on");
is(nextTipIndex(), 2, "each start moves on");

saveTipCursor(TIPS.length - 1);
is(nextTipIndex(), 0, "the run wraps back to the first tip");

is(stepTipIndex(0, -1), TIPS.length - 1, "Previous wraps at the start");
is(stepTipIndex(TIPS.length - 1, 1), 0, "Next wraps at the end");
is(stepTipIndex(3, 2), 5, "a step moves through the collection");
is(stepTipIndex(99, 0), 0, "a position from a shorter list lands on a tip");
is(stepTipIndex(-4, 1), 1, "a bad position is treated as the first tip");

store.set(
  "idm-clone.tips.v1",
  JSON.stringify({ index: TIPS.length + 4 }),
);
is(loadTipCursor(), -1, "an out-of-range saved position is ignored");
store.set("idm-clone.tips.v1", "not json");
is(loadTipCursor(), -1, "a broken saved position is ignored");

saveTipCursor(3);
is(loadTipCursor(), 3, "a position is remembered");
resetTipCursor();
is(loadTipCursor(), -1, "the reset forgets the position");
is(nextTipIndex(), 0, "the next tip after a reset is the first");

/* ------------------------------------------------------------- the setting */

reset();
is(DEFAULT_SETTINGS.tipsOnStartup, true, "startup tips are on by default");
is(loadSettings().tipsOnStartup, true, "an empty store keeps them on");

saveSettings({ ...DEFAULT_SETTINGS, tipsOnStartup: false });
is(loadSettings().tipsOnStartup, false, "turning them off survives a restart");

store.set(
  "idm-clone.settings.v1",
  JSON.stringify({ ...DEFAULT_SETTINGS, tipsOnStartup: "yes" }),
);
is(loadSettings().tipsOnStartup, true, "junk falls back to the default");

console.log(`tips: ${checks} checks passed (${TIPS.length} tips)`);
