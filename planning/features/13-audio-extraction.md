# Audio extraction and conversion

**Wave 4 · Status:** 🔧 `media/audio.rs` exists, nothing calls it

## Goal

From a video (or its audio track), produce an mp3, opus or flac file — or keep
the track exactly as it was downloaded, with no conversion at all.

## Where it stands

`media/audio.rs` can describe and pick an audio track. The video coordinator
downloads the audio rendition as-is and muxes it into the container. There is no
choice in the panel, no converter, and no setting.

## Steps

1. Add a "save as" row to the quality panel: original track, mp3, opus, flac.
2. For "original", write the downloaded track straight out with the right
   extension (m4a/aac, webm/opus), no FFmpeg call.
3. For a conversion, use the pinned FFmpeg with `-vn` and `-c:a` for the chosen
   codec; never re-encode video, because there is no video in the output.
4. Keep the conversion's progress: FFmpeg reports time, and the panel shows it
   as a percentage of the track's duration.
5. Verify the output exists and is non-empty before removing the temporary
   track; a failed conversion keeps both and says why.
6. Name the result after the video, never after the temporary track file.

## Files

`crates/idm-core/src/media/audio.rs`,
`crates/idm-core/src/media/download.rs`, `src-tauri/src/video/download.rs`,
`src-tauri/src/video/mux.rs`, `src-tauri/src/video/tools.rs` (the FFmpeg
fetcher), `src/components/windows/video/*`.

## Edge cases

- A source with no audio track: the row is absent, not disabled with a mystery.
- A very long track: conversion must not hold the window; it runs in the job.
- Opus in an MP4 container is unusual; for opus the container is WebM or Ogg,
  which FFmpeg decides from the extension.
- The user deletes the job mid-conversion: the temporary files go with it.

## Tests

Unit: the extension and codec choice per output. Integration: a short fixture
track converted to each of the three formats, each output read back by FFmpeg
for duration and codec.

## Proof

`cargo test -p idm-core`, then a real video download in the app saved as mp3,
and the file playing at the right length.

## Out of scope

Playlists, chapter splitting, and any video re-encode.
