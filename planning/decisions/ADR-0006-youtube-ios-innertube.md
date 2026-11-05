# ADR-0006 — Resolve YouTube through the iOS InnerTube client

Status: accepted.

## Context

YouTube's web player now streams through **SABR/UMP over POST**
(`application/vnd.yt-ump`). The formats in `ytInitialPlayerResponse` no longer
carry a download address, the legacy `get_video_info` / `api_video_info`
endpoints answer `410 Gone`, and the web/Android InnerTube clients reject an
unauthenticated player call (no poToken). A direct GET of what the web client
leaves behind answers `403`. The web player is also the ES6 modular build, whose
signature and `n` routines do not match the classic extraction.

That leaves the simple "grab the URL and GET it" model dead for the web client.

## Decision

`src-tauri/src/video/yt.rs` resolves a YouTube watch page itself, the way the
reference product keeps its own extractor:

1. Read `https://www.youtube.com/watch?v=<id>` with a desktop user agent and a
   `CONSENT` cookie, and take the `VISITOR_DATA` and `STS` values it carries.
2. POST the InnerTube `player` endpoint (`/youtubei/v1/player`) with the **iOS
   client** context (`clientName: iOS`, `clientVersion: 20.11.6`) and its
   headers (`x-youtube-client-name: 5`, `x-goog-visitor-id`, iOS user agent).
3. Parse `streamingData.formats` and `adaptiveFormats`; the iOS client returns a
   signed, direct `url` for every format (up to 2160p), with **no poToken, no
   cipher, no SABR**.
4. Build the same `Manifest` + `SourcePlan::Files` the media path already
   downloads and joins with the native MP4/WebM writers.

The browser extension no longer tries to read or evaluate the player script. On
a YouTube page it always offers its button and hands the **page address** to the
app; a captured range is sent when one exists, but the app detects a YouTube
page from its URL and resolves it regardless of the target it was given.

## Consequences

- **No new dependency.** The request is `reqwest` + `serde_json`, already in the
  app. No yt-dlp, no ffmpeg, no JavaScript library; the installer stays ~3.6 MB.
- **Every quality.** The iOS client returns the full ladder as plain GET
  addresses, so the existing segmented downloader runs at full speed and the
  native muxer joins picture and sound.
- **A maintained client.** The iOS client version and the request shape are the
  moving part; a version bump is one constant in `yt.rs`, not a rewrite.
- **The button always shows** on a YouTube page (like the reference product),
  because the page, not a pre-read stream address, is what the app needs.
