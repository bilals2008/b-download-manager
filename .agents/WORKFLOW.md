# Working on this repository

This file is the workflow. It says how a change is planned, split, implemented
and proven here. Read it before starting anything that touches more than one
file.

## 1. Plan in waves, implement in parallel

A feature that spans the engine, the app and the window is three changes that
do not depend on each other. Write them down as separate, self-contained
pieces first, then hand each one to its own agent and let them run at the
same time.

Rules that keep the parallel work from colliding:

- One piece, one set of files. Two agents never edit the same file. When two
  pieces need the same file, they need the same *module*, so the module is
  split first (see §3) and each agent gets its own module.
- Every piece lands on its own branch or as its own commit. One commit per
  piece, never a chain of "layer" commits over the same feature.
- A piece that needs a file another agent owns waits. It does not edit around
  it.

## 2. Use the skills

The skills in `.agents/skills` and the ones installed with the agent are the
house rules for the parts of this work that are not Rust or TypeScript:

- `openscreen-demo` — a demo video of a feature for the pull request.
- The UI skills (design system, accessibility, layout, motion, typography)
  for anything that draws.
- The platform skills (Tauri, Rust, React) for anything that runs on the
  platform.

Load a skill before you write the code it covers. A skill that exists and is
not used is a rule that was paid for and not spent.

## 3. No file grows past the limit

The limit is **800 lines** for a source file. `scripts/check-file-size.mjs`
fails the build when a file is over it.

A file over the limit is split before the change, not after it:

- One responsibility per file. A file that holds two things is split into
  two files, and the split follows the names the code already uses.
- The split moves code, it does not rewrite it. Behaviour must be identical
  before and after, so the test suite is the proof.
- Tests live next to the thing they test: `foo.rs` and `foo/tests.rs`, or
  `foo/mod.rs` and `foo/bar.rs` when the module grows past a file.

When a feature touches a file that is already over the limit, the feature
splits it as part of the same change.

## 4. Prove it on the real application

The unit tests and the check scripts are the floor, not the proof. Follow the
build discipline in `AGENTS.md`: targeted commands while iterating, the full
suite once at the end, no concurrent cargo builds, no needless release builds.

- `cargo test --workspace` — the engine and the app.
- `pnpm test` — the window.
- `pnpm check:all` — every self-check, including the file-size guard.
- `pnpm tauri build` — the real bundle, not `cargo check`.
- **Run the built application and use it.** Start it, drive it through the
  loopback bridge (`scripts/bridge-probe.mjs`), and watch what it does.

The bridge probe is the honest test:

```bash
pnpm tauri build --no-bundle      # or run the app from target/release
pnpm probe:bridge                  # a real download through the real app
```

A bug that only shows up in the running application — a frozen window, a
progress bar that does not move, a file that is not written — is not fixed
until the probe says so.

## 5. What is checked before a commit

1. `pnpm verify` — check:all, the type check and the unit tests.
2. `cargo test --workspace`.
3. `pnpm tauri build` — the bundle still builds.
4. The probe against the running app, for anything that touches the engine,
   the bridge or the window.

## 6. The waves

`planning/REMAINING-FEATURES.md` is the list: what is done, what the engine
already has but nothing can reach, and what is still missing, wave by wave.

The work ahead is large, so it is cut into waves. A wave is one set of pieces
that do not touch each other's files; the pieces inside a wave run in parallel.
The split list is the allowlist in `scripts/check-file-size.mjs`: an entry is
removed when that file has been split, not when the wave starts.

- **Wave 0 — the small windows work.** `progress` and `complete` must stay
  listed by exact label in `src-tauri/capabilities/default.json`. A window
  missing there still opens (app commands are not gated) but cannot be dragged,
  cannot be closed, and never hears `download://update` — it reads as a freeze.
- **Wave 1 — splits only.** Move code, never rewrite it: `lib.rs` → commands /
  setup, `state.rs` → downloads / settings / queue, `actions.rs` → one module
  per action family, `downloader.rs` → probe / ranges / assembly / resume,
  `download-store.tsx` → store / selectors / effects, `App.tsx` → one hook per
  concern. `cargo test --workspace` and `pnpm test` are the proof of a split.
- **Wave 2 — engine reliability.** Checksum verification of a finished file
  (paste a hash, `sha2`), per-host learned connection caps, slow-connection
  shedding and rebalancing, server location (country + RTT) per download.
- **Wave 3 — the wire.** DoH/DoT (hickory via `reqwest::dns_resolver`, with
  reqwest 0.13's `DohResolver`/`DotResolver` when the version allows it),
  adapter binding with `IP_UNICAST_IF`/`IPV6_UNICAST_IF` plus
  `NotifyIpInterfaceChange` for the kill switch, and PAC through WinHTTP's
  `WinHttpGetProxyForUrl` (per URL, never a cached global route).
- **Wave 4 — media.** Soft subtitles into MP4 (`mov_text`) and MKV
  (`S_TEXT/UTF8`), audio extraction to mp3/opus/flac, image galleries,
  Google Drive/Docs, and DRM *detection* (refuse, never touch). All of it goes
  through a pinned FFmpeg fetched and checksum-verified like the yt-dlp helper;
  no transcode of video.
- **Wave 5 — the outside world.** Debrid services (Real-Debrid, AllDebrid,
  Premiumize, TorBox: API key in the OS credential store, never in
  `settings.json`, never in a log), and archive handling.

A wave is done when `pnpm verify`, `cargo test --workspace`, `pnpm tauri build`
and the probe against the running app all pass, and `WHATS_NEW.md` has the
reader-facing line for it.
