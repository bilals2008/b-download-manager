# Slow-connection shedding

**Wave 2 · Status:** ✅ shipped (`pace::stalled`, four hand-overs per run at most)

## Goal

When one range is far behind the others, stop waiting on it and give the bytes
it has not fetched to a connection that is actually moving. A single dead
connection must not hold the last megabyte of a file for a minute.

## Where it stands

`segmenter.rs` plans the ranges once and they stay as planned. A connection that
stalls is retried by `downloader.rs`, but its remaining bytes are never handed
to a faster connection. `SegmentInfo` already reports per-connection progress,
so the signal exists.

## Steps

1. Define "behind": a range whose rate over the last window is below a fraction
   of the median rate of the other ranges, and that has more than a minimum
   number of bytes left.
2. Stop that connection cleanly (the range bookkeeping must survive; the part
   file stays).
3. Re-queue its remaining bytes as one more range for the scheduler, which the
   existing planner already understands.
4. Never shed the last connection, and never shed when only two are left: the
   median of two says nothing.
5. Count the sheds and show them in the connection details, so a slow run is
   explained rather than blamed on the server.

## Files

`crates/idm-core/src/downloader.rs`, `crates/idm-core/src/segmenter.rs`,
`crates/idm-core/src/downloader/tests.rs`,
`src/components/windows/progress/connection-details.tsx`.

## Edge cases

- A server with a per-connection bandwidth cap reproduces "one is slow" every
  time; the shed must not turn into a shed loop. A host that lost a range this
  way must not be shed again in the same run.
- HTTPS upload progress and the speed limiter must see the shed as a stop, not
  as a failure.
- Resume after a shed must not lose the bytes already on disk.

## Tests

Unit, on a fake clock and fake rates: a stalled range is shed, a uniformly slow
run is not, and the shed bytes are re-planned exactly once. Integration against
the lab server with one throttled connection.

## Proof

`cargo test -p idm-core`, then a real download from a server that throttles one
connection, watching the run still finish at full speed.

## Out of scope

Reworking the retry policy; a shed is not a retry.
