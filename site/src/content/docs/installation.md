---
title: Getting the app
description: Where B Download Manager comes from, and when the browser extension arrives.
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

There is no extension to install yet. The Chrome, Edge and Firefox versions are still being finished; when they are ready they will be in each browser's add-on store, and this page will link to them.

Once installed, it adds **Download with B Download Manager** to the right-click menu and can take over the downloads your browser would otherwise start. It needs the app running, because the hand-off goes to the copy on your own computer.

If you build the app yourself, the extension is in the same repository and [Building from source](/developers/#browser-extensions) explains how to load it.
