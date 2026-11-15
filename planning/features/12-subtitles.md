# Subtitles: manual and automatic captions

> **Nov 2026:** implemented (rendition or local file, muxed into MP4/MKV/WebM). See REMAINING-FEATURES.md for the current proof.

**Wave 4 · Status:** 🔧 `media/subtitles.rs` exists, nothing calls it

## Goal

A video download can carry its captions: the ones the page offers, or a file
the user picks, muxed as selectable tracks into MP4 or MKV without touching the
picture.

## Where it stands

`crates/idm-core/src/media/subtitles.rs` parses and stores caption tracks. The
video pipeline (`media/hls.rs`, `media/dash.rs`, `media/download.rs`) downloads
tracks and joins them, but never a subtitle track, and the panel has no choice
for one.

## Steps

1. Parse the manifest's subtitle renditions alongside audio and video, so they
   appear in the quality panel as their own rows.
2. Download a caption rendition to a temporary file with the same resume state
   as any track.
3. Mux with a pinned, checksum-verified FFmpeg, fetched and verified like the
   yt-dlp helper: MP4 gets `mov_text`, MKV gets `S_TEXT/UTF8`, video and audio
   are copied, never re-encoded.
4. Allow a local subtitle file: a picker in the panel, matched to the video by
   the user, added to the same mux.
5. Write the result to a temporary name and rename only after FFmpeg exits 0,
   so a crash never leaves a half-muxed film.
6. Report "no captions offered" honestly when the manifest has none.

## Files

`crates/idm-core/src/media/subtitles.rs`,
`crates/idm-core/src/media/hls.rs`, `crates/idm-core/src/media/dash.rs`,
`crates/idm-core/src/media/download.rs`, `src-tauri/src/video/download.rs`,
`src-tauri/src/video/mux.rs`, `src/components/windows/video/*`.

## Edge cases

- WebVTT with styling is muxed as text; styling is not carried into MP4. Say so
  in the hint rather than silently dropping it.
- Two languages: both tracks, named, first one default.
- A caption file whose timings do not match: not this app's problem to fix, do
  not shift timings.
- Burned-in captions are a transcode and are refused; only soft tracks.

## Tests

Unit: the rendition parser and the container-to-codec choice. Integration: a
local HLS fixture with one caption track, muxed, then ffprobe the result for the
subtitle stream.

## Proof

`cargo test -p idm-core`, then a real video in the app with captions chosen, and
the finished file showing two subtitle tracks in a player.

## Out of scope

Subtitle search or download from third-party sites; only what the page offers
or the user has.
