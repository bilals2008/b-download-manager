# 14 — Own YouTube extraction + native muxer (drop `yt-dlp` and `ffmpeg`)

Status: implemented

## Why

Two symptoms are blocking real use:

1. The floating download button does not appear on YouTube.
2. YouTube downloads fail — the reused signed `videoplayback` address is refused
   with `403` once the page changes its signature, and the only thing after
   that is the `yt-dlp` last resort in `src-tauri/src/video/ytdlp.rs`.

The user wants both external tools gone and the capability rebuilt in-house,
the way the reference product (IDM) does it.

## What reverse-engineering IDM found

The IDM "Integration Module" extension (`IDMGCExt.crx`, `IDMEdgeExt.crx`,
`idmmzcc3.xpi`) was extracted. The payload is one obfuscated `content.js` that
runs at `document_start` on every frame, plus a `background.js` that relays to
the native app over `nativeMessaging`. The obfuscation is ROT-13 on the
site-specific regexes; decoding them shows exactly how IDM reads a YouTube page:

- It matches `ytcfg.set({ ... })` in an inline script to recover
  `PLAYER_JS_URL`, `STS`, `VISITOR_DATA`, `EMB_VISITOR_DATA`,
  `DATA_SYNC_ID`, `SESSION_INDEX` (the `h.cb` regex).
- It finds the video element with a selector list, not a guess:
  `[role="main"]:not(hidden) video`, `:scope > div#player video`,
  `ytmusic-player video` (the `h.eb` regex).
- It fetches the player script (`base.js`, from `PLAYER_JS_URL`) over XHR and
  locates, by regex, the **signature decipher function** (`h.$a`) and the
  **`n`-parameter transform function and its lookup array** (`h.ab`, `h.bb`).
- It reads the streaming formats from the page's player response (the same
  `ytInitialPlayerResponse` the project already parses) and produces a final,
  working `videoplayback` URL per itag by running the player's **own** transform
  functions **in the page's own JavaScript realm**.
- It posts those clean URLs to the native app, which is only a segmented HTTP
  downloader. IDM ships **no `yt-dlp` and no `ffmpeg`**: the fragile cipher
  logic lives in the page-side script, not in native code.

The takeaway: the project already does the "read `ytInitialPlayerResponse`"
half. What it is missing is the second half — fixing the `n` parameter and the
`signatureCipher` forms in-page — and that is exactly the part that makes a
reused address survive past the first play. That gap is the `403`, and the
`403` is why the fallback tool exists.

## Current state (what the plan touches)

- `extension-firefox/content/page-sources.js` reads `ytInitialPlayerResponse`
  and takes only the plain `format.url` field; it drops `signatureCipher`
  forms and does not transform `n`.
- `src-tauri/src/video/capture.rs` parses each itag URL as a plain
  `SourcePlan::Files` track.
- `src-tauri/src/video/download.rs` retries a `403` on a YouTube job through
  `super::ytdlp` (the fallback being removed here).
- `src-tauri/src/video/mux.rs` remuxes with the `ffmpeg` sidecar behind one
  function, `join`. ADR-0004 already names the native writer as "V4b — replaces
  the body of `join`".
- `src-tauri/src/video/tools.rs` locates both sidecars (`Tool::Ffmpeg`,
  `Tool::YtDlp`).
- `scripts/fetch-ffmpeg.mjs`, `src-tauri/binaries/`, `docs/FFMPEG.md` and the
  bundle config carry the `ffmpeg` sidecar.

## Plan

### Phase 0 — Reproduce and fix the YouTube button (small, first)

The two symptoms are described but not yet pinned to a line. Before the big
rewrite, reproduce against the dev build and record the exact break:

- Open a watch page with the devtools console. Check `bdmPageSources()`
  returns the itag list, `bdmCapturedSources()` returns URLs, and the overlay's
  `present()` runs on pointer-over.
- Confirm whether the button never renders (overlay/registry bug) or renders
  but the panel fails (bridge/parse bug). The failure mode decides whether
  Phase 1 alone fixes both symptoms.

Fix whatever surfaces here, in a focused commit, before touching the tooling.

### Phase 1 — Own YouTube extraction, drop `yt-dlp`

Mirror IDM's page-side approach in `page-sources.js` (and its helpers):

1. Also accept `signatureCipher` / `cipher` forms, not only `format.url`.
2. Recover `PLAYER_JS_URL` from `ytcfg.set({ ... })`; fetch the player script
   and locate the signature decipher and the `n` transform (the `$a` / `ab` /
   `bb` shapes above), the same way IDM's decoded regexes do.
3. Compute `n` and the signature **in the page realm**, then emit a clean,
   whole-file URL per itag. This replaces "reuse whatever the player fetched"
   with "produce a fresh working URL", so the `403` path is gone by design.
4. Rust side: delete `src-tauri/src/video/ytdlp.rs`, remove `Tool::YtDlp` from
   `tools.rs`, remove the fallback block in `download.rs` (lines ~295–350) and
   the `ytdlp_selector` helper, and drop `super::ytdlp::cancel` calls from
   `capture.rs`. A YouTube job that still cannot be built fails with the one
   sentence instead of silently invoking a tool.

Outcome: no `yt-dlp` anywhere; the app produces its own YouTube URLs.

### Phase 2 — Native remuxer, drop `ffmpeg`

Implement ADR-0004's V4b behind the existing `mux::join` boundary:

1. A pure-Rust container writer that copies bytes, never re-encodes:
   - **MP4** for `avc1`/`hev1` + `mp4a`/`ac-3`/`ec-3` (write a `moov` with the
     two tracks, interleave, `moov` first for faststart).
   - **MKV/WebM** for `vp9`/`av01` + `opus`/`vorbis` (EBML `SimpleBlock`).
2. Replace `verify.rs`'s `ffmpeg` playback check with a native probe (or drop
   the check if it was only there because `ffmpeg` was present).
3. Remove `Tool::Ffmpeg` from `tools.rs`, delete `scripts/fetch-ffmpeg.mjs`
   and the `src-tauri/binaries` sidecar + bundle entries, and retire
   `docs/FFMPEG.md`.

Outcome: no `ffmpeg` sidecar; the installer shrinks back to a few megabytes
(reversing the ADR-0004 size consequence).

### Phase 3 — Cleanup, ADR, notes, verification

- Mark ADR-0004 as superseded by the native writer (or add a new ADR recording
  the reversal and its rationale).
- Update `CHANGELOG.md` and `WHATS_NEW.md` (short, reader-facing; run
  `pnpm check:whats-new`).
- Remove any remaining `yt-dlp`/`ffmpeg` references in `scripts/`,
  `docs/DEVELOPMENT.md`, `docs/RELEASES.md`, `README.md` and the CI/release
  workflows.
- Run `pnpm check-all` and the Rust test suite; verify an end-to-end download
  of a YouTube video (picture + sound) and a plain-file download.

## Risks and open questions

- **The `n` parameter is a moving target.** YouTube changes the player script
  often. Running the player's own functions in-page (IDM's model) is the
  lowest-maintenance "own" answer, but it still needs the occasional regex
  touch-up when the player is reworked. A bundled JS interpreter in Rust is the
  heavier alternative and is rejected for now.
- **Native muxer scope.** MP4+MKV/WebM copying across the codec matrix is real
  but bounded work; no transcode means no codec work, only container rewrite.
- **Protected/DRM streams** (`vnd.yt-ump`) stay refused exactly as today.
