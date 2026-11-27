# Browser extensions

One integration, written once. `source/` holds every script, page and asset;
a browser folder holds only what that browser needs on top of it, and `build.mjs`
puts the two together. Nothing is duplicated between the builds, so a fix in the
overlay reaches Chrome, Edge, Brave, Opera, Vivaldi, Firefox and Safari at once.

```
extensions/
  source/            the extension itself: background, content scripts, popup,
                     link window, settings page, shared libs, theme, icons
  firefox/           manifest.json (MV3, event page) and its README
  chromium/          manifest.json (MV3, service worker), worker.js, README
  safari/            manifest.json (MV3, background document), README
  build.mjs          assembles dist/<browser>/ from source/ + <browser>/
  dist/              generated; load a browser's folder from here (git-ignored)
```

`source/` is browser-agnostic and talks to one API, `browser`. Firefox and
Safari already have it. Chromium exposes `chrome` instead, so `source/lib/platform.js`
adapts the callback API to promises and wraps the message listener; the Chromium
build loads it first, and it is a no-op everywhere else. That single file is why
the rest of this folder has no `if (chrome)` branches in it.

## Build

```powershell
pnpm build:extensions            # all three, into extensions/dist/
node extensions/build.mjs firefox        # one of them
node extensions/build.mjs --clean        # remove the generated folders
```

A build always starts from an empty output folder, so a file deleted from
`source/` cannot survive in a folder a browser is loading.

## Install

### Chrome, Edge, Brave, Opera, Vivaldi, Arc

```powershell
node extensions/build.mjs chromium
```

Open `chrome://extensions` (or `edge://extensions`, `brave://extensions`,
`opera://extensions`), turn on **Developer mode**, choose **Load unpacked**, and
pick `extensions/dist/chromium`.

### Firefox

```powershell
node extensions/build.mjs firefox
```

Open `about:debugging#/runtime/this-firefox`, choose **Load Temporary Add-on…**,
and pick `extensions/dist/firefox/manifest.json`. Temporary add-ons are removed
when Firefox closes.

For a signed build on a release Firefox, pack and sign it:

```powershell
pnpm pack:firefox                            # a ZIP made from the build
npx web-ext sign --source-dir extensions/dist/firefox --channel unlisted
```

`npx web-ext build --source-dir extensions/dist/firefox` makes the same archive
without signing; a release Firefox only installs signed add-ons.

### Safari

```powershell
node extensions/build.mjs safari
```

Safari extensions are wrapped in an app, and the wrapper is produced by Apple's
tooling on macOS:

```bash
xcrun safari-web-extension-converter extensions/dist/safari
```

Then run the generated app once, and enable the extension in Safari's
**Settings → Extensions**. Safari 16.4 or newer is required (the manifest says
so). The folder is a complete web extension; what it cannot do on Windows is
become an installable Safari app.

## What it does

- **Right-click a link, video, audio element or image** → *Download with B
  Download Manager*. An explicit click is never filtered by the site list.
- **Takes over browser downloads** when the setting is on. The download is
  frozen first, handed to the application, and only then stopped in the browser;
  if the app is closed or refuses it, the browser copy is resumed untouched.
- **Download button over videos**, in a closed shadow root, following the video
  through resize, theatre mode, fullscreen and in-page navigation. It lists the
  qualities the application parsed, and carries the download's own progress.
- **Download all links**: a compact window with the page's files, filters,
  selection, the app's folders and queue, and one answer per link.
- **Toolbar popup**: connection state, active downloads with progress, speed
  and pause/resume, the media on the current page, and a settings button.
- **Settings**: interception, ask-before-saving, the video overlay, file types,
  excluded sites, and Auto/Light/Dark theme. While the app runs, its Settings
  page owns interception, ask, types and sites; the extension's copy is the
  fallback for when it is closed.

## How it talks to the application

The app runs a small JSON API on `127.0.0.1:27500` and starts it with itself.
There is no native messaging host to register and nothing to paste: every
request carries `X-BDM-Client: browser-extension`, the server accepts only
well-formed extension origins, and it refuses a `Host` that is not the bridge's
own address.

`GET /v1/ping` is the handshake: it names the app and the API version, so a
different program on the port is reported as a mismatch rather than a connection.

| Request | Answer |
| --- | --- |
| `GET /v1/ping` | `{ app, api, version, port, connected }` |
| `GET /v1/capture` · `POST /v1/capture` | `{ capture, ask, types, skipSites }` |
| `GET /v1/video-button-settings` | `{ style, label, hover }` |
| `GET /v1/downloads` | `{ downloads, active }` |
| `POST /v1/downloads` | `{ mode: "direct", id, name, dest }` or `{ mode: "ask" }` |
| `GET /v1/batch-options` | `{ folders, queues, connections, maxItems }` |
| `POST /v1/downloads/batch` | one answer per link, and the counts |
| `POST /v1/downloads/:id/pause` · `resume` · `cancel` · `show` | the new state |
| `POST /v1/media` · `GET /v1/media/:id` · `POST /v1/media/:id/download` | a media job |
| `POST /v1/focus` | brings the main window forward |

`POST /v1/downloads` takes `{ url, name, referer, mode }`; a link must be
`http://`, `https://` or `ftp://`. The server sanitises the name and makes it
unique in the folder, so a hand-over cannot escape the folder or overwrite a
file. `POST /v1/downloads/batch` takes up to 25 links, a folder the app offered,
and answers each link on its own as `started`, `queued`, `skipped`, `duplicate`,
`invalid` or `refused`.

## Permissions

| Permission | Why |
| --- | --- |
| `contextMenus` | the right-click entries |
| `downloads` | see a browser download, freeze it, cancel it after a hand-over |
| `storage` | the settings and the theme |
| `cookies` | the site's own cookies for a stream that needs them, behind one consent question |
| `alarms` | one periodic read of the app's video-button choice |
| `tabs` | Chromium only: the popup reads the current tab |
| `http://127.0.0.1:27500/*` | the loopback bridge |
| `http://*/*`, `https://*/*` | the content script that finds videos and links |

Nothing leaves the machine: the only network destination is the loopback
bridge, and the application downloads the file itself.

## Checks

```powershell
pnpm check:extension
npx web-ext lint --source-dir extensions/dist/firefox --self-hosted
```

`check:extension` builds every browser folder first, then parses every script,
resolves every file a manifest or a page names, checks the shared files are
byte-identical in all three builds, runs the settings, media, link and page
helpers in a sandbox, exercises the background against a mocked bridge that
refuses a request without the extension header, drives the overlay and the link
window in a fake page, and runs `lib/platform.js` against a callback-style
`chrome` (including `lastError` and the promise-shaped message listener).

## Limitations

- **The app must be running.** A web extension cannot start a desktop program.
- **HLS and DASH are offered; live streams are not.** A `dynamic` MPD or a
  playlist with no `#ENDLIST` is refused, because there is no recording path.
- **DRM/EME media is not offered.** The overlay checks `mediaKeys` and never
  pretends a protected stream is available.
- **Safari is packaged, not published.** Distribution goes through Apple's
  converter and an app wrapper on macOS.
- **Chromium builds need Chrome 110 or newer** for the badge text colour; older
  engines keep their own badge colour and everything else works.
- **The overlay and the link window run in the top frame only.** Media inside a
  cross-origin iframe is not read; the download takeover still sees files the
  browser downloads.
