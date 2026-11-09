# ADR-0007 — Offer `yt-dlp` as the YouTube resolver, keep the native path

Status: accepted.

## Context

ADR-0006 replaced the external extractor with the app's own iOS InnerTube
client. That client still answers for many videos, but YouTube changes its
player often and some pages come back as `403` on the first segment; the
in-house code has no signature or throttling solver to repair them.

The reference product installs `yt-dlp` and `ffmpeg` on demand. Its helper
works on the pages the in-house client fails, and its download of the tool is
already accepted by users.

## Decision

- The **resolver** is the optional `yt-dlp` helper when it is installed
  (`video/ytdlp.rs`). It is asked for page metadata only (`-J`); the app
  downloads the direct addresses with its own engine, so multi-connection
  speed, resume and the native muxer still apply.
- The **in-house resolver** (`video/yt.rs`) stays as the fallback. A fresh
  install, or a machine where the helper download failed, still reads a page.
- `ffmpeg` is not installed. The native MP4/WebM writers already join picture
  and sound, and the installer stays small.
- The helper is downloaded on demand from its official GitHub release, checked
  against the publisher's `SHA2-256SUMS`, and kept at
  `<app_data_dir>/tools/yt-dlp.exe` (`BDM_YTDLP` overrides it for tests). A
  checksum mismatch discards the bytes.
- The offer appears inside the video panel of a YouTube job while the helper
  is missing, not at startup. Installing re-reads the current job.

## Consequences

- **Fewer failed YouTube downloads**, and the fragile extraction is no longer
  the only path.
- **A moving helper.** `yt-dlp` is updated by its publisher; the app always
  fetches the latest release when the user installs it.
- **No new install weight.** The tool arrives after the user says yes; the
  bundle still ships no video tool.
- **The in-house path remains tested**, because it is what runs before the
  helper exists.
