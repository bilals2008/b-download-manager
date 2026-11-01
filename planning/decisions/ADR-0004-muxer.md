# ADR-0004 — Remux with an `ffmpeg` sidecar behind one function

Status: accepted (V4a)

## Context

A media download produces two tracks (video and audio) or one fragmented
stream. The file the user gets has to be one container, and the bytes must not
be re-encoded: a transcode would change quality, cost CPU and break the promise
that the download is the stream the page served.

Two implementations were on the table:

- **V4a** — ship an `ffmpeg` build as a Tauri sidecar and run it with `-c copy`.
- **V4b** — write a native fMP4/Matroska writer (what the reference product
  does with `ts2mp4`), weeks of work plus a codec matrix.

## Decision

V4a. `ffmpeg` is run as a sidecar with `-c copy`, `-movflags +faststart` for
MP4, into a temporary file that is then replaced into place. The build must be
an **LGPL** build: no `--enable-gpl`, because the application is distributed
under its own licence and GPL codecs are not needed for copying streams.

Every caller goes through exactly one function:

```rust
video::mux::join(inputs: &[PathBuf], output: &Path, container: Container)
        -> Result<(), MuxError>
```

That signature is fixed. V4b, if the sidecar is ever refused, replaces the body
of `join` without touching the media coordinator, the panel or the bridge.

The container comes from the research's table, implemented in
`mux::choose_container`:

| video codec | audio codec | output |
|---|---|---|
| `avc1`, `hev1`/`hvc1` | `mp4a`, `ac-3`, `ec-3` | MP4 |
| `vp09`/`vp9`, `av01` | `opus`, `vorbis` | MKV |
| TS input | AAC | MP4 |

The extension is added to the page title after `bridge::dest::sanitize`, the
same name rules the Site Grabber uses. A taken name becomes `name (1).ext`.

## Consequences

- **Installer size.** The LGPL `ffmpeg` binary is tens of megabytes; the
  installer is no longer a few megabytes. This ADR supersedes the earlier
  "no sidecars, keep the installer tiny" note for this feature.
- **Supply chain.** The binary is not committed to the repository. A release
  build places it at `src-tauri/binaries/ffmpeg-<target-triple>[.exe]` (the
  Tauri `externalBin` convention) and adds that entry to `bundle.externalBin`.
  `mux::locate` also accepts `ffmpeg` beside the executable, on `PATH`, or the
  `BDM_FFMPEG` override used by the tests.
- **Failure is visible.** Without a binary the join returns one sentence and
  keeps both tracks; it never writes a half file and never falls back to a
  silent or concatenated container.
- **Cancellation.** One join runs at a time; `mux::cancel` kills the child and
  removes the temporary. The caller deletes the tracks only after success.
- **No re-encoding, ever.** `-c copy` is the only codec flag in the runner. A
  transcode is a bug.

## Alternatives rejected

- **Concatenating bytes.** Gives a `.ts` or a fragmented `.mp4` that many
  players refuse; not a deliverable.
- **Re-encoding to a common codec.** Violates the product's rule and the
  research's finding.
- **A native muxer now.** Correct in the long run, but not before the feature
  ships; the one-function boundary keeps the door open.
