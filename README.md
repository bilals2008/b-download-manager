<div align="center">

<img src="src/assets/brand/logo.png" alt="B Download Manager" width="88" height="88" />

# B Download Manager

A multi-connection desktop download manager for HTTP, HTTPS and FTP that resumes where it stopped.

[![CI](https://github.com/bilals2008/b-download-manager/actions/workflows/ci.yml/badge.svg)](https://github.com/bilals2008/b-download-manager/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/bilals2008/b-download-manager?style=flat-square&label=release)](https://github.com/bilals2008/b-download-manager/releases/latest)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-engine-000000?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-19-61DAFB?style=flat-square&logo=react&logoColor=black)](https://react.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.7-3178C6?style=flat-square&logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![Version](https://img.shields.io/badge/version-1.0.0-lightgrey?style=flat-square)](#)
[![License](https://img.shields.io/badge/license-MIT-2ea44f?style=flat-square)](LICENSE)

</div>

## Overview

B Download Manager splits a file across multiple connections, writes each byte range to its own part file, and joins them when the last range lands. Pausing keeps the parts, closing the app keeps the manifest, and the next run continues from the bytes already on disk.

Around that engine:

- **Browser capture.** A Firefox extension and a Chrome/Edge build hand links and media to the app over a loopback bridge, so a download starts without pasting a URL. Settings owns the capture switch, the file types and the site list.
- **Video and media.** HLS, MPEG-DASH and plain-file tracks are captured with the quality list; the two tracks are joined into one file by a native MP4 remuxer, with no re-encode and no external tool.
- **Site Grabber.** Walk a page, filter by extension, size and depth, and start only the files you want.
- **Queue and recovery.** Ordered downloads with a simultaneous-download limit, categories, conflict dialogs and resume after a restart.
- **Desktop fit.** System tray with three mark styles, toolbar packs, remembered window state, and an in-app What's New after each update.

## Requirements

- Node.js 24 (the version CI builds with) and pnpm 12.15 (pinned by `packageManager`)
- Rust 1.77+ (stable) with the `rustfmt` and `clippy` components
- Windows: Microsoft C++ Build Tools + WebView2
- macOS: Xcode Command Line Tools
- Linux: WebKitGTK + Tauri prerequisites

## Run from source

```bash
pnpm install
pnpm tauri dev
```

## Build

```bash
pnpm tauri build                            # the default bundle for this OS
pnpm tauri build --bundles nsis             # Windows installer
pnpm tauri build --bundles deb,appimage     # Linux packages
pnpm tauri build --target universal-apple-darwin --bundles app,dmg   # macOS
```

Bundles land in `target/release/bundle/`. The media path is fully native, so a bundle needs no external video tool.

Release builds are signed for the in-app updater. The signing key and the rotation procedure are documented in [docs/RELEASES.md](docs/RELEASES.md). The workflow publishes a **draft** release that a person reviews and publishes by hand; installed apps never see a draft.

## Tests and checks

```bash
cargo test --workspace          # the app and the engine crate
pnpm test                       # frontend unit tests (Vitest)
pnpm check:all                  # every self-check script, including the release contract
pnpm verify                     # check:all + typecheck + unit tests, what to run before pushing
```

CI runs the same commands on every pull request, adds a Windows leg for the `cfg(windows)` code, lint-checks the whole workspace, and builds a signed production bundle on every push to `main`. The release workflow runs the full gate on the tag, builds the draft, and verifies every signature and `latest.json` against the key baked into the app before a person publishes it.

## Tech Stack

- Shell: Tauri 2
- Engine: Rust (`crates/idm-core`), one workspace with the app
- Frontend: React 19, TypeScript 5.7, Vite 6
- Styling: Tailwind CSS v4 with semantic tokens in `src/index.css`

## License

Released under the [MIT License](LICENSE). Contributions are accepted under the
same terms; see [CONTRIBUTING.md](CONTRIBUTING.md) and
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Please report vulnerabilities
privately as described in [SECURITY.md](SECURITY.md).

<div align="center">

<sub>Built by <a href="https://github.com/bilals2008">Muhammad Bilal Hassan</a> · <a href="https://mbilalhassan.vercel.app/">mbilalhassan.vercel.app</a></sub>

</div>
