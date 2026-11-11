# Connection rebalancing until the last byte

**Wave 2 · Status:** ✅ shipped (`pace::lagging` takes the free slot; a hand-over stops the range at the boundary instead of splitting it mid-write)

## Goal

Keep the connections busy to the end. When one finishes its range well before
the others, the remaining work should be split so every connection has
something to do until the file is complete, instead of one connection fetching
the tail alone.

## Where it stands

Ranges are planned once at the start. The engine re-plans only when a range
fails, not when a connection finishes early. `segmenter.rs` already knows how to
make part files and ranges, which is the mechanism rebalancing needs.

## Steps

1. When a connection finishes its range with substantial work left on other
   ranges, offer it the back half of the range with the most bytes left.
2. Do the split on the byte map, not on the wire: the receiving side of the
   split range stops at the new boundary it is told, and the free connection
   starts at that boundary.
3. Bound the number of rebalances per run (and per range) so a fast connection
   does not shred the plan into thousands of tiny ranges.
4. Keep the report coherent: the chunk list the progress window draws must
   follow the plan, not the plan of a second ago.
5. Never rebalance under the speed limiter's floor or while a shed is in flight.

## Files

`crates/idm-core/src/downloader.rs`, `crates/idm-core/src/segmenter.rs`,
`crates/idm-core/src/downloader/tests.rs`, `crates/idm-core/src/progress.rs`
(the chunk model the meter reads).

## Edge cases

- A server that ignores the end of the range and keeps sending: the receiving
  connection must stop reading and close, and the new connection must not
  double-write.
- Resume state written mid-rebalance must describe the bytes on disk, not the
  plan.
- Zero-byte and one-byte files have nothing to rebalance.

## Tests

Unit: a split of a range, the boundary arithmetic, the bound on splits.
Integration: three ranges, one artificially fast connection, the last byte still
written once.

## Proof

`cargo test -p idm-core`, then a real download with one connection throttled and
the file's hash checked against the publisher's.

## Out of scope

Changing how many connections are opened; that is the per-host cap's job.
