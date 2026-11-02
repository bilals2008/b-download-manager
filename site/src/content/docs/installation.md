---
title: Getting the app
description: Where B Download Manager comes from, and how to add the Firefox extension.
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

The Firefox extension is published on
[Mozilla Add-ons](https://addons.mozilla.org/en-US/firefox/addon/b-download-manager/).
Install it, keep B Download Manager running, and it adds **Download with B
Download Manager** to the right-click menu, a button on supported videos, and a
toolbar popup that follows the downloads already in the app.

Chrome, Edge and Brave versions are still being finished; when they are ready
they will be in each browser's add-on store and this page will link to them.

The extension needs the app running, because the hand-off goes to the copy on
your own computer. There is no code to copy and nothing to pair.

If you build the app yourself, both extension sources are in the same
repository and [Building from source](/developers/#browser-extensions) explains
how to load them.
