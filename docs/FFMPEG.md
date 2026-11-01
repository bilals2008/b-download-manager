# FFmpeg sidecar — licensing, redistribution, and updates

B Download Manager ships an LGPL `ffmpeg` build as a Tauri sidecar to remux
HLS and DASH tracks into a single playable container (`-c copy`, no
re-encoding). This document covers the binary's origin, licence, redistribution
requirements, size impact, and update compatibility.

## Origin

The pinned build comes from [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds),
autobuild `2026-10-31-12-59`, revision `N-128227-g4811625b5f`. The exact build,
URL, and SHA-256 are pinned in `scripts/fetch-ffmpeg.mjs`. A release build
downloads, verifies, and places it at
`src-tauri/binaries/ffmpeg-<target-triple>[.exe]` before Tauri bundles it via
`externalBin`.

Pinned builds:

| Triple | File | SHA-256 |
|---|---|---|
| `x86_64-pc-windows-msvc` | `ffmpeg-N-128227-g4811625b5f-win64-lgpl.zip` | `6550f7ab…d48734` |
| `x86_64-unknown-linux-gnu` | `ffmpeg-N-128227-g4811625b5f-linux64-lgpl.tar.xz` | `f7ce163e…ee4b` |

macOS has no pinned build yet; that leg falls back to an `ffmpeg` on `PATH` at
runtime.

## Licence

The build is **LGPL** (no `--enable-gpl`, no `libx264`, `libx265`, or other
GPL-only codecs). GPL codecs are not needed because the application only copies
streams (`-c copy`), never re-encodes.

LGPL obligations that apply:

- The application remains under its own licence; linking an LGPL library does
  not make the application GPL or LGPL.
- The `ffmpeg` binary must remain replaceable by the user. Because it ships as
  a separate sidecar file beside the executable (not statically linked), this
  requirement is met: a user can substitute their own `ffmpeg` build.
- Source code for the `ffmpeg` build must be offered on request. BtbN's
  autobuilds publish their build scripts; direct BDM users who need the source
  can contact the maintainer.

## Redistribution

The `ffmpeg` binary is **not committed** to the repository
(`src-tauri/binaries/ffmpeg-*` is gitignored). Each release build fetches the
pinned build, verifies its SHA-256, and copies it into `src-tauri/binaries/`
before Tauri bundles it. This keeps the repository free of large binaries while
ensuring every released installer carries the exact pinned build.

`externalBin` in `tauri.conf.json`:

```json
"externalBin": ["binaries/ffmpeg*"]
```

Tauri places the sidecar beside the executable in the installed app, and
`video/tools.rs` locates it there (beside the app, then `binaries/`, then
`PATH`, then the `BDM_FFMPEG` override).

## Binary size

The Windows LGPL build is approximately 136 MB. This increases the installer
size accordingly; the previous "no sidecars, ~3.3 MB installer" decision
(ADR-0004) is superseded for the HLS/DASH feature. Progressive MP4/WebM
downloads are unaffected — they never invoke ffmpeg.

## Update compatibility

- The sidecar is pinned to a specific revision. `fetch-ffmpeg.mjs` checks the
  SHA-256 and fails loudly if the build is rotated, so a release never ships an
  unverified binary.
- The media path locates ffmpeg by name prefix (`ffmpeg-`), so a future build
  with a different revision string is found without code changes, as long as
  the target-triple suffix is preserved.
- macOS and Linux builds use their own pinned builds; Windows only uses the
  Windows build. Cross-platform updates are not affected by the sidecar.
- If ffmpeg is missing at runtime (e.g. a partial install), the join returns
  `MuxError::NotFound` and the download reports failure. No file is written to
  the destination, and the UI never reports success for a missing file.

## How the media path uses it

`video/tools.rs:Tool::Ffmpeg.locate()` searches, in order:

1. `BDM_FFMPEG` environment variable (tests and power users)
2. Beside the executable (`current_exe().parent()`)
3. `<exe-dir>/binaries/`
4. `<exe-dir>/../Resources` (macOS app bundles)
5. `PATH`

`video/mux.rs:join()` runs it with `-hide_banner -loglevel error -nostdin -y`,
`-c copy`, `-movflags +faststart` for MP4/M4A, and the appropriate `-f`
container flag. `video/verify.rs:plays()` decodes the first three seconds as a
playback check. Neither re-encodes.