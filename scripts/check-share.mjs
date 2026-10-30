// Self-check for Tell a friend: the Windows share window, its refusal, and the
// clipboard left underneath it. Node strips the types, so the check runs
// against the same file the app imports. Run it with `pnpm run check:share`.

import assert from "node:assert/strict";

import { APP_HOME_PAGE } from "../src/lib/brand.ts";
import { shareApp } from "../src/lib/share.ts";

function withNavigator(share) {
  Object.defineProperty(globalThis, "navigator", {
    value: share ? { share } : {},
    configurable: true,
    writable: true,
  });
}

// The clipboard trick needs a document; only what copyText touches is there.
let copySucceeds = true;
globalThis.document = {
  createElement: () => ({
    setAttribute() {},
    style: {},
    select() {},
    remove() {},
  }),
  body: { appendChild() {} },
  execCommand: () => copySucceeds,
};

withNavigator(undefined);

// The desktop build hands the link to the share window of Windows.
assert.equal(
  await shareApp(async () => true),
  "shared",
  "the system window is the first answer",
);

// A machine with no sheet to show still has the clipboard.
copySucceeds = true;
assert.equal(
  await shareApp(async () => false),
  "copied",
  "a refused system window copies instead",
);

// The browser preview answers with the Web Share API.
withNavigator(async () => {});
assert.equal(await shareApp(), "shared", "the Web Share API answers for itself");

withNavigator(async () => {
  throw new DOMException("cancelled", "AbortError");
});
assert.equal(await shareApp(), "dismissed", "closing the sheet is not a copy");

withNavigator(undefined);
copySucceeds = false;
assert.equal(await shareApp(), "failed", "a refused clipboard is reported");

// The message a reader pastes has to lead somewhere.
assert.match(APP_HOME_PAGE, /^https:\/\/b-download-manager\.vercel\.app\/$/);

console.log("tell a friend: 6 checks pass");
