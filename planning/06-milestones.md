# Milestones

Each milestone has a definition of done that can be checked, not a feeling.
Nothing here is "polish the UI" — that is not a milestone.

Status reflects the work done so far.

---

## M0 — the core engine ✅ done

**Done when** a single URL downloads in parallel, correctly, and can be
resumed.

- [x] Probe the server for size and range support
- [x] Split the file into contiguous ranges, one part file each
- [x] Fetch each range with `Range` + `Accept-Encoding: identity`
- [x] Validate the `Content-Range` reply
- [x] Write at absolute offsets, stop cleanly at any byte
- [x] Join the parts and rename onto the destination
- [x] Persist a manifest and resume from it
- [x] Pause, resume and cancel
- [x] Tests: range request lands at the right offset; a server that ignores
      the range is rejected; segment planning covers the file without gaps

**Where:** `crates/idm-core/src/segmenter.rs`, `crates/idm-core/src/downloader.rs`
(the engine left the Tauri app in M2).

---

## M1 — the shell ✅ done

- [x] Tauri v2 app, Rust backend, React + TypeScript front end
- [x] Tailwind CSS v4 with a single token source
- [x] shadcn/ui primitives: button, progress, table, input, card, badge
- [x] IPC in both directions: commands in, `download://update` events out
- [x] A live table of downloads with per-row progress and actions
- [x] A token palette with a contrast gate; at v1.0.0 it is nine appearances,
      and `pnpm check:palette` prints 70 pairs passing in all of them

---

## M2 — a real crate boundary ✅ done

**Why:** the engine cannot be tested without a window until it is its own
crate.

- [x] Move the engine to `crates/idm-core` with no Tauri dependency
- [x] `crates/idm-cli` — a headless client, used by the test suite
- [x] The Tauri app becomes a thin shell over the crate
- [x] The existing tests move with the code and still pass

**Done when:** `cargo test -p idm-core` runs the engine suite with no
display, and the CLI can download a URL to a path.

---

## M3 — correctness under failure ✅ done

**Why:** the current engine is optimistic. It detects a server that ignores
ranges, and little else.

- [x] Chunk-head verification against the file head
- [x] Chunk order verification, with rollback
- [x] Downgrade to one connection on a data mismatch, and tell the user
- [x] Exponential backoff with a cap, instead of `400ms × attempt`
- [x] Honour `Retry-After` on `429`
- [x] Disk space check before starting, and during — on Windows; on macOS and
      Linux `free_space()` returns `None` and the check is skipped
      (`src-tauri/src/lib.rs`)
- [x] A failure mid-assembly must not leave a half-written destination
- [x] Unknown total size (`Content-Range: bytes 0-0/*`)

**Done when:** a test server that serves deliberately wrong bytes produces a
failed download with a clear message, and never a corrupt file.

---

## M4 — scheduling and speed ◐ partly

**Why:** throughput is the product.

- [x] Measure the actual split rule: the rule the reference product uses is
      one connection per `0x2800` bytes (`FUN_005bad00`), and
      `bench_connections` measures what a server really gives
- [x] Fix `maybe_grow()` to compare progress *ratios*, not remaining bytes
- [x] A global connection limit across all downloads
- [x] Per-site connection overrides in the engine (`SiteLimits`,
      `downloader.rs:1904`) — the commands exist; no screen uses them yet
- [x] Speed limiting, per download and globally
- [x] Accurate ETA and average speed — the user interface derives them from
      the byte counters the engine reports
- [x] A queue with priorities — `src-tauri/src/queue.rs` starts queued
      downloads in priority order up to the simultaneous limit
- [ ] Scheduled start times, and the queue moved inside the engine
- [ ] The toolbar Speed Limiter menu (`Turn on` / `Settings…`) is still a
      disabled stub in `src/components/shell/menus.ts`, even though both caps
      work elsewhere: the global one in Settings, the per-download one in the
      progress window's Speed limit tab

**Done when:** on the same machine and the same server, the multi-connection
download is measurably faster than a single-connection client, and the
number is written down in the README. **Not met as of v1.0.0**: neither
`README.md` nor `docs/` holds a throughput table, and the two benchmark tests
(`bench_local_segmented`, `bench_connections`) are `#[ignore]`d, so the project
must not claim a speed figure until one is measured.

---

## M5 — the product around the engine ✅ done, with gaps

Checked against the source on 6 November 2026. This milestone was written when
none of it existed, and the plan's own "out of scope" note for browser
integration was overtaken by events.

- [x] The application window: title bar, menu bar, toolbar packs, categories
      pane, download list
- [x] A tray icon with pause-all, resume-all, show and quit
      (`src-tauri/src/tray.rs`)
- [x] A download list that sorts by name, size, speed, ETA and status
      (`download-columns.ts`) and searches through the find bar
- [x] Pause-all / resume-all (menu, tray and rebindable shortcuts)
- [x] The browser integration: Firefox and Chrome/Edge extensions, a paired
      loopback bridge, referer hand-off, batch hand-over
- [x] A first-run sentence in the add dialog (`firstRunDone`)
- [ ] OS-level notifications. The completion window is in-app
      (`src-tauri/src/complete.rs`, `showCompleteDialog`); there is no
      `tauri-plugin-notification` in `src-tauri/Cargo.toml`, so a download
      that finishes while the window is hidden announces nothing.
- [ ] Drag and drop of a URL or file onto the window: no drop handler exists
      and `drop-target` is a disabled menu item
- [x] A per-download speed cap, editable in the progress window
      (`speed-limit-tab.tsx` → `setDownloadSpeedLimit`)
- [ ] Per-download connections after the download starts: the count is chosen
      at add time and only displayed afterwards
      (`download-properties-dialog.tsx:379`)
- [ ] A per-download start time: nothing in the engine or the UI stores one
      (`session.rs`, `actions.rs` have no start-time field)
- [ ] A tray-only mode: start with no window and open one from the icon
      (`minimize_to_tray` exists; starting hidden does not)
- [ ] Export and import, and the batch add dialog — all disabled stubs in
      `src/components/shell/menus.ts`

**Done when:** a person who has never seen the reference product can start a
download, pause it, and find it again tomorrow. Met for the core flow
(resume after a restart is tested); not met for the stubs above.

---

## M6 — protocols beyond HTTP ◐ partly

Each is independently valuable and independently shippable.

- [x] FTP, using the same chunk list (the reference product's
      `FTPConnection::StartGetChunk`)
- [x] HTTP proxy and SOCKS5
- [x] Media: HLS, MPEG-DASH, and plain-file tracks. Each segment is fetched by
      the same engine as an ordinary file, a segment already on disk is not
      fetched again, and the finished track is remuxed natively — no
      `ffmpeg -c copy`, no `yt-dlp` (see [11](./11-media-download-plan.md),
      [14](./14-own-youtube-and-native-muxer-plan.md), `media/download.rs`,
      `src-tauri/src/video/mp4.rs`)
- [ ] Resume metadata for servers that do not support ranges (the product's
      "multipart/x-byteranges" and friends)
- [ ] FTPS (implicit TLS) and FTP through the proxy
- [ ] A proxy bypass list; today every request takes the configured proxy
- [ ] RTMP, if it is ever wanted

---

## M7 — hardening ◐ partly

- [x] A diagnostics log of what the downloads did (`crates/idm-core/src/log.rs`)
- [x] Graceful shutdown that saves the manifest on exit (`lifecycle.rs`)
- [x] CI on every push: verify on Linux and Windows, clippy, the contrast gate,
      and a signed production bundle on `main` (`.github/workflows/ci.yml`)
- [x] Release builds for Windows (`nsis`), Ubuntu 22.04 (`deb`, `appimage`) and
      macOS (universal `app` + `dmg`), signed for the in-app updater
      (`.github/workflows/release.yml`)
- [ ] **Cross-OS correctness.** `free_space()` is implemented for Windows and
      returns `None` elsewhere (`src-tauri/src/lib.rs`), so on macOS and Linux
      a download starts with no room check; the staging folder is hidden only
      through the Windows file attribute (`staging.rs`).
- [ ] A fuzzed manifest reader
- [ ] A soak test: 24 hours, random disconnects, no leaked handles
- [ ] **A flaky engine test.** `downloader::tests::pausing_an_unknown_size_download_does_not_finish_it_short`
      failed inside the full-suite run on 6 November 2026 and passed twice in
      isolation — a timing assumption, not a data race in the shipped code.
      Fix the assertion before it is trusted as a gate.

---

## Order and dependencies

```
M0 ── M1 ── M2 ── M3 ── M4 ── M5 ── M7
                 │               │
                 └──── M6 ──────┘
```

M2 before M3, because the failure tests need a headless harness.
M3 before M4, because scheduling decisions need a trustworthy engine.
M6 can run in parallel with M5 once M2 lands.
