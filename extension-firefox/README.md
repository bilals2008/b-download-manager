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
  rules follow the app's Settings while it runs, and the extension's own copy
  while it is closed.
- **IDM-style video button** on supported HTML5 videos. A 30 px button appears
  at the video's top-right corner while the pointer is over the player — over
  the picture, or over the player's own gradient and controls, which sit above
  it. Clicking it opens a compact panel with the files or streams that video
  offers, each labelled with its quality, a progress bar and an *Open app*
  action. It is built in a closed shadow root with a constructed stylesheet, so
  page CSS and CSP cannot change it.

  The button follows the video rather than the event that announced it: a
  player that resizes, goes to the theatre layout or enters fullscreen moves it,
  and there is only ever one of them. A stream the player asks for after the
  pointer has arrived brings the button up, and a navigation inside the page
  (YouTube's watch page) drops the streams of the video that was left behind, so
  the panel never offers an expired address for the next one.

  After a stream is handed over, the button carries the download itself: a thin
  ring around it fills as the file arrives, the icon becomes a check when it is
  saved, and the ring turns red when it fails. The ring only appears on the
  video the download came from, and it stays while the panel is closed — which
  is the point of it.
- **Toolbar badge** shows how many downloadable media the tab offers. It is
  cleared when the tab navigates or the overlay is turned off.
- **Compact popup**: connection state, active downloads (progress, size,
  speed, pause/resume, two-click stop, open in app), media on the current
  page, and one settings button.
- **Settings**: interception, ask-before-saving, video overlay, file-type
  filter, excluded sites, and Auto/Light/Dark theme. Every change is saved
  immediately; *Reset to defaults* is the only destructive action and asks for
  a second click. While the app is running, its Settings page owns interception,
  ask, file types and excluded sites; the extension's copy is the fallback for
  when it is closed. The video overlay and the theme always stay
  extension-owned.

The panel offers only addresses the engine can fetch as a file. When a player
streams through Media Source Extensions it names only a `blob:` URL, so the
panel reads the requests that player made and offers the whole-file address of
each stream: a quality badge and the size it will take. A picture-only DASH
stream says `video only`, because the engine saves picture and sound as separate
files. An HLS/DASH manifest and a DRM-protected element are still not offered.

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
| `GET /v1/capture` | `{ capture, ask, types, skipSites }` |
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

`GET /v1/capture` is the app-owned copy of the capture rules. While the app
answers, its Settings page wins for the hand-over switch, the ask switch, the
file types and the excluded sites. When the app is closed the request fails and
the extension uses its own saved copy, so the rules still apply with the app
gone.

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
- Hover a supported video and click the small download button. The app's
  Add URL dialog opens with the stream, its quality name and the folder, and
  nothing starts until you confirm.
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
addresses, the page title and URL as a referrer, and the addresses of the media
requests the page made; it does not read page text, form values, cookies or
storage, and it never sends anything to a remote server.
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
checks the shared colour tokens, runs the settings, media and page-list helpers
in a sandbox, exercises the background against a mocked bridge, verifies that
every icon name exists, and runs the Chrome build's hand-over contract too.

It also runs the content scripts in a fake page (`scripts/lib/fake-dom.mjs`):
a YouTube-shaped player whose controls cover the picture, a stream that arrives
after the pointer, a video inserted later, a navigation inside the document,
fullscreen, a protected video, the switch turned off, a closed application and a
page that unloads. That is where the button's behaviour is checked, because a
static screenshot cannot show whether it appears at all. `web-ext lint` is
Mozilla's own manifest and API validation.

## Files

```
manifest.json          MV3 event page, Gecko id, options page
background.js          menus, takeover, badge, message routing
lib/bridge.js          the versioned loopback contract
lib/settings.js        defaults, normalisation, the takeover and overlay rules
lib/media.js           what counts as a downloadable media address
lib/icons.js           the SVG icons drawn by the popup and the overlay
content/page-media.js  reads media addresses out of the page
content/video-registry.js  which videos the page has, and which is under the pointer
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
- **DRM/EME media is not offered.** The overlay checks `mediaKeys` and never
  pretends a protected stream is available. An MSE player's `blob:` URL is
  replaced by the file requests it made; those addresses are signed and expire,
  so send one while the page is playing or it will fail later.
- **A page that navigates inside the document starts from an empty list.** The
  streams belong to the address that fetched them, so the ones from the video
  that was left behind are dropped. Until the player asks for its next segment
  the button is not offered on the new video; that takes as long as playback
  needs, usually a second or two.
- **A hand-over that asks is followed by its address.** The add dialog keeps
  the address it was given, and the download list carries it, so the button
  finds its download without guessing. If the address is edited in the dialog,
  or the dialog is never confirmed, nothing is followed: the button says *Not
  started in the app* and clears itself, instead of showing a progress that
  belongs to nothing. The wait for the dialog is two minutes.
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
