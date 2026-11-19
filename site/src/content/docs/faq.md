---
title: FAQ
description: Short answers about what the app does, what it costs, and what it does not do yet.
---

## Is it free?

There is nothing to buy, no account to create, and no ads. It is a personal project, not a product on sale.

## Does it need to be installed?

Yes. Every release has an installer for Windows, macOS and Linux. See [Getting the app](/installation/) for the file for your system. The Firefox extension is a separate install from [Mozilla Add-ons](https://addons.mozilla.org/en-US/firefox/addon/b-download-manager/).

## Which browsers can send links to the app?

Firefox, through the extension on Mozilla Add-ons. It hands links, videos and files to the app over a bridge that stays on your own computer. Chrome, Edge and Brave versions are still being finished.

## Which computers does it run on?

It is made for Windows and also works on macOS and Linux. Windows gets the extra polish: the window corners, the frame and the tray icon are tuned for it.

## Can I close the app while something downloads?

Yes. Closing the window leaves the app running next to the clock and the download keeps going. The icon's menu brings the window back.

## Will it carry on after a restart?

Yes. Every download keeps a note of how far it got, so after a reboot it resumes where it stopped — at worst a couple of seconds of work is fetched again.

## Can it download videos from streaming sites?

Yes. Pages that stream video in small pieces (HLS and MPEG-DASH) are captured and joined into one MP4 without re-encoding. On YouTube, the extension offers the qualities the page has and the captions it offers, and you can save just the sound as MP3, Opus or FLAC.

## Can I limit how fast it goes?

Yes, either for everything at once in **Settings → Connections**, or for a single download in its **Speed limiter** tab.

## How many downloads at a time?

The queue buttons at the top start and stop the whole list. Each file uses up to 32 connections, and **Settings → Connections** caps how many the app may open in total.

## Can I schedule downloads for later?

Yes. **Settings → Downloads** sets the days and hours the queue runs. It starts and stops by itself, and can turn the computer off once the queue is empty — never in the middle of a download.

## Does it collect anything about me?

No account, no analytics, no ads. The app reaches out for the files you ask it to download, checks for a newer version of itself, and talks to the browser extension on your own computer. Nothing about you is sent anywhere.

## Where are my downloads kept?

In the save folder you choose, sorted into a folder per kind — Videos, Music, Documents, Programs, Compressed and so on. General and Other files land in the save folder itself. You can give a category its own folder in **Settings → Save locations**, and pick a different folder for one download when you add it. Removing an entry from the list never deletes the file.

## Can I use the pictures and fonts in my own project?

Yes. The app is published under the [MIT licence](https://github.com/bilals2008/b-download-manager/blob/main/LICENSE); the bundled fonts keep their own open-font licences.
