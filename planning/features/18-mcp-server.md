# MCP server for AI assistants

> **Nov 2026:** implemented (separate stdio server, routes in the bridge, advanced setting, shipped beside the app). See REMAINING-FEATURES.md for the current proof.

**Wave 6 · Status:** ❌ not written (the reference build has `crates/mcp`)

## Goal

An assistant on this machine can list, add, pause, resume and remove downloads
through the Model Context Protocol, with the same rules the window obeys.

## Where it stands

Nothing. The app's only programmatic door is the loopback bridge for the browser
extension, which is scoped to extension origins and a fixed API.

## Steps

1. A separate small program (`crates/idm-mcp`) that speaks MCP over stdio, as
   the reference does, so an assistant starts it and owns its lifetime.
2. It talks to the running app, not to the engine directly: reuse the bridge's
   local API or add one authenticated local route, never a second copy of the
   download logic.
3. Tools with reader-level names: list downloads, add a link (URL, folder,
   connections), pause, resume, remove, and check the queue.
4. No tool that deletes a file from disk without a second, explicit call.
5. A setting to turn the server's door on and off; off by default.
6. Build it beside the app binary and ship it in the same installer.

## Files

New crate `crates/idm-mcp`, `src-tauri/src/bridge/mod.rs` (the route it uses),
`src-tauri/src/lib.rs` (what it may call), `src/components/settings/advanced-settings.tsx`,
the installer configuration.

## Edge cases

- The app is not running: the server says so plainly instead of starting a
  second instance that would fight for the index.
- Two assistants at once: the app serialises the commands, as it does for two
  windows.
- A tool call that would delete data needs its own confirmation step.

## Tests

Unit: the tool schema and the argument validation. Integration: an MCP client
against a running app adding one download and reading it back.

## Proof

`cargo test -p idm-mcp`, then a real assistant session against the running app
adding a download that appears in the window and finishes.

## Out of scope

A network-facing MCP server; this is local only.
