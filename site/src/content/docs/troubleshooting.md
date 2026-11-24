---
title: Troubleshooting
description: What to try when a download, the browser button or the window does not behave.
---

## The browser button does nothing

- **Is the extension installed?** Firefox: install it from [Mozilla Add-ons](https://addons.mozilla.org/en-US/firefox/addon/b-download-manager/). Chrome, Edge, Brave and Safari: load the build as [Getting the app](/installation/#browser-extension) describes.
- **Is the app running?** The add-on cannot start B Download Manager, and without it the button has nothing to send the file to.
- **Did you load Firefox's copy by hand?** A temporary add-on is forgotten when Firefox closes. Install the signed copy from Mozilla Add-ons instead, or load it again the way [Building from source](/developers/#browser-extensions) describes.
- **Chrome, Edge or Brave:** the extension is not in the Chrome Web Store, so it lives in the folder it was loaded from. If `extensions/dist/chromium` was moved or deleted, run `pnpm build:extensions` and load it again.
- **Safari:** the extension appears only after the app that wraps it has been run once, and a Safari you built yourself needs **Develop → Allow Unsigned Extensions**.
- **Does the add-on say "Not running"?** Start B Download Manager. There is no code to copy and nothing to pair: the add-on reports **Connected** as soon as the app is open. If it says **Version mismatch**, the add-on and the app are from different releases.
- **Right-click a link, a picture, a video or a sound file.** On a page with a detected video, use the extension's download button instead: it lists the qualities the page offers. A page that only shows a player and offers no media link has nothing for the right-click hand-off.

## A download will not start

Usually the link is no longer a file:

- Download links often expire after a few minutes. Copy a fresh one.
- The site may want you to be signed in. Open the link in your browser and see what it shows.
- It may go to a page rather than a file, which gives the browser something to display and the app nothing to save.

Copy the link again and try once more.

## A download stopped halfway

Select it and press **Resume**. If the file's website does not allow a download to be picked up, the app starts again from the beginning and says so.

## It is slower than expected

- Check **Settings → Connections** for a **Speed limit**, and the **Speed limiter** tab of the download itself.
- Several downloads at once share your connection. Pause the ones you do not need yet.
- Some websites only allow a file to be fetched in one piece, and then there is nothing to speed up.

## It says there is not enough space

The app checks the drive before and during the download, so it stops before the disk fills. Free some space and press **Retry**, or start again and pick a folder on another drive.

## The app seems to have disappeared

It is probably still running. Click its icon next to the clock. The icon's menu brings the window back and is also where **Exit** lives.

If closing the window should quit the app instead of hiding it, open **Options → General** and turn off **Keep the application in the tray when the window is closed**.

## The file will not open

Some sites hand back a web page instead of the file — usually a sign-in page or a "not found" page. The download looks finished but the file is small and nothing opens it. Open the original link in your browser to see what the site is actually sending.

## A setting or menu item is missing

The Remote page (pairing a phone) is still on the way, and so are batch adding, export and import, and the drop target in the Tasks menu. They say so where they appear. Everything else in Settings works today.

## Asking for help

The app is public work in progress, so a problem is worth reporting: [open an issue](https://github.com/bilals2008/b-download-manager/issues/new) with four things and most problems can be answered without a second round of questions:

1. The version, from **Help → About B Download Manager…**
2. Your version of Windows (or macOS, or Linux).
3. What you clicked, and what you expected to happen.
4. The exact words in the message, if there was one.
