# Sidecar binaries

Release builds place the LGPL `ffmpeg` here as
`ffmpeg-<target-triple>[.exe]` (for example
`ffmpeg-x86_64-pc-windows-msvc.exe`) and bundle it as a Tauri sidecar through

```json
"externalBin": ["binaries/ffmpeg*"]
```

in `bundle` in `src-tauri/tauri.conf.json`. Tauri places the sidecar beside the
executable, and the media path locates it there first (`video/tools.rs`).
`yt-dlp` is optional and uses the same convention (`binaries/yt-dlp`).

The binary is not committed to this repository; a release build fetches the
pinned LGPL build with `node scripts/fetch-ffmpeg.mjs` (see `docs/FFMPEG.md`
for licensing and redistribution notes). Run `pnpm ffmpeg:fetch` to place it
for a local build. Without `ffmpeg`, a download still finishes its tracks and
reports that the join needs the tool; it never writes a half file or
re-encodes. See `planning/decisions/ADR-0004-muxer.md`.
