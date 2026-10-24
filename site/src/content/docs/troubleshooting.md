---
title: Troubleshooting
description: What to try when a download, the browser button or the window does not behave.
---

## The browser button does nothing

- **Is the app running?** The add-on cannot start B Download Manager, and without it the button has nothing to send the file to.
- **Did Firefox restart?** Firefox forgets a temporary add-on when it closes. Load it again the way [Getting the app](/installation/) describes.
- **Chrome or Edge:** click the add-on's button. Its popup says whether it can see the app.
- **Right-click a link, not empty page space.** Pictures, videos and sound files work too. A page that only shows a player, like most video sites, is not a link to a file.

## A download will not start

Usually the link is no longer a file:

- Download links often expire after a few minutes. Copy a fresh one.
- The site may want you to be signed in. Open the link in your browser and see what it shows.
- It may go to a page rather than a file, which gives the browser something to display and the app nothing to save.

Copy the link again and try once more.

## A download stopped halfway

Select it and press **Resume**. If the file's website does not allow a download to be picked up, the app starts again from the beginning and says so.

## It is slower than expected

- Check **Options → Network** for a **Speed limit**, and the **Speed limiter** tab of the download itself.
- Several downloads at once share your connection. Pause the ones you do not need yet.
- Some websites only allow a file to be fetched in one piece, and then there is nothing to speed up.

## It says there is not enough space

The app checks the drive before and during the download, so it stops before the disk fills. Free some space and press **Retry**, or start again and pick a folder on another drive.

## The app seems to have disappeared

It is probably still running. Click its icon next to the clock. The icon's menu brings the window back and is also where **Exit** lives.

If closing the window should quit the app instead of hiding it, open **Options → General** and turn off **Keep the application in the tray when the window is closed**.

## The file will not open

Some sites hand back a web page instead of the file — usually a sign-in page or a "not found" page. The download looks finished but the file is small and nothing opens it. Open the original link in your browser to see what the site is actually sending.

## A settings page is empty

File types, Browser capture, Sites Logins, Dial Up / VPN and Sounds are not built yet. The page says so, rather than pretending to work.

## Asking for help

Send four things and most problems can be answered without a second round of questions:

1. The version, from **Help → About B Download Manager…**
2. Your version of Windows (or macOS, or Linux).
3. What you clicked, and what you expected to happen.
4. The exact words in the message, if there was one.
