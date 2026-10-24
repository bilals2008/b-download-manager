---
title: Getting the app
description: Where to get B Download Manager, and how to add its browser button.
---

## Is there an installer yet?

Not yet. B Download Manager is at version 0.1.0 and has not been released for download. If you are already using it, someone built a copy for you or gave you one.

When a version you can simply install exists, the download and the steps for Windows, macOS and Linux will be on this page.

## Add the browser button

The app comes with a small add-on for Firefox, Chrome and Edge. It adds **Download with B Download Manager** to the right-click menu, and it can take over the downloads your browser would otherwise start.

**Start B Download Manager first.** The add-on cannot open the app by itself, and without the app it can do nothing.

### Firefox

1. Open Firefox, type `about:debugging` in the address bar and press Enter.
2. Click **This Firefox** on the left.
3. Click **Load Temporary Add-on…**.
4. Choose the file called `manifest.json` inside the `extension-firefox` folder you were given.

Firefox forgets a temporary add-on when it closes. Load it the same way next time, or use a signed copy if you were given one.

### Chrome and Edge

1. Type `chrome://extensions` (or `edge://extensions`) in the address bar and press Enter.
2. Turn on **Developer mode**, in the top-right corner.
3. Click **Load unpacked** and choose the `browser-extension` folder.
4. Click the puzzle-piece icon in the toolbar and pin **B Download Manager** so its button stays visible.

### Use it

Right-click any link, picture, video or sound file on a page and choose **Download with B Download Manager**. The download appears in the app like any other, and carries on even if you close the browser.

Clicking the add-on's button shows a small popup that says whether it can see the app. If it says it cannot, start B Download Manager and open the popup again.

## Remove the add-on

- **Firefox:** open `about:debugging`, click **This Firefox**, and click **Remove** next to the add-on.
- **Chrome or Edge:** open the extensions page and click **Remove** on its card.
