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

**Where:** `src-tauri/src/segmenter.rs`, `src-tauri/src/downloader.rs`.

---

## M1 — the shell ✅ done

- [x] Tauri v2 app, Rust backend, React + TypeScript front end
- [x] Tailwind CSS v4 with a single token source
- [x] shadcn/ui primitives: button, progress, table, input, card, badge
- [x] IPC in both directions: commands in, `download://update` events out
- [x] A live table of downloads with per-row progress and actions
- [x] A soft warm palette with a contrast gate

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
- [x] Disk space check before starting, and during
- [x] A failure mid-assembly must not leave a half-written destination
- [x] Unknown total size (`Content-Range: bytes 0-0/*`)

**Done when:** a test server that serves deliberately wrong bytes produces a
failed download with a clear message, and never a corrupt file.

---

## M4 — scheduling and speed ◐ partly

**Why:** throughput is the product.

- [x] Measure the actual split rule: the rule IDM uses is one connection
      per `0x2800` bytes (`FUN_005bad00`), and `bench_connections` measures
      what a server really gives
- [x] Fix `maybe_grow()` to compare progress *ratios*, not remaining bytes
- [x] A global connection limit across all downloads
- [x] Per-site connection overrides
- [x] Speed limiting, per download and globally
- [x] Accurate ETA and average speed — the user interface derives them from
      the byte counters the engine reports
- [ ] A queue with priorities and scheduled start times

**Done when:** on the same machine and the same server, the multi-connection
download is measurably faster than a single-connection client, and the
number is written down in the README.

---

## M5 — the product around the engine ⬜

- [ ] A real application window: menu, tray icon, notifications
- [ ] Drag and drop a URL or a file onto the window
- [ ] A download list that can be sorted and searched properly
- [ ] Per-download settings: connections, speed limit, start time
- [ ] A pause-all / resume-all
- [ ] The browser integration, if it is ever wanted (out of scope until
      then)
- [ ] A first-run flow that explains the connection setting in one sentence

**Done when:** a person who has never seen IDM can start a download, pause
it, and find it again tomorrow.

---

## M6 — protocols beyond HTTP ◐ partly

Each is independently valuable and independently shippable.

- [x] FTP, using the same chunk list (IDM's `FTPConnection::StartGetChunk`)
- [x] HTTP proxy and SOCKS5
- [ ] Resume metadata for servers that do not support ranges (IDM's
      "multipart/x-byteranges" and friends)
- [ ] Media: HLS, MPEG-DASH, and the MPEG-TS segmenting IDM does
- [ ] RTMP, if it is ever wanted

---

## M7 — hardening ◐ partly

- [x] A diagnostics log of what the downloads did
- [x] Graceful shutdown that saves the manifest on exit
- [ ] A fuzzed manifest reader
- [ ] A soak test: 24 hours, random disconnects, no leaked handles
- [ ] Windows and Linux builds from CI
- [ ] A signed installer

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
