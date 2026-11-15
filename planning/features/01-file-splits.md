# Splitting the files over the size limit

> **Nov 2026:** not pursued this pass (three files remain over the limit). See REMAINING-FEATURES.md for the current proof.

**Wave 1 · Status:** 🔧 22 files over 800 lines

## Goal

Every source file under 800 lines, without one byte of behaviour changing, so
the later features can be built in parallel without two pieces editing one file.

## Where it stands

The full list is the allowlist in `scripts/check-file-size.mjs`. The worst:
`downloader/tests.rs` 2845, `downloader.rs` 2272, `video/mp4.rs` 1805,
`video/verify.rs` 1707, `state/download-store.tsx` 1640, `lib.rs` 1515,
`state.rs` 1513, `net/pac/js.rs` 1498, `media/download.rs` 1420,
`video/download.rs` 1400, `video/webm.rs` 1362, `media/dash.rs` 1356,
`grabber/crawl.rs` 1293, `actions.rs` 1257, `video/yt.rs` 1245.

## Steps

1. Split `src-tauri/src/lib.rs` first: commands grouped by family
   (`commands/downloads.rs`, `commands/settings.rs`, `commands/system.rs`),
   the `setup` closure into `setup.rs`, the handler list staying in `lib.rs`.
2. `state.rs`: the downloads, the settings, the queue and the limits each in
   their own module under `state/`, re-exported from `state.rs`.
3. `actions.rs`: one module per family (start, pause/resume, cancel, delete,
   queue, metadata), with `actions.rs` keeping the shared reporter.
4. `downloader.rs`: probe, planning, ranges, assembly, resume.
5. `download-store.tsx`: store, selectors, effects; the provider stays.
6. `video/*` and `media/*` files: codecs and containers stay one file per
   format; the orchestration moves to a coordinator module.
7. Remove each entry from the allowlist in the same commit that splits it.

## Rules

- Move code; never rewrite it. A split that changes behaviour is a rewrite.
- Tests move next to their module (`foo.rs` + `foo/tests.rs`).
- One commit per split; a wave of splits is not one commit.
- Stop the wave when `cargo test --workspace` and `pnpm test` pass unchanged.

## Proof

`cargo test --workspace`, `pnpm verify`, `pnpm check:file-size` reports zero
files over the limit, then launch the built app and drive one download.

## Out of scope

Any new behaviour. A split that needs one is a separate piece, done after.
