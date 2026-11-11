# Queue scheduler

**Wave 6 · Status:** ❌ not written (the reference build has `app/src/queue.rs`)

## Goal

Downloads run overnight on a schedule: start at a time, stop at a time, limit
how many run at once, and optionally shut the computer down when the queue is
empty.

## Where it stands

The queue exists (`src-tauri/src/queue.rs`, Start queue / Stop queue, a maximum
of simultaneous downloads), but it is manual: no times, no days, no shutdown.

## Steps

1. A schedule is a small set of rules: days of the week, a start time, a stop
   time, and an action at the end (keep running, pause the queue, shut down).
2. Store it in the engine's file and evaluate it in the backend, so a browser
   hand-over to a queued download obeys it with no window open.
3. The rule must survive the app starting mid-window: on start, ask "is now
   inside a window" and start the queue if so.
4. Shutdown is an explicit, separate confirmation in the settings page, and the
   app pauses and saves every download before it asks the OS to power off.
5. Show the next window in the status bar: "queue runs at 01:00" is the whole
   feature for someone who forgets what they set.
6. Keep the default rule empty and off.

## Files

New `src-tauri/src/schedule.rs`, `src-tauri/src/queue.rs`,
`src-tauri/src/settings_store.rs`, `src-tauri/src/state.rs`,
`src/components/settings/downloads-settings.tsx`,
`src/components/workspace/status-bar.tsx`.

## Edge cases

- A window that spans midnight.
- The computer asleep at the start time: wake timers are a setting, and a missed
  window must not fire at noon.
- Changing the clock or the time zone mid-window.
- A shutdown with a download still running is refused, not forced.

## Tests

Unit: the rule evaluator at a fixed clock, including midnight and a missed
window. Integration: a queue with one small download, a window that starts
immediately, the download runs and the queue stops at the end time.

## Proof

`cargo test -p my-idm-clone`, then a real schedule a minute in the future in the
running app, watching the queue start and stop without a click.

## Out of scope

Per-download schedules, and any cloud scheduling.
