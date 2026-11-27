---
title: Getting the app
description: Where B Download Manager comes from, and how to add the browser extension.
---

## Is there an installer yet?

Yes. Every release is published on
[GitHub Releases](https://github.com/bilals2008/b-download-manager/releases/latest),
with the files for each platform attached to it:

- **Windows 10 and 11** — the `-setup.exe` installer. It installs per user, so
  there is no admin prompt, and an update replaces it in place.
- **Linux** — the `.AppImage` runs on any distribution; the `.deb` is for
  Debian and Ubuntu.
- **macOS 11 or later** — one universal `.dmg` for Apple silicon and Intel. It
  is not signed by Apple yet, so the first launch needs right-click → **Open**.

The download button on the home page picks the file for the system you are on.
Phones and tablets get the releases page, because there is no build for them.

## Browser extension

The extension adds **Download with B Download Manager** to the right-click menu,
a button on supported videos, and a toolbar popup that follows the downloads
already in the app. Every browser runs the same extension; only the package
around it differs.

| Browser | How to install it |
| --- | --- |
| **Firefox** | Published on [Mozilla Add-ons](https://addons.mozilla.org/en-US/firefox/addon/b-download-manager/): open the page and choose **Add to Firefox**. |
| **Chrome and Edge** | Built from the repository and loaded once, below. |
| **Brave, Opera, Vivaldi, Arc** | The same Chromium folder, loaded from that browser's own extensions page. |
| **Safari 16.4 or newer** | Built from the repository and wrapped into an app on a Mac, below. |

Only the Firefox build is in a store. The Chromium and Safari builds are
installed from this repository, which needs
[Node.js](https://nodejs.org/) and pnpm. Both give the same features: the
right-click entries, the video button, the link window, the popup and the
settings page.

The extension needs the app running, because the hand-off goes to the copy on
your own computer. There is no code to copy and nothing to pair.

### Chrome, Edge, Brave, Opera or Vivaldi

```powershell
pnpm install
pnpm build:extensions
```

That writes `extensions/dist/chromium`. Then:

1. Open `chrome://extensions`, or `edge://extensions` in Edge (Brave, Opera and
   Vivaldi have their own extensions page).
2. Turn on **Developer mode**.
3. Choose **Load unpacked** and pick `extensions/dist/chromium`.

Chrome 110 or newer is required. Keep the folder where it is, because the
browser reads it on every start. After a new release, run
`pnpm build:extensions` again and press **Reload** on the extension card.

### Safari

Safari extensions ship inside an app, and the wrapper is made by Apple's
tooling, so this one needs a Mac with Xcode:

```bash
pnpm install
pnpm build:extensions
xcrun safari-web-extension-converter extensions/dist/safari
```

Run the generated app once, then open Safari → **Settings → Extensions** and
turn the extension on. A Safari you build yourself also needs **Develop → Allow
Unsigned Extensions** after each launch.

Safari 16.4 or newer is required. The folder is a complete web extension; what
it cannot do on Windows or Linux is become an installable Safari app.

### Where the extension lives

If you build the app yourself, the extension is in the same repository.
`extensions/source/` holds every script and page, each browser folder adds only
its manifest, and `extensions/build.mjs` puts the two together in
`extensions/dist/<browser>/`. [Building from source](/developers/#browser-extensions)
explains the wiring.
