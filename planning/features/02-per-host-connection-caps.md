# Per-host learned connection caps

**Wave 2 · Status:** ✅ shipped (`LearnedCaps`, `host-caps.json`, shown in the window)

Left: a reset button in the properties dialog; the engine can already forget a host.

## Goal

Remember what each server actually allowed the last time, and start the next
download from that host at that cap instead of the user's global count. A host
that answered two ranges and rejected twelve should not be asked twelve times
every download.

## Where it stands

`crates/idm-core/src/limits.rs` holds the global connection budget and per-site
caps the user set. Nothing learns from what a server did. `downloader.rs` plans
the ranges from the requested count; the failure path already notices a server
that refuses ranges and falls back to one connection, but that knowledge is
thrown away with the run.

## Steps

1. Define what a host is: the registrable host of the URL (the same rule
   `limits.rs` already uses for a site cap).
2. Record an outcome per host after a run: the ranges asked for, the ranges the
   server honoured, the ranges that failed on the first byte, and when it was
   last seen.
3. Cap the next start at the largest count that worked, never above the user's
   setting, never below one.
4. Persist it in the engine's own file (the settings store), not in
   `localStorage`: a browser hand-over has no window and needs it too.
5. Show the cap in the progress window's connection details, so a cap the user
   did not set is explained rather than mysterious.
6. Forget a host's history after a fixed quiet period, so a server that was
   upgraded is not capped forever.

## Files

`crates/idm-core/src/limits.rs`, `crates/idm-core/src/downloader.rs`,
`src-tauri/src/settings_store.rs`, `src-tauri/src/state.rs`,
`src/components/windows/progress/connection-details.tsx`.

## Edge cases

- A redirected download belongs to the host it finally talks to, not the URL
  the user pasted.
- Two downloads from one host at the same time must share the knowledge
  without racing on the write.
- A cap learned from a flaky network must not outlive the network; the quiet
  period and a manual "reset this host" in the properties dialog cover it.

## Tests

Unit: the rule that turns (asked, honoured, failed) into the next cap, including
the one-connection floor and the user's ceiling. Integration: a test server that
honours only two ranges, two runs, the second starting at two.

## Proof

`cargo test -p idm-core`, then the real app against the lab server, watching the
second download ask for the learned count in the connection details.

## Out of scope

Per-download overrides; the user's own cap stays the ceiling.
