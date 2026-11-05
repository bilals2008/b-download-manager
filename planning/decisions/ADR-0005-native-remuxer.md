# ADR-0005 — Remux natively in Rust (V4b)

Status: accepted. Supersedes ADR-0004.

## Context

ADR-0004 chose an `ffmpeg` sidecar (V4a) behind `mux::join`, and named a native
writer (V4b) as the replacement that "replaces the body of `join` without
touching a caller". The sidecar added tens of megabytes to the installer and a
second supply chain, and the product now needs to stand on its own: its own
YouTube extraction and its own muxer, with no external tool.

## Decision

`mux::join` writes the container itself, with no child process and no
re-encode:

- **`src-tauri/src/video/mp4.rs`** — a progressive MP4/M4A writer. It reads a
  plain MP4 (`moov` sample tables) and a fragmented MP4 (`moov` init +
  `moof`/`mdat`), copies every sample and the codec's `stsd` entry verbatim, and
  writes `ftyp` + `moov` + `mdat` with the index first.
- **`src-tauri/src/video/webm.rs`** — a WebM/Matroska writer. It reads the EBML
  `Info`, `Tracks` and `Cluster` blocks, copies each block's frames, and writes
  `Info` + `Tracks` + ordered `Cluster`s, folding a `BlockGroup` into a
  `SimpleBlock`.

Both are reached through the unchanged `join(inputs, output, work_dir,
container)` signature. A container that neither writer covers is refused with
one sentence; bytes are never concatenated into a fake file.

## What was removed

- `src-tauri/src/video/ytdlp.rs` and the `403` → `yt-dlp` fallback in
  `download.rs`.
- `Tool::Ffmpeg` / `Tool::YtDlp` and `scripts/fetch-ffmpeg.mjs`.
- `docs/FFMPEG.md`, `src-tauri/binaries/`, and every `externalBin` entry.
- The ffmpeg playback check in `verify.rs`; the container/index check remains.

## Consequences

- **Smaller installer.** No external binary ships.
- **Coverage.** MP4 and WebM/Matroska are native. A TS→MP4 join (some HLS)
  is refused with one sentence, not silently concatenated.
- **A join is a byte copy** inside `spawn_blocking`; cancel is a checked flag,
  not a killed child.
- **YouTube** no longer relies on a reused URL or an external downloader: the
  extension reads `ytInitialPlayerResponse`, unscrambles a `signatureCipher`
  with the page's own player function, and hands over a real address.
