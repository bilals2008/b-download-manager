# DRM detection

**Wave 4 · Status:** 🔧 only a content-type refusal in `video/capture.rs`

## Goal

A protected stream is recognised and refused with one clear sentence before
anything is downloaded. Detection only — nothing is ever decrypted, and no
bypass is written or suggested.

## Where it stands

`video/capture.rs` refuses a response whose content type is `application/drm`,
and `video/yt.rs` checks for `drmParams` in the player response. The engine
itself has no rule, so an HLS or DASH manifest that carries protection reaches
the downloader and fails late, or downloads encrypted bytes.

## Steps

1. One module in the media layer that reads a manifest and answers "protected,
   because …": `#EXT-X-KEY` (METHOD=AES-128 with a key URI), DASH
   `ContentProtection`, `vnd.yt-ump` containers, and an encrypted init segment.
2. Check the answer before the first byte: at parse time for a manifest, and
   after the first response for a direct file.
3. Refuse with a reader's sentence naming the reason, in the panel and in the
   list row, and never retry.
4. Log the reason once, without the manifest body.
5. Keep the rule out of the engine's download path: the media layer decides
   before the engine is asked to start.

## Files

New `crates/idm-core/src/media/drm.rs`,
`crates/idm-core/src/media/hls.rs`, `crates/idm-core/src/media/dash.rs`,
`src-tauri/src/video/capture.rs`, `src-tauri/src/video/probe.rs`.

## Edge cases

- A clear manifest that points at one protected variant: the whole job is
  refused, not just that variant.
- A DRM-free stream behind a signed-in session is not DRM; do not conflate the
  two, the cookie consent path handles it.
- A false positive is worse than a late failure: require the marker, not a
  guess from the content type alone.

## Tests

Unit: a protected fixture and a clear fixture per format, and the sentence for
each reason. Integration: the test server serves each fixture, the jobs refuse
before any track is written.

## Proof

`cargo test -p idm-core`, then a real protected sample in the app: the panel
says why, the list has no row, and the log has one line.

## Out of scope

Decryption, key fetching, and anything that defeats protection.
