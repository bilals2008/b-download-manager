# Chrome / Edge extension

The small Chromium build of the browser integration. It speaks the same
loopback API as the Firefox extension in `extension-firefox/`; that one has
the IDM-style video button and the full popup.

The application listens on `127.0.0.1:27500` and starts the server with
itself, so there is no native messaging host to register.

## Install

1. Start `B Download Manager`.
2. Open `chrome://extensions` (or `edge://extensions`).
3. Turn on **Developer mode**.
4. Press **Load unpacked** and pick this folder.
5. Click the extension icon. The popup says whether the app answered.

## What it does

- **Right click a link, video, audio element or image** →
  *Download with B Download Manager*.
- **Take over browser downloads** when the popup switch is on. The file is
  handed to the application **before** the browser copy is cancelled; if the
  app is closed, the browser download continues untouched.
- The popup checks `/v1/ping` with the client header, so a different program
  on the port is reported as a mismatch instead of a connection.

## Permissions

`contextMenus`, `downloads` and `storage`, plus the loopback host permission.
The background worker sends `X-BDM-Client: browser-extension`; the bridge
accepts only extension origins and refuses a page's preflight.

## Limitations

- No video overlay and no live download list; use the Firefox build for those.
- No options page. The takeover switch lives in the popup.
- The app must be running; a web extension cannot start a desktop program.
