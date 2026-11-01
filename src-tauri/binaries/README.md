# Sidecar binaries

Release builds place the optional LGPL `ffmpeg` here as
`ffmpeg-<target-triple>[.exe]` (for example
`ffmpeg-x86_64-pc-windows-msvc.exe`) and add

```json
"externalBin": ["binaries/ffmpeg"]
```

to `bundle` in `src-tauri/tauri.conf.json`. The media path locates a binary
beside the executable first, so a portable build can also ship it that way.
`yt-dlp` is optional and uses the same convention (`binaries/yt-dlp`).

No binary is committed to this repository. Without `ffmpeg`, a download still
finishes its tracks and reports that the join needs the tool; it never writes a
half file or re-encodes. See `planning/decisions/ADR-0004-muxer.md`.
