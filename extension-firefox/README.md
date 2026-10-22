# Firefox extension

The Firefox half of the browser bridge. It hands a link to the desktop
application, which opens its add dialog the way it does for a link typed in by
hand.

Kept apart from `browser-extension/`, which is the Chrome build: Firefox does
not load a Manifest V3 service worker, so the background here is an event page
(`background.scripts`) with its own `browser_specific_settings`.

## What it does

- **Right click a link, video or audio element** → *Download with Internet
  Download Manager*. A menu click always goes to the application; the skip
  rules do not apply to it.
- **Take over browser downloads**: a download the browser starts is handed to
  the application and then cancelled here. The hand-over happens **before** the
  cancel, so a download is never lost when the application is closed.
- **Download button on pages**, the IDM-style panel: a small button in the
  corner of a page that has media. It opens the same list the popup shows, one
  Download per file, and can be hidden for that page view with its close button.
  The toolbar button carries a badge with the number of media on the open page.
  The panel is a content script and can be turned off in the popup or the
  settings page; with it off the badge goes too.
- **Media on this page**: the popup reads the open tab and lists the media
  files it can see (video, audio, source, and links ending in a media
  extension). One click sends the link over with the page as the referrer.
  A blob: stream from MSE has no address outside the tab, so it is not listed.
- **Settings** (the button in the popup footer):
  - the takeover switch;
  - the page download panel;
  - file types to take over, empty for all;
  - sites to leave alone, subdomains follow their domain;
  - both lists take a pasted list, and a row moves by dragging it or by
    focusing its handle and pressing Up or Down;
  - **Save** writes, **Reset** restores the defaults (two clicks, Escape backs
    out), and a page open in another tab reloads itself when the settings
    change there.

## The contract it talks to

From `src-tauri/src/bridge.rs`:

| Request | Answer |
| --- | --- |
| `GET http://127.0.0.1:27500/ping` | `200 ok` |
| `GET /download?url=…&name=…&referer=…` | `200` once the add dialog is open |

`url` must be `http://`, `https://` or `ftp://`. Values are percent-encoded. The
server refuses a request whose `Origin` is not `moz-extension://` or
`chrome-extension://`, and the extension sends the `moz-extension://` origin by
itself.

## Load it

Temporary, for testing:

1. Start the application.
2. `about:debugging#/runtime/this-firefox` → *Load Temporary Add-on…*
3. Pick `extension-firefox/manifest.json`.

Temporary add-ons are dropped when Firefox closes.

Or with [web-ext](https://extensionworkshop.com/documentation/develop/getting-started-with-web-ext/):

```bash
npx web-ext run --source-dir extension-firefox
```

A permanent install needs the add-on signed, because Firefox only installs
signed extensions. `npx web-ext sign --source-dir extension-firefox` with AMO
credentials, or load it as a self-distributed build with
`xpinstall.signatures.required` set to `false` in `about:config` on Developer
Edition / Nightly.

## Checks

```bash
npm run check:extension
```

Runs the extension's own `settings.js` in a Node sandbox: the manifest points at
files that exist, every script and stylesheet a page loads exists, and the
takeover rules and the host cleaning do what the settings page promises.

## Files

```
manifest.json      MV3 event page, gecko id, options page
bridge.js          the loopback contract: ping, hand-over, names
settings.js        defaults, normalisation, the takeover rule
list-editor.js     the add, remove and reorder list the settings page uses
background.js      menu, traffic watching, the takeover, badge, panel messages
panel.js           content script: the corner download button and its list
popup.html/.css/.js    state, switches, media links on the open page
options.html/.css/.js  the settings page
theme.css          palette from src/index.css, dark base and light
icons/icon-*.png   the app mark at 32, 48, 96 and 128
```

Permissions, and why each one is there: `downloads` to see and cancel a browser
download, `contextMenus` for the right-click entry, `storage` for the settings,
`scripting` + `activeTab` to read the media links when the popup opens, and the
loopback host permission for the bridge. The page panel is a content script on
every `http` and `https` page, which Firefox lists as "Access your data for all
websites" at install; that broad grant is what lets the corner button appear
without a click. It reads media addresses and nothing else.

The panel is in a closed shadow root with a constructed stylesheet, so neither
the page's CSS nor its CSP can change or block it.

## Not built, and why

These two need the desktop application to change first; the extension cannot do
them honestly on its own.

- **Batch links** — the bridge takes one link and the add dialog holds one
  request (`browserRequest` in `src/state/download-store.tsx`). Sending several
  would overwrite the ones before it, silently. It needs a multi-link request in
  `bridge.rs` and a list in the add dialog.
- **HLS and DASH** — `Media splitting for MP4 DASH, HLS and MPEG-TS` is still
  open in `PROGRESS.md`. Listing a `.m3u8` would hand the application a playlist
  to download as a file, not a video, so the popup deliberately does not offer
  one.
