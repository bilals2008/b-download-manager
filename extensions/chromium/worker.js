/*
  The Chromium entry.

  Firefox and Safari list the shared scripts in the manifest, because both run
  a background document that can hold several classic scripts. A service worker
  runs one file, so this file imports the same list, in the same order, and
  then the shared background itself.

  `lib/platform.js` goes first: it is what gives this engine a promise-shaped
  `browser` for the rest of the list to use.
*/

importScripts(
  "lib/platform.js",
  "lib/media.js",
  "lib/link-rules.js",
  "lib/settings.js",
  "lib/bridge.js",
  "lib/manifest.js",
  "background.js",
);
