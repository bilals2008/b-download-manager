---
title: Building from source
description: Requirements, commands and extension wiring for working on B Download Manager.
---

The pages before this one are written for people using the app. This one is for
whoever builds it.

## Requirements

| Tool | Notes |
| --- | --- |
| [Node.js](https://nodejs.org/) | 24, the version CI builds with |
| [Rust](https://rustup.rs/) | 1.77 or later, stable toolchain |
| pnpm | 12.15, pinned by `packageManager` in `package.json` |
| **Windows** | [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) (*Desktop development with C++*) and WebView2, which ships with Windows 10 and 11 |
| **macOS** | Xcode Command Line Tools: `xcode-select --install` |
| **Linux** | WebKitGTK and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) |

The window frame, tray icon and rounded corners are tuned for Windows. The
engine and the interface build on macOS and Linux, where window corners fall
back to the square shape the content draws itself.

## Run from source

```bash
pnpm install
pnpm tauri dev
```

The first run compiles the Rust engine and takes a few minutes; later starts are
fast.

The frontend on its own:

```bash
pnpm dev
```

That serves the interface at `http://localhost:1420` with no engine behind it,
so every download and every engine call fails. It is only useful for layout and
styling work.

## Build a bundle

```bash
pnpm tauri build
```

The installer and the binary land in `target/release/bundle/` at the repository root.
Application icons are generated from the single brand source:

```bash
pnpm tauri icon src/assets/brand/logo.png
```

## Browser extensions

The app runs a small JSON API on `127.0.0.1:27500`. It starts and stops with the
app and is reachable only from this machine. There is no native messaging host
to register and no registry entry to add, which is why the app must be running
before the extension can do anything.

One header says who is asking: `X-BDM-Client: browser-extension`. A web page
cannot put it on a cross-origin request without a preflight, and the preflight is
refused unless the origin is an extension's, so the app and the extension find
each other with no code to copy. A request whose `Host` is not the bridge's own
address is refused (that is the shape of DNS rebinding), an `Origin` is refused
unless it is a well-formed `moz-extension://` or `chrome-extension://` one, and
only such an origin is ever echoed back in `Access-Control-Allow-Origin`.

```powershell
pnpm probe:bridge    # against the running app: the rules, and one real download
```

The extension treats `/v1/ping` as the handshake: a different program holding
the port is reported as a mismatch, not as a connection.

The integration is written once, in `extensions/source/`, and each browser gets
a folder that adds only its manifest to it. `extensions/build.mjs` puts the two
together in `extensions/dist/<browser>/`.

- **Firefox** (`extensions/firefox/`): Manifest V3 with an event page. Load it
  temporarily from `about:debugging#/runtime/this-firefox`; the signed release is
  on Mozilla Add-ons.
- **Chrome, Edge, Brave, Opera and Vivaldi** (`extensions/chromium/`): Manifest
  V3 with a service worker, whose entry imports the shared background. Load it
  with **Load unpacked** on `chrome://extensions` or `edge://extensions`.
- **Safari** (`extensions/safari/`): Manifest V3 with a non-persistent
  background document, wrapped into an app by Apple's converter on macOS.

Every build reads the same versioned loopback API, and runs the same overlay,
link window and settings page. Chromium reaches them through
`extensions/source/lib/platform.js`, which adapts `chrome`'s callback API to the
promise-shaped `browser` the shared scripts use.

## What the application writes

Each download writes its part files beside the destination file, plus a
`.idm.json` manifest recording which ranges are finished and the referer the
browser passed along. The list of downloads is saved between runs, so a restart
brings the rows back with their state.

Removing a download stops it and drops it from the index. Nothing on disk is
deleted.

## What is not built yet

Named queues, batch adding, export and import, the drop target, phone pairing
(Remote) and live-stream recording are still open. The basic scheduler — days,
hours and power-off when the queue is empty — works today in **Settings →
Downloads**. `planning/09-remaining-work.md` in the repository tracks the rest,
though parts of it date from v1.0.
