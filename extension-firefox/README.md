# Firefox extension

The Firefox half of B Download Manager. It hands links and media to the
desktop application and shows the application's live downloads in the toolbar
popup.

This build is Manifest V3 with an **event page** (`background.scripts`), which
is what Firefox supports; it is not a Chrome service worker. The Chrome build
lives in `browser-extension/` and speaks the same versioned loopback API.

## What it does

- **Right-click a link, video, audio element or image** →
  *Download with B Download Manager*. A menu click always goes to the
  application; the site exclusions do not apply to an explicit click.
- **Takes over browser downloads** when the setting is on. A download the
  browser starts is handed to the application **before** the browser copy is
  cancelled, so a closed application never loses the file. File-type and site
  rules are applied here.
- **IDM-style video button** on supported HTML5 videos. A 30 px button appears
  at the video's top-right corner on hover. Clicking it opens a compact panel
  with the direct media files that video offers, a progress bar and an
  *Open app* action. It is built in a closed shadow root with a constructed
  stylesheet, so page CSS and CSP cannot change it.
- **Toolbar badge** shows how many download-capable media elements are on the
  tab. It is cleared when the tab navigates or the overlay is turned off.
- **Compact popup**: connection state, active downloads (progress, size,
  speed, pause/resume, two-click stop, open in app), media on the current
  page, and one settings button.
- **Settings**: interception, ask-before-saving, video overlay, file-type
  filter, excluded sites, and Auto/Light/Dark theme. Every change is saved
  immediately; *Reset to defaults* is the only destructive action and asks for
  a second click.

The panel deliberately offers only addresses the engine can fetch as a file.
A `blob:` URL from Media Source Extensions, an HLS/DASH manifest, and a
DRM-protected element are not offered, because they are not downloadable as a
plain file by this application. The extension does not pretend otherwise.

## How it talks to the application

The desktop app runs a small HTTP API on `127.0.0.1:27500`. This is the local
bridge: no native messaging host, no registry entry, no extra executable to
install, and no port exposed beyond loopback. The app starts the server with
itself.

Every request carries `X-BDM-Client: browser-extension`. A web page cannot add
that header without a preflight, and the server refuses a preflight whose
`Origin` is not `moz-extension://` or `chrome-extension://`. A plain `<img>`,
form or `<script>` request is therefore refused as well. Requests without an
`Origin` (a local test, for example) are still required to carry the header.

| Request | Answer |
| --- | --- |
| `GET /v1/ping` | `{ app, api, version, port }` |
| `GET /v1/downloads` | `{ downloads, active }` |
| `POST /v1/downloads` | `{ mode: "direct", id, name, dest }` or `{ mode: "ask" }` |
| `POST /v1/downloads/:id/pause` | the download's new state |
| `POST /v1/downloads/:id/resume` | the download's new state |
| `POST /v1/downloads/:id/cancel` | the download's new state |
| `POST /v1/downloads/:id/show` | opens the progress window |
| `POST /v1/focus` | brings the main window forward |

`POST /v1/downloads` takes `{ url, name, referer, mode }`; `url` must be
`http://`, `https://` or `ftp://`, and `mode` is `direct` or `ask`. The server
sanitises the file name and makes it unique in the download folder, so a
hand-over cannot escape the folder or overwrite an existing file.

The extension pings first. If nothing answers, the app is reported as not
running. If something answers but does not identify itself as
`b-download-manager` with API version `1`, the popup says the versions or the
program do not match instead of guessing.

### Why not native messaging

A native messaging host needs a JSON manifest in a fixed Firefox location, a
registry key on Windows, and an executable whose lifetime and stdout framing
are managed separately from the app. The loopback server is already inside the
application, is reachable only from this machine, and fails in a way the popup
can explain ("not running", "not responding", "version mismatch"). The
extension therefore needs no `nativeMessaging` permission and nothing has to be
registered per user or per Firefox profile.

## Install on Windows

### 1. Start the application

```powershell
cd D:\Dev\my-idm-clone
npm install
npm run tauri dev
```

Or start the built `B Download Manager` executable. The application opens the
bridge when it starts. Closing the window hides it in the tray; **Exit** in the
tray menu stops it and the extension reports "Not running".

To confirm the bridge is listening:

```powershell
Get-NetTCPConnection -LocalPort 27500 -State Listen
```

### 2. Load the extension in Firefox (development)

1. Open `about:debugging#/runtime/this-firefox`.
2. Choose **Load Temporary Add-on…**.
3. Pick `extension-firefox/manifest.json`.

Temporary add-ons are removed when Firefox closes, so repeat this after a
restart. On a normal Firefox release, this is the only way to run an unsigned
build.

### 3. Install permanently (optional)

Firefox only installs signed add-ons on release builds. There are two honest
routes:

- **Sign it yourself through AMO**:
  ```powershell
  npx web-ext sign --source-dir extension-firefox --channel unlisted
  ```
  This needs AMO API credentials. Install the resulting `.xpi` from
  `about:addons` → gear → *Install Add-on From File*.
- **Self-distributed build**: use Firefox Developer Edition or Nightly, set
  `xpinstall.signatures.required` to `false` in `about:config`, then install
  the `.xpi`. This is a developer setting, not a distribution path for normal
  users.

To produce the `.xpi` without signing:

```powershell
npx web-ext build --source-dir extension-firefox --overwrite-dest
```

### 4. Use it

- Click a link's right-click menu → *Download with B Download Manager*, or
  start a download normally and let the takeover send it to the app.
- Hover a supported video and click the small download button.
- Click the toolbar icon for connection state, active downloads and the media
  on the current page.
- Open the extension settings from the popup or the right-click menu.

## Permissions and privacy

| Permission | Why it is needed |
| --- | --- |
| `contextMenus` | the right-click entries |
| `downloads` | see a browser download and cancel it after a successful hand-over |
| `storage` | settings and the theme |
| `http://127.0.0.1:27500/*` | the loopback bridge |

The content script is declared for `http://*/*` and `https://*/*`, so Firefox
shows **Access your data for all websites** at install. That broad grant is
what lets the video button appear without a click. The script reads media
addresses and the page URL as a referrer; it does not read page text, form
values, cookies or storage, and it never sends anything to a remote server.
Nothing leaves the machine: the only network destination is the loopback
bridge, and the application downloads the file itself.

The overlay is disabled on pages listed under **Sites left alone**, along with
download interception. An explicit right-click or popup download still works
there.

## Checks

```powershell
npm run check:extension
npx web-ext lint --source-dir extension-firefox --self-hosted
```

`check:extension` parses every script, resolves every file a page loads,
checks the shared colour tokens, runs the settings and media helpers in a
sandbox, exercises the background against a mocked bridge, verifies that every
icon name exists, and runs the Chrome build's hand-over contract too.
`web-ext lint` is Mozilla's own manifest and API validation.

## Files

```
manifest.json          MV3 event page, Gecko id, options page
background.js          menus, takeover, badge, message routing
lib/bridge.js          the versioned loopback contract
lib/settings.js        defaults, normalisation, the takeover and overlay rules
lib/media.js           what counts as a downloadable media address
lib/icons.js           the SVG icons drawn by the popup and the overlay
content/page-media.js  reads media addresses out of the page
content/overlay.js     the floating button, panel and live progress
content/overlay-style.js  the shadow-root stylesheet and its token copy
popup/                 connection, downloads, page media, settings entry
options/               the settings page
theme.css              the app's palette, shared by both extension pages
fonts/                 Inter, the app's interface face
icons/                 the app mark at 32, 48, 96 and 128 px
```

## Known limitations

These are honest boundaries, not unfinished UI:

- **HLS and DASH are not offered.** The engine has no muxer yet
  (`PROGRESS.md`: *Media splitting for MP4 DASH, HLS and MPEG-TS*). A `.m3u8`
  would be saved as a playlist, not a video.
- **DRM/EME and `blob:` media are not offered.** There is no file address the
  application may read. The overlay checks `mediaKeys` and never pretends a
  protected stream is available.
- **A type filter matches the URL path.** A URL with no extension but a
  `Content-Disposition` file name is filtered out when the type list is
  non-empty; leave the list empty to take every type.
- **Batch hand-over is not implemented.** The app's bridge starts one
  download per request and its add dialog holds one request.
- **The overlay runs in the top frame only.** Media inside a cross-origin
  iframe is not listed by the page panel; the browser's own download takeover
  still sees files the browser downloads.
- **The app must be running.** The extension cannot launch a desktop program
  from a web extension sandbox; it reports the closed state instead.
- **The app must be restarted after an update** that changes the bridge API.
  The version check makes a mismatch visible rather than silently failing.
