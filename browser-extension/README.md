# The browser extension

This is the piece that makes a click in the browser start a download in
`B Download Manager`, the way the IDM extension does.

IDM hands a link to the application the other way round: the browser asks
the application to take a download over. The clone does the same thing with
a small server on the loopback interface, which is what a browser extension
can reach without anything being installed on the system. The application
listens on `127.0.0.1:27500`.

## Installing it in Chrome or Edge

1. Open `chrome://extensions` (or `edge://extensions`).
2. Turn on **Developer mode**.
3. Press **Load unpacked** and pick this folder.
4. Press the icon of the extension once. The popup says whether the
   application is running.

## What it does

- The right click menu on a link has **Download with Internet Download
  Manager**.
- A download the browser starts is taken over by the application, which
  opens its add dialog with the address and the name of the file already
  filled in. The switch in the popup turns that off.

## What it does not do yet

- The address of the page the link was on is sent along, but the
  application does not use it yet. A server that only serves a link to the
  page it was on will refuse the download until the referer is used.
- There is no native messaging host. IDM registers one (`com.tonec.idm`,
  the name is in the string table of `IDMan.exe`); the loopback server is
  the simpler way to reach the same result.
