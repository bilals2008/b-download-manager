# Chromium build

Chrome, Edge, Brave, Opera, Vivaldi and Arc run the same extension: one
Manifest V3 package, one engine. This folder adds only what Chromium needs to
the shared source in `../source/`:

- `manifest.json` — MV3 with a service worker, the `tabs` permission the popup
  needs, and the content scripts in the order the overlay expects.
- `worker.js` — the service-worker entry. A service worker runs one file, so it
  imports the shared background list with `importScripts`, adapter first.

`../source/lib/platform.js` is loaded first in the worker and in every content
script and page: it turns `chrome`'s callback API into the promise-shaped
`browser` the shared scripts are written against, and adapts the message
listener so one async handler answers through `sendResponse` with `true`.

## Build and load

```powershell
node extensions/build.mjs chromium
```

Then `chrome://extensions` (or `edge://extensions`, `brave://extensions`,
`opera://extensions`), **Developer mode**, **Load unpacked**,
`extensions/dist/chromium`.

After changing anything in `../source/`, rebuild and press **Reload** on the
extension card. A service worker is restarted when it is reloaded.

## What differs from Firefox

- The background is a service worker, so it is unloaded when idle. Listeners are
  registered at the top level, which is what lets the browser wake it again.
- `minimum_chrome_version` is 110: that is where `action.setBadgeTextColor`
  exists. An older engine keeps its own badge colour; nothing else changes.
- The video overlay, link window and settings page are the same files as the
  Firefox build, not a reduced copy.
