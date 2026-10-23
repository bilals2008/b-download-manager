<div align="center">

<img src="src/assets/brand/logo.png" alt="B Download Manager" width="96" height="96" />

# B Download Manager

**A desktop download manager that splits a file across connections, resumes where it stopped, and hands the browser's downloads over to itself.**

Multi-connection · Resumable · HTTP & FTP · Proxy aware · Tray resident

[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-engine-000000?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-19-61DAFB?style=flat-square&logo=react&logoColor=black)](https://react.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.7-3178C6?style=flat-square&logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![Tailwind CSS](https://img.shields.io/badge/Tailwind_CSS-4-06B6D4?style=flat-square&logo=tailwindcss&logoColor=white)](https://tailwindcss.com/)
[![Vite](https://img.shields.io/badge/Vite-6-646CFF?style=flat-square&logo=vite&logoColor=white)](https://vite.dev/)
[![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey?style=flat-square)](#requirements)

</div>

---

## Overview

One connection per file is how a browser downloads. B Download Manager asks the
server for byte ranges instead: a file is cut into contiguous ranges, each one
fetched on its own connection and written to its own part file, then joined at
the end. Pausing keeps the parts, closing the application keeps the manifest, and
the next run continues from the bytes that are already on disk.

The engine is a standalone Rust crate with no window toolkit in it, so the whole
behaviour — the probe, the ranges, the retries, the reassembly — is covered by
headless tests against a local HTTP server and a local FTP server. The desktop
application is a thin Tauri shell around it, and the interface is a Windows-style
download window: categories on the left, a live table on the right, the same
toolbar and menus the classic download managers use.

- **Segmented transfers** — one connection per range, up to 32, with a per-site cap and a global budget.
- **Resume, always** — a JSON manifest next to the part files; a stopped, paused or crashed download continues where it left off.
- **HTTP, HTTPS and FTP** — including `REST` offsets over FTP, one data connection per range.
- **Proxy aware** — HTTP and SOCKS proxies, applied to new downloads.
- **Speed control** — a global cap, a per-download cap, and a connection budget for the whole application.
- **Browser hand-off** — a Firefox extension with an IDM-style video button, live download controls and settings, plus a small Chrome/Edge build; both use a versioned loopback bridge inside the application.
- **A queue you can watch** — categories, folders, sortable columns, search, selection, pause/stop/resume, properties, and a drag-free table.
- **Its own completion window** — the IDM dialog with the file badge, the saved path, *Open file* and *Open folder*.
- **Lives in the tray** — closing the window hides it; the tray menu brings it back, starts and stops the queue, and exits.
- **Themed and local** — one token file drives light, dark and system appearance; eight self-hosted typefaces plus the system face, nothing fetched at runtime.

---

## Screens

| Window | What it does |
| --- | --- |
| **Main window** | Categories tree, views and per-folder drill-down; the live download table with size, progress, speed and time left; toolbar, menus and status bar |
| **Add new download** | Address first; the name, category and size follow from it and from the server's answer, the folder is picked with the system dialog |
| **Download complete** | The window IDM opens when a file finishes, with the file badge, the size, the saved path and the two open buttons |
| **Download progress** | The live window of one download: status, sizes, rate, time left, resume capability, the connections with what each one has and is doing, the speed limiter, and a minimize-to-tray button with a tray icon of its own |
| **Options** | General, Save to, Downloads, Connection and Proxy pages — theme, face, weight, folder, limits, tray, proxy |
| **Properties** | Address, destination, connections, status and error of one download |

---

## Tech Stack

| Layer | Technology |
| --- | --- |
| Shell | [Tauri 2](https://tauri.app/) — frameless window, own title bar, tray icon, dialogs |
| Engine | [Rust](https://www.rust-lang.org/) — [`idm-core`](./crates/idm-core) with its own HTTP and FTP clients, sessions and limits |
| Frontend | [React 19](https://react.dev/), [TypeScript](https://www.typescriptlang.org/), [Vite 6](https://vite.dev/) |
| UI | [shadcn/ui](https://ui.shadcn.com/) patterns, [Radix UI](https://www.radix-ui.com/), hand-drawn SVG artwork |
| Styling | [Tailwind CSS v4](https://tailwindcss.com/) with semantic tokens in `src/index.css` |
| Persistence | Part files, a JSON manifest per download, and a session index for the list |
| Tooling | `tsc`, `vite`, `cargo test`, clippy, and six project gates (`check:palette`, `check:fonts`, `check:selection`, `check:find`, `check:progress`, `check:extension`) |

---

## Getting Started

### Requirements

| Tool | Notes |
| --- | --- |
| [Node.js](https://nodejs.org/) | 20 or later |
| [Rust](https://rustup.rs/) | Stable toolchain |
| **Windows** | [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) (*Desktop development with C++*) and WebView2 (preinstalled on Windows 10/11) |
| **macOS** | Xcode Command Line Tools: `xcode-select --install` |
| **Linux** | WebKitGTK and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) |

The window frame, tray icon and round corners are tuned for Windows; the engine
and the interface build on the other two platforms, where the corners fall back
to the square ones the content draws itself.

### Install and run

```bash
npm install
npm run tauri dev
```

> The first run compiles the Rust engine and can take a few minutes. Later starts are fast.

Frontend only — the UI renders, but every engine call fails, which is expected:

```bash
npm run dev
```

---

## Build

```bash
npm run tauri build
```

The installer and the binary land in `src-tauri/target/release/bundle/`. The app
icons are generated from the single brand source:

```bash
npm run tauri icon src/assets/brand/logo.png
```

---

## Available Scripts

| Command | Description |
| --- | --- |
| `npm run tauri dev` | Run the desktop app (Rust + Vite) |
| `npm run dev` | Frontend dev server only |
| `npm run build` | Typecheck, then bundle the frontend |
| `npm run tauri build` | Production desktop bundle |
| `npm run check:palette` | WCAG 2.2 contrast gate for every colour token pair |
| `npm run check:fonts` | Every offered face has files, a stack and a settings value |
| `npm run check:selection` | The selection model: click, Ctrl, Shift and ranges |
| `npm run check:find` | The Find bar: the walk, the wrap and the first press |
| `npm run check:progress` | The progress window: percentages, connection words, rate sampling and speed-limit conversions |
| `npm run check:extension` | Both browser extensions: manifest, host permissions and hand-off |
| `cargo test --manifest-path crates/idm-core/Cargo.toml` | The engine, against a local HTTP and FTP server |
| `cargo test --manifest-path src-tauri/Cargo.toml` | The application and its commands |

---

## How It Works

1. **Probe.** A one-byte range request (`Range: bytes=0-0`) reveals the size and
   proves the server honours ranges. A `200` answer means the file is fetched on
   a single connection. Over FTP the size comes from `SIZE`.
2. **Segment.** The file is cut into contiguous byte ranges, one per connection.
   IDM's rule is one range per 10 KB of the file, capped by what the user allows
   (`DEFAULT_CONNECTIONS` is 8, the hard ceiling is 32).
3. **Download.** Each connection asks for its own range with
   `Range: bytes=<start>-<end>` and `Accept-Encoding: identity`, and writes into
   its own part file. Over FTP each range opens its own data connection and uses
   `RETR` after `REST`. Requests name themselves like a browser, so a server that
   only serves browsers does not answer `403`.
4. **Verify.** The head of the file is kept and compared with the head of every
   other range. A range that holds the head of the file was not honoured: its
   bytes are given back and the file is fetched on a single connection.
5. **Reassemble.** When the last range lands, the parts are joined in order and
   renamed onto the destination.
6. **Resume.** A JSON manifest next to the part files records what is done.
   A manifest whose ranges overlap or are out of order is never resumed from.

The engine notifies the interface through one event, `download://update`, so the
table and the completion window all read the same state.

### Notes on the engine

The behaviour is a port of the mechanics reverse engineered out of `IDMan.exe`
(IDM 6.43.15.2). [`ANALYSIS.md`](./ANALYSIS.md) holds the evidence — the exact
strings, addresses and pseudo-code the design rests on — and an explicit list of
what could **not** be established.

**On the number of connections.** IDM's rule (one per 10,240 bytes, capped by the
user) is implemented as found, but its worth depends on the server and the link.
A measurement against one local server (20 MB, the same server, back to back)
gave 19.5 s with eight connections against 23.0 s with one — while the link
itself moved by a factor of five between runs. The honest conclusion is that the
measurement did not settle the question; it needs a link that is not throttled.
What it did settle was a bug: a `429` answer with a one-byte body was mistaken for
a one-byte file. The probe now fails on any non-success answer, and
`a_server_that_is_busy_is_not_mistaken_for_a_small_file` covers it.

---

## Browser Integration

The application runs a versioned JSON API on `127.0.0.1:27500`; it starts with
the application and is reachable only from this machine. A request must carry
`X-BDM-Client: browser-extension`, and an `Origin`, when present, must be an
extension origin. Neither build needs a native messaging host or a registry
entry; the bridge is the application itself.

| Browser | Folder | What it adds |
| --- | --- | --- |
| Firefox | [`extension-firefox/`](./extension-firefox/) | Right-click hand-off, browser download takeover, an IDM-style hover button on supported HTML5 video with live progress, a compact popup with pause/resume/stop, page media and settings; MV3 event page |
| Chrome / Edge | [`browser-extension/`](./browser-extension/) | Right-click hand-off and browser download takeover with the same bridge contract |

The bridge offers `ping`, `downloads` (list), `POST /v1/downloads`,
`pause`, `resume`, `cancel`, `show` and `focus`. A hand-over opens the app's add
dialog when *Ask before saving* is on, or starts the engine directly otherwise.
The extension checks the app id and API version before trusting the port, so a
closed app, a stale app and an unrelated program are reported differently.

Install steps, permissions, the API table and the honest limitations are in
[`extension-firefox/README.md`](./extension-firefox/README.md). For a quick
development run:

1. Start the app (`npm run tauri dev`).
2. `about:debugging#/runtime/this-firefox` → **Load Temporary Add-on…** →
   `extension-firefox/manifest.json`.
3. Right-click a link or hover a direct-file HTML5 video.

A permanent Firefox install needs a signed `.xpi`; the README explains the AMO
and Developer Edition routes.

---

## Desktop Integration

The tray icon is the way the application stays alive: closing the window hides
it, the tray menu brings it back, starts and stops the queue, and exits. The
switch is on **Options → General**.

A download progress window has its own **Minimize to tray** button beside the
minimize button, and every open progress window owns a tray icon of its own,
labelled with the file name: clicking it brings that window back, so a window
hidden there comes back without a trip through the download list. Right-click
its tray icon for **Show window** and **Close window**.

Windows are drawn by the application, not by the platform: the title bar, the
menus, the toolbar and the two small windows all use the same tokens, so a theme
change or a different typeface updates every surface at once. The small windows
take their corners from the compositor rather than drawing a second radius
inside it, so all four corners are the same shape on every Windows version.

---

## Project Structure

```
crates/
  idm-core/          the engine: no window toolkit, no application
    downloader.rs      the download itself: ranges, retries, pause, assembly
    http.rs            the wire format: range headers, Content-Range, retries
    ftp.rs             the FTP client: login, PASV, REST, RETR
    transfer.rs        where the bytes come from: HTTP or FTP
    verify.rs          the check that a range's data belongs to that range
    limits.rs          connection budget, per-site caps, speed limit, proxy
    disk.rs            free space on the destination volume
    segmenter.rs       byte ranges and their part files
    session.rs         the index of known downloads, so the list survives a restart
    log.rs             the diagnostics log
  idm-cli/           a headless client for the engine
src-tauri/src/
  lib.rs             the Tauri commands, the reporter and the tray
  actions.rs         start, pause, resume and cancel, shared by window and bridge
  bridge/
    mod.rs           the loopback routes, the versioned contract and its tests
    http.rs          the small HTTP/1.1 reader and writer
    dest.rs          file naming, sanitising and collision-free destinations
  complete.rs        the completion window
  progress.rs        the live download window and its tray icon
  state.rs           the known downloads, limits and saved preferences
  window_style.rs    the rounded frame
src/
  App.tsx            the shell: title bar, menus, toolbar, workspace, status bar
  components/
    shell/           title bar, menu bar and popups, toolbar, commands
    workspace/       categories pane, download table, dialogs, context menus
    windows/         the completion window of a finished file, and the progress
                     window of a live one (status, connections, speed limiter)
    settings/        the Options pages
    ui/              the primitives (button, dialog, listbox, table, ...)
  lib/
    ipc.ts           typed wrappers over the Tauri commands
    downloads.ts     the download model, categories, sorting, formatting
    settings.ts      preferences and their storage
    theme.ts         light, dark and system
    fonts.ts         the offered faces and how they are applied
  index.css          design tokens: colour, radius, type, motion
docs/DEVELOPMENT.md  how the project is put together and what the gates check
planning/            scope, engine spec, UI direction, milestones, ADRs
```

---

## Tauri Commands

| Command | Purpose |
| --- | --- |
| `start_download(url, dest, connections, referer)` | Begin a download, returns its id |
| `list_downloads()` | Current state of every download |
| `pause_download(id)` / `resume_download(id)` / `cancel_download(id)` | Suspend, continue, stop and keep the parts |
| `remove_download(id)` | Drop it from the list, after stopping it |
| `probe_link(url, referer)` | The name and the size the server gives a link |
| `set_speed_limit(bytes_per_second)` | Bandwidth cap of the whole application |
| `set_download_speed_limit(id, bytes_per_second)` | Bandwidth cap of one download, returns the rate the engine holds |
| `open_progress_window(id)` | Open the live progress window of a download, or bring it back |
| `set_progress_details(open)` | Fold the connection details of that window, and size it to match |
| `set_site_connections(host, connections)` / `site_connections(host)` | Connection cap of one site |
| `set_max_connections(max)` | How many connections the application may hold open |
| `set_proxy(url)` | The proxy new downloads go through, HTTP or SOCKS |
| `set_minimize_to_tray(on)` / `set_show_complete_dialog(on)` | The two switches of the General page |
| `default_destination()` | A sensible starting path |
| `open_path(path)` | Open a finished file, or the folder it sits in |
| `quit_app()` | Leave the application for good |

The backend pushes `download://update` with the same `DownloadInfo` shape, and
`tray://command` carries the tray menu into the window.

---

## Design System

One file owns the appearance: [`src/index.css`](./src/index.css). Components read
semantic tokens (`--background`, `--chrome-panel`, `--chrome-line`, `--progress`)
and never a raw colour, so the whole window — title bar, menus, table, the two
small windows — switches together.

| Aspect | Tokens | Gate |
| --- | --- | --- |
| Appearance | `data-theme="light"` over a dark base, `auto` follows the system | `check:palette` |
| Typeface | nine faces, each with a `data-font` stack; Inter stays second as the fallback | `check:fonts` |
| Weight | `regular`, `medium`, `semibold`, each moving the whole scale by one step | `check:fonts` |
| Surfaces | one canvas, hairlines instead of second shades, `--radius` for panes and controls | `check:palette` |

Eight of the nine faces ship with the application — Inter, Google Sans Flex,
Geist, DM Sans, Plus Jakarta Sans, Instrument Sans, IBM Plex Sans and JetBrains
Mono; the ninth is the system face. No request leaves the application to fetch a
font.

---

## Documentation

| Document | Contents |
| --- | --- |
| [`ANALYSIS.md`](./ANALYSIS.md) | The reverse engineering: strings, addresses, pseudo-code, and what could not be established |
| [`docs/DEVELOPMENT.md`](./docs/DEVELOPMENT.md) | Conventions, module boundaries and the checks a change has to pass |
| [`PROGRESS.md`](./PROGRESS.md) | What works, what is left, and the gates as they stand |
| [`planning/`](./planning/) | Scope, engine spec, UI direction, milestones, verification, risks, ADRs |

---

## Contributing

1. Fork the repository and create a branch (`git checkout -b feature/amazing-feature`)
2. Keep the engine free of window code: `idm-core` must stay testable headless
3. Run the gates before opening a pull request:

```bash
npm run build
npm run check:palette && npm run check:fonts && npm run check:selection && npm run check:find && npm run check:progress && npm run check:extension
cargo test --manifest-path crates/idm-core/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Conventional Commits are welcome (`feat:`, `fix:`, `docs:`, `refactor:`). New
colours go in `src/index.css` first, so the palette gate can check them; new
components read tokens, never literals.

---

## Author

**Muhammad Bilal Hassan**

[![GitHub](https://img.shields.io/badge/GitHub-bilals2008-181717?style=flat-square&logo=github)](https://github.com/bilals2008)
[![Website](https://img.shields.io/badge/Website-mbilalhassan.vercel.app-00b849?style=flat-square&logo=vercel&logoColor=white)](https://mbilalhassan.vercel.app/)

---

## Acknowledgements

The interface and the engine's behaviour follow Internet Download Manager 6.43,
which is a product and a trademark of Tonec Inc. This project is an independent
reimplementation for interoperability and study; it is not affiliated with,
endorsed by, or distributed by Tonec.

<div align="center">

**Built with care by [Muhammad Bilal Hassan](https://github.com/bilals2008)**

</div>
