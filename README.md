<div align="center">

<img src="src/assets/brand/logo.png" alt="B Download Manager" width="88" height="88" />

# B Download Manager

A multi-connection desktop download manager for HTTP, HTTPS and FTP that resumes where it stopped.

[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-engine-000000?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-19-61DAFB?style=flat-square&logo=react&logoColor=black)](https://react.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.7-3178C6?style=flat-square&logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![Version](https://img.shields.io/badge/version-0.1.0-lightgrey?style=flat-square)](#)

</div>

## Overview

B Download Manager splits a file across multiple connections, writes each byte range to its own part file, and joins them when the last range lands. Pausing keeps the parts, closing the app keeps the manifest, and the next run continues from the bytes already on disk.

## Requirements

- Node.js 20+
- Rust 1.77+ (stable)
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
pnpm tauri build
```

Installer and binary land in `src-tauri/target/release/bundle/`.

## Tech Stack

- Shell: Tauri 2
- Engine: Rust (`idm-core`)
- Frontend: React 19, TypeScript 5.7, Vite 6
- Styling: Tailwind CSS v4 with semantic tokens in `src/index.css`

## License

No license file has been added yet. Contact the author before reuse or redistribution.

<div align="center">

<sub>Built by <a href="https://github.com/bilals2008">Muhammad Bilal Hassan</a> · <a href="https://mbilalhassan.vercel.app/">mbilalhassan.vercel.app</a></sub>

</div>
