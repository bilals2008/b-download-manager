# 11 - Media download: implementation plan (handoff)

This is a work order, not a design essay. It is written for the next agent: it
says what exists, what is missing, in what order to build it, which files to
touch, and how to prove each phase works.

Read with:
[10-video-capture-research.md](./10-video-capture-research.md) (how the
reference product does it, with evidence), [04-engine-spec.md](./04-engine-spec.md)
(the engine it must fit into) and [07-verification-plan.md](./07-verification-plan.md).

No code is written here on purpose. Every phase below is sized to land on its
own and to be shippable on its own.

---

## 0. Ground rules for the implementing agent

1. Read `AGENTS.md` and `docs/DEVELOPMENT.md` first. Long files stay long: no
   growing `App.tsx`, `lib.rs`, `downloader.rs` or `menu-bar.tsx`.
2. Do not re-derive the research. The evidence, addresses and the reference
   product's behaviour are in doc 10.
3. Every phase ends with its gates green (`cargo test`, `pnpm build`,
   `pnpm check:palette`) and its acceptance list ticked.
4. No re-encoding, ever. This feature downloads and remuxes. A transcode is a
   bug.
5. When a phase is done, tick the matching box in
   [06-milestones.md](./06-milestones.md) M6 and update
   [README.md](./README.md) current-state table.

---

## 1. Goal and non-goals

**Goal.** A user watching a video in the browser gets the app's button over the
player, picks a quality, and gets one playable file on disk: audio and video
already joined, correct container, correct name. Same outcome as the reference
product, through a cleaner pipeline.

**In scope**

- HLS (`m3u8`), MPEG-DASH (`mpd`), and plain progressive media (`mp4`, `webm`).
- A quality list per player, with audio paired to video automatically.
- Segment download through the existing range engine, resume included.
- Remux to `mp4` / `mkv` without re-encoding.
- Extensions for both builds (`extension-firefox`, `browser-extension`).

**Out of scope, permanently**

- DRM of any kind: Widevine, PlayReady, FairPlay, YouTube UMP
  (`application/vnd.yt-ump`). Detect, refuse, say why.
- Breaking an encrypted HLS playlist (`#EXT-X-KEY`). Refuse, same as the
  reference of the open-source comparison in doc 10.
- Solving YouTube's signature cipher or `n` throttle in our own code.
- Live stream recording (see V7 note: deliberately last).

---

## 2. What already exists (verified)

Do not rebuild these.

| Piece | Where | State |
|---|---|---|
| Range engine, part files, assembly, resume | `crates/idm-core/src/downloader.rs`, `segmenter.rs`, `session.rs` | working |
| Probe, `Content-Range`, retries, backoff | `crates/idm-core/src/http.rs`, `transfer.rs` | working |
| Connection budget, per-site caps, speed limit | `crates/idm-core/src/limits.rs` | working |
| SSRF / private-address policy | `src-tauri/src/grabber/policy.rs` (`addr_is_public`) | working, reusable |
| Loopback bridge for the extensions | `src-tauri/src/bridge/mod.rs` | working, routes listed below |
| Download start path | `src-tauri/src/actions.rs::start` | working |
| Video button settings (style, label, hover) | `src-tauri/src/state.rs`, `src/components/settings/video-download-button-settings.tsx`, `GET /v1/video-button-settings` | working |
| `<video>` discovery, overlay button | `extension-firefox/content/video-registry.js`, `page-media.js`, `overlay.js` | working |
| Media classification by itag, audio/video/muxed | `extension-firefox/lib/media.js` | working |
| Chrome/Edge build | `browser-extension/` (`background.js`, `manifest.json`, `popup.js`) | thin, needs parity |

Bridge routes today: `GET /v1/ping`, `GET /v1/capture`,
`GET /v1/video-button-settings`, `GET|POST /v1/downloads`,
`GET /v1/batch-options`, `POST /v1/downloads/batch`, `POST /v1/focus`,
`POST /v1/downloads/{id}/{pause|resume|cancel|show}`.

### The one line that decides this whole feature

`extension-firefox/lib/media.js` today:

```
A manifest is never offered, because it would save as a playlist.
```

Everything below exists to make that line false.

---

## 3. Gap list

| # | Gap | Consequence today |
|---|---|---|
| G1 | `.m3u8` / `.mpd` are rejected by design | the most common video sites give nothing |
| G2 | Only per-request URLs are captured, never the parsed playlist | no segment list, no size, no duration |
| G3 | No quality grouping: every stream is its own row | user picks "1080p video only" and no audio |
| G4 | No audio track pairing | silent file at 1080p+ |
| G5 | No remux in the engine | segments would be saved as one `.ts` / `.m4s` mess |
| G6 | Bridge has no way to carry the page's cookies/headers | protected sites fail |
| G7 | Chrome build has no media layer at all | feature is Firefox-only |

---

## 4. Target architecture

```
extension                          app (Rust)                        engine crate
---------                          ----------                        ------------
page-media.js  --manifestUrl-->    bridge POST /v1/media
                                    video/capture.rs  -> policy check
                                    video/probe.rs    -> GET manifest (cookie-aware)
idm-core::media::hls / dash  <----  parse (shared crate, no windows)
  -> MediaManifest { formats }
                                    video/window.rs  -> quality panel (Tauri window)
user picks a format ------------>   bridge POST /v1/media/{id}/download
                                    one Download per track  (existing engine)
                                    video/mux.rs -> ffmpeg -c copy (or native)
  <-- progress events -------------  progress.rs (existing)
```

Three rules that keep it clean:

- **All parsing lives in `crates/idm-core/src/media/`.** No windows, no Tauri,
  testable from `cargo test` and from `idm-cli`.
- **The engine downloads tracks; it does not know what a video is.** A track is
  a normal `Download` with its own part files and manifest. The media layer is a
  coordinator on top, not a new engine.
- **The extension captures, it does not parse.** One exception only: the
  YouTube player response, because it lives in the page (V6).

---

## 5. Contracts (agree before coding)

### 5.1 Extension -> app: register a media candidate

`POST /v1/media`

```
{
  "pageUrl":     "https://site/watch/123",
  "manifestUrl": "https://cdn.site/master.m3u8?token=...",
  "kind":        "auto" | "hls" | "dash" | "file",
  "title":       "Page title, for the file name",
  "referer":     "https://site/watch/123",
  "headers":     { "Cookie": "...", "User-Agent": "..." }   // optional
}
```

Answer `202 { "job": 7 }`. `headers` is the only new trust boundary: it is
accepted only for the manifest's own origin, never written to the session index,
never logged, and dropped as soon as the tracks are started.

### 5.2 App -> extension: what was found

`GET /v1/media/7`

```
{
  "job": 7,
  "state": "parsing" | "ready" | "failed",
  "error": null,
  "title": "...",
  "duration": 631.2,
  "formats": [
    { "id": "137", "kind": "video", "container": "mp4", "codec": "avc1.640028",
      "width": 1920, "height": 1080, "fps": 30, "bandwidth": 4200000,
      "size": 331000000, "label": "MP4 1080p" },
    { "id": "251", "kind": "audio", "container": "webm", "codec": "opus",
      "bandwidth": 131000, "size": 10300000, "label": "Audio 128 kbps" }
  ],
  "pairs": [ { "video": "137", "audio": "251", "label": "MP4 1080p" } ]
}
```

### 5.3 User -> app: download one pairing

`POST /v1/media/7/download`

```
{ "video": "137", "audio": "251", "name": "Video title", "dest": "C:\\Users\\me\\Videos" }
```

Answer `{ "downloads": [41, 42], "download": 41 }`. One id for the user-facing
row, the ids of the tracks for the progress window's connection list.

### 5.4 Rust types (names fixed, bodies are the agent's choice)

In `crates/idm-core/src/media/mod.rs`:

- `MediaKind` - `Video`, `Audio`, `Muxed`
- `Format` - the fields of the JSON above
- `Manifest` - `title`, `duration`, `formats`, `container_hint`
- `MediaError` - one enum, `Display` messages a user can read, same tone as
  `TransferError`
- `Parse` trait or plain functions `hls::parse(text, base_url)` and
  `dash::parse(text, base_url)`; plain functions are enough

In `src-tauri/src/video/`:

- `capture.rs` - the bridge handlers, policy checks
- `probe.rs` - the manifest GET (cookie header, size cap, redirect cap)
- `window.rs` - the quality panel window (label `video`, add it to
  `capabilities/default.json`, call `round_corners` like the grabber does)
- `mux.rs` - the remux runner and its temp-file discipline

---

## 6. Phases

Sizes are for one agent working carefully, not calendar promises.

### V0 - Contracts and harness (0.5 day)

- Add `crates/idm-core/src/media/mod.rs` with the types above, no parsing.
- Add a test-only HTTP server module `crates/idm-core/src/media/testserver.rs`
  modelled on `crates/idm-core/src/ftp/server.rs`: serves a fixture directory,
  answers ranges, can be told to fail or 404 a segment.
- Add `crates/idm-core/src/media/fixtures/` with three tiny things: a 2-segment
  HLS playlist with an init segment, a 2-representation MPD, and a bad copy of
  each. Keep the files under a few KB - they are text, not media.

**Done when:** `cargo test -p idm-core media` runs and the fixtures load.

### V1 - HLS, end to end (3 days)

Tasks

1. `media/hls.rs`: parse a master playlist (variants: `BANDWIDTH`, `RESOLUTION`,
   `CODECS`, `FRAME-RATE`) and a media playlist (`#EXTINF`, `#EXT-X-MAP`,
   `#EXT-X-BYTERANGE`, `#EXT-X-MEDIA` audio groups). Relative URLs resolved
   against the playlist URL. Reject `#EXT-X-KEY` with a clear error.
2. `media/quality.rs`: turn variants into `Format`s, and build `pairs`
   (video variant + best audio group).
3. `src-tauri/src/video/probe.rs`: fetch a manifest through the existing
   `reqwest` client shaped by `transfer::Source::parse`, then call the parser.
   Caps: 2 MiB manifest, 5 redirects, 5000 segments, 12 h duration.
4. Bridge: `POST /v1/media` and `GET /v1/media/{job}`. Job state in `AppState`,
   in a new `Mutex<HashMap<u64, MediaJob>>` - not in `state.rs`'s download map.
5. Extension `lib/manifest.js`: recognise `m3u8` and `mpd` requests in the
   existing capture listener and report the manifest URL instead of refusing it.
   Keep `bdmIsStreamUrl` for links that are only a link.
6. Download: split the chosen variant into segments, hand each segment to the
   engine as a ranged part of one destination file (`segmenter::plan` already
   does contiguous parts), or download each segment to its own part file and
   concatenate. Pick the second: segments have their own URLs and their own
   `Content-Range` behaviour, and the first cannot resume a re-signed URL.

**Done when**

- A fixture HLS stream downloads to one playable `.ts` (V4 turns it into `.mp4`).
- Pause, kill the app, restart, resume: no segment is downloaded twice and the
  output is byte-identical to an uninterrupted run.
- A playlist with `#EXT-X-KEY` fails with one sentence and no partial file.
- `cargo test -p idm-core media::hls` covers: master, media, byte ranges, audio
  group, relative URLs, encrypted refusal, truncated playlist.

### V2 - DASH (3 days)

Tasks

1. `media/dash.rs`: parse `MPD` -> `Period` -> `AdaptationSet` -> `Representation`.
   Support `SegmentTemplate` (`$Number$`, `$Time$`, `$RepresentationID$`,
   `$Bandwidth$`), `SegmentList`, `SegmentBase` (single file + `sidx` index).
   `@presentationTimeOffset`, `startNumber`, `timescale`, `duration`.
2. `media/dash.rs`: read `contentType`, `mimeType`, `codecs`, `width`, `height`,
   `frameRate`, `bandwidth`, and `Role` (skip `trickmode` and `alternate` with
   no audio).
3. Quality + pairing as in V1, plus the `sidx` case where one file is the whole
   track (then it is just a normal ranged download).
4. Multi-period: v2 supports the first period and refuses the rest with a
   message. Multi-period is a V7 item.

**Done when**

- The fixture MPD downloads and concatenates, video and audio separately.
- `Representation` ids that are YouTube itags still work (they are just ids).
- `cargo test -p idm-core media::dash` covers: template with all placeholders,
  list, base+sidx, missing `SegmentTemplate` inheritance, bad XML.

### V3 - The quality list the user actually sees (2 days)

Tasks

1. `src-tauri/src/video/window.rs`: a small Tauri window like `grabber/window.rs`
   - square content, `round_corners`, no radius classes.
2. `src/components/windows/video/`: `video-panel.tsx`, `format-list.tsx`,
   `use-media-job.ts` (polling hook, same shape as `use-grabber.ts`).
3. Rows: one per `pair`, video-only formats shown with a note that they carry no
   sound, audio-only rows last. Use `components/ui/listbox.tsx`, never a native
   `<select>`.
4. Show size estimate when the manifest gives one (`bandwidth * duration`), else
   omit the column - never a guessed number.
5. Remember the last chosen quality per host in settings (`prefs.rs`), default
   `1080p` and fall back to the closest lower format.

**Done when**

- The panel opens from the overlay button, lists real formats, and the download
  starts with the chosen pair.
- Keyboard: arrows move, Enter starts, Escape closes. `verify_keyboard` and
  `verify_states` style gates pass on the panel.

### V4 - Remux (2 days)

Decision to take in the ADR, then implement only that one:

- **V4a (recommended): `ffmpeg` as a Tauri sidecar.** `-c copy`, `-movflags
  +faststart`, `-y` to a temp file, then `disk::replace_file` into place. One
  dependency, every container, every codec, zero maintenance. LGPL build only,
  no `--enable-gpl`.
- **V4b: native fMP4 writer.** Only if shipping `ffmpeg` is refused. It is what
  the reference product did (`ts2mp4` + `idmvconv`, doc 10 section 3 step 8),
  and it is weeks of work plus a codec matrix nobody asked for.

Container rule, matching the research:

| Video codec | Audio codec | Output |
|---|---|---|
| `avc1`, `hev1` | `mp4a`, `ac-3`, `ec-3` | `.mp4` |
| `vp9`, `av01` | `opus`, `vorbis` | `.mkv` |
| TS input (HLS) | AAC | `.mp4` |

Tasks

1. `src-tauri/src/video/mux.rs`: locate the sidecar, run it with a timeout, kill
   it on cancel, never leave a half file, report a readable error, log the exit
   code only.
2. Name the output from the page title, sanitised by the existing
   `grabber::policy` name rules; the container extension comes from the table.
3. Delete the two track temp files only after the mux succeeded and the file was
   replaced into place.

**Done when**

- The fixture pair muxes to one file that plays, in both containers.
- Cancel during mux leaves no partial output and no orphan temp files.

### V5 - Cookies and headers (1 day)

Tasks

1. Bridge: accept the optional `headers` map on `POST /v1/media`.
2. `video/policy.rs`: allow `Cookie`, `User-Agent`, `Referer`, `Origin`, and
   `Authorization` only for the manifest's origin. Anything else is dropped.
   A header value with a newline, or over 8 KiB, is dropped.
3. Never persist headers. Redact in the log the same way `transfer::redacted`
   does for URLs.
4. Ask once, per extension, with a plain sentence, before the first cookie is
   forwarded - the open-source comparison in doc 10 shows the reference product
   has to do the same, and it is the honest thing to do anyway.

**Done when**

- A fixture that requires a `Cookie` downloads; the same fixture without the
  cookie returns the site's own error, not a stack trace.
- No header value appears in `session.rs`'s index, in the log, or in an error.

### V6 - YouTube (3 days, and it will keep breaking)

Two paths, and the order matters:

1. **Native path (default):** the extension reads the player response from the
   page (`ytInitialPlayerResponse` or the `/youtubei/v1/player` answer the
   content script already sees), and posts `adaptiveFormats` +
   `progressiveFormats` as a candidate with `kind: "dash"` and the signed URLs
   already inside. The app then needs no YouTube knowledge: it has a format
   list with URLs and byte sizes, exactly like an MPD that was parsed for it.
   This is the reference product's DASH path (doc 10, step 5).
2. **Sidecar path (fallback):** if the player response is missing or the
   signature cannot be used, offer `yt-dlp` as an optional sidecar, exactly like
   `ffmpeg` in V4a. Same reason: it is maintained by people who do this daily.

Rules

- Never implement the signature cipher or the `n` throttle ourselves.
- A format whose URL fails with `403` is retried once with a fresh page
  response, then reported as "this site changed, update the app".
- Do not claim support for a site in the UI. Show what was found; a site that
  gives nothing shows nothing.

**Done when**

- 1080p YouTube download produces one `.mp4` with sound, on a fresh session,
  and `cargo test` still passes with no network.

### V7 - Refusals and hardening (1 day, plus a soak)

- DRM: `application/vnd.yt-ump`, `#EXT-X-KEY`, `ContentProtection` in an MPD,
  and `pssh` boxes all produce the same message: protected, cannot download.
- Live: `#EXT-X-ENDLIST` absent, or `MPD@type="dynamic"` -> refused with a
  sentence. Live recording is not in this plan.
- Multi-period DASH -> refused, first period offered as a note.
- Soak: 2 h of the fixture server with random 500s and disconnects, no leaked
  handles, no orphan part files (`M7` in the milestones doc asks for the same).

---

## 7. Quality naming rules

Mirror the reference product's vocabulary (doc 10, step 6) but do not copy its
typos and do not invent numbers:

- Resolution first: `2160p`, `1440p`, `1080p`, `720p`, `480p`, `360p`, `240p`,
  `144p`. `4K` may follow `2160p`, nothing else.
- Container/codec prefix shown only when it differs from the best available:
  `MP4 1080p`, `WEBM 1080p`.
- `60fps` only when the manifest states a frame rate at or above 50.
- `HDR` only when the manifest says so (`SupplementalProperty`,
  `transfer_characteristics` in the codec string).
- `high bitrate` only when two formats share resolution and frame rate and one
  has more than 1.5x the bandwidth of the other.
- Audio rows: `Audio 128 kbps`, `Audio 256 kbps`. No invented "HQ"/"HD".
- Never a number that was not in the manifest or measured from a download.

---

## 8. Security and policy (non-negotiable)

The Site Grabber's rules in `docs/SITE-GRABBER.md` are the house standard for
anything that fetches a user-supplied address. The media path follows them:

| Boundary | Rule |
|---|---|
| Scheme | `http` and `https` only for manifests and segments |
| Private networks | `grabber::policy::addr_is_public` on every hop, DNS re-checked |
| Redirects | at most 5, re-checked per hop |
| Size | 2 MiB per manifest, 5000 segments, 12 h duration, refuse beyond |
| Headers | origin-scoped, never persisted, never logged |
| Disk | free-space check through `disk::free_space` before the first segment |
| Names | `grabber::policy` sanitising, never overwrite without the conflict path |
| DRM | refused, always, with a message |

---

## 9. Tests and gates per phase

| Gate | Command | Applies |
|---|---|---|
| Engine tests | `cd crates/idm-core && cargo test` | V0-V7 |
| App tests | `cd src-tauri && cargo test` | V1-V7 |
| Frontend build | `pnpm build` | V3 |
| Palette | `pnpm check:palette` | V3 |
| Grabber helpers | `pnpm check:grabber` | unchanged, must stay green |
| Manual | one real site per phase, Firefox and Chrome, with the app closed and open | every phase |

A phase is not done because it compiles. It is done when the acceptance list is
ticked and someone watched the file play.

---

## 10. Risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | YouTube changes weekly | native path reads the page, sidecar fallback, no cipher code of ours |
| R2 | `ffmpeg` licensing and installer size | LGPL build, sidecar only, ADR records the choice |
| R3 | Cookie forwarding is a privacy surface | origin-scoped, opt-in, never stored, redacted |
| R4 | Segment URLs expire mid-download | re-fetch the manifest once on `403`/`410`, then fail cleanly |
| R5 | A 4K download is tens of GB of segments | size estimate before start, disk check, the existing per-download cap |
| R6 | Two tracks finish out of order | mux waits for both, then one atomic replace; a cancelled mux deletes both |
| R7 | Parsers are the new attack surface | size caps, no regex on untrusted XML, fixture tests for hostile input |

---

## 11. Do not

- Do not add a second download engine for media. Tracks are `Download`s.
- Do not parse playlists in JavaScript. One parser, in Rust, tested.
- Do not put a native `<select>` in the quality panel.
- Do not show a quality the manifest did not offer, or a size you did not get
  from a manifest or a measurement.
- Do not touch `#EXT-X-KEY`, `ContentProtection` or UMP except to refuse.
- Do not grow `App.tsx`, `lib.rs`, `bridge/mod.rs` or `downloader.rs`. New
  modules, as in section 4.
- Do not add a dependency before checking that the standard library or an
  installed crate does it.

---

## 12. Open questions for the owner

1. Ship `ffmpeg` as a sidecar (V4a) or build a native muxer (V4b)? The plan
   recommends V4a.
2. Is `yt-dlp` acceptable as a second optional sidecar (V6), or must YouTube
   stay native-only even if it breaks?
3. Should the quality panel be a separate window like the progress window, or a
   sheet inside the main window? The plan assumes a separate window.
4. Cookie forwarding: opt-in per browser (recommended) or a single app setting?

---

## 13. Handoff checklist

- [ ] ADR added under `planning/decisions/` recording the muxer choice
- [ ] `crates/idm-core/src/media/` exists with types, parsers, fixtures, tests
- [ ] `src-tauri/src/video/` exists with capture, probe, window, mux
- [ ] Bridge routes V1, V2, V3, V5 in `bridge/mod.rs` with tests like the
      existing ones in that file
- [ ] Frontend panel under `src/components/windows/video/`
- [ ] Both extensions capture manifests; `bdmIsStreamUrl` no longer a blanket no
- [ ] `docs/DEVELOPMENT.md` layout section updated
- [ ] `planning/README.md` current-state row for media splitting updated
- [ ] `06-milestones.md` M6 media box ticked per phase
- [ ] `CHANGELOG.md` entry
