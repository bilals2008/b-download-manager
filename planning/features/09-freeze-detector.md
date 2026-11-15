# Freeze detector beside the crash monitor

> **Nov 2026:** implemented, including the freeze report and its next-start offer in Settings -> About. See REMAINING-FEATURES.md for the current proof.

**Wave 2 · Status:** ✅ shipped (`hang::ask` + one report per episode; a suspended machine is not a freeze)

Left: showing the frozen report on the next start, and a stack sample.

## Goal

A window that stops answering is a failure the user feels and the log never
records. Detect it, write a report with the main thread's stack, and say so the
next time the app starts.

## Where it stands

`crates/idm-crash` watches the process from outside and writes a minidump when
it ends badly. Nothing watches for a hung main thread, so the folder-freeze bugs
of the last weeks left no evidence.

## Steps

1. In the monitor, poll the main thread's responsiveness (a window message with
   a short timeout, or a heartbeat the UI thread writes) every second.
2. After a threshold of consecutive misses, capture a stack sample of the
   process without killing it (the monitor is a separate process for a reason).
3. Write a "frozen for N seconds" report with the stack into the same folder the
   crash reports use, once per episode.
4. On the next start, offer the last frozen report in the same place crash
   reports are offered, phrased for a reader: "the window stopped responding".
5. Mark the probe's own cost: the heartbeat write must be cheap enough to run
   in the release build.

## Files

`crates/idm-crash/src/monitor.rs`, `crates/idm-crash/src/lib.rs`,
`src-tauri/src/crash.rs`, `src-tauri/src/lib.rs` (start the watch),
`src/components/workspace/about-dialog.tsx` (where a report is offered today).

## Edge cases

- A debugger attached freezes the process on purpose; do not report then.
- A machine suspended and resumed looks like a long freeze; compare against
  wall clock change (sleep) and discard.
- The monitor must never itself block the app to take a sample.

## Tests

Unit: the episode rule (misses, threshold, one report per episode, reset after
recovery) on a fake clock. Integration: a test build that sleeps the main thread
for a set time and a check that exactly one report lands.

## Proof

`cargo test -p idm-crash`, then a debug build with an injected 5-second block,
and the report present and readable on the next start.

## Out of scope

Uploading reports anywhere; they stay on the machine.
