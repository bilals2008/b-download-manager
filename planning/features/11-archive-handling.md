# Archive handling, the rest

**Wave 5 · Status:** 🔧 first half shipped (row menu → Extract here)

## Goal

An archive unpacks itself when it finishes if the user asked for that, the
archive goes only after a verified extract, and an encrypted one asks for its
password instead of failing.

## Where it stands

`idm-archive` is a dependency and `src-tauri/src/extract.rs` unpacks a finished
download into a folder of its own name, through the crate's traversal-safe
policy. Missing: the setting, the post-extract cleanup, the password path, and
any progress while a large archive unpacks.

## Steps

1. Add two settings to the engine's file: extract when done (off), and keep the
   archive after extracting (on).
2. Hook the completion report in `actions.rs`: when the setting is on and the
   name is an archive, run the same `unpack` path off the main thread, so a
   browser hand-over behaves like a dialog download.
3. Delete the archive only after the report says nothing was refused; a refused
   entry means the extract is incomplete and the archive must stay.
4. On `NeedsPassword`, hold the job and ask in the completion window for the
   password; retry once, and never store the password.
5. Report progress for large archives: the crate needs an entry counter
   callback (or a progress closure in `Policy`) before the UI can draw a bar.
6. Show the outcome: files written, bytes, skipped entries, refused entries.

## Files

`crates/idm-archive/src/lib.rs` (a progress hook in `Policy`),
`src-tauri/src/extract.rs`, `src-tauri/src/actions.rs`,
`src-tauri/src/settings_store.rs`, `src-tauri/src/state.rs`,
`src/components/settings/downloads-settings.tsx`,
`src/components/windows/download-complete-window.tsx`.

## Edge cases

- A split archive (`.001`, `.part1.rar`) is unpacked from its first volume only.
- Extracting the same archive twice must not overwrite the first folder; the
  crate renames, and the report's `renamed` count is shown.
- A disk that fills mid-extract must leave the staging folder removed and say
  which file failed.

## Tests

Unit: the "delete the archive only if complete" rule and the settings reader.
Integration: extract a real zip into a temp folder; a zip with a `..` entry is
refused and the archive left in place.

## Proof

`cargo test -p idm-archive -p my-idm-clone`, then a real download of a zip in the
app with the setting on, and the folder present with the archive gone.

## Out of scope

Creating archives; this app unpacks only.
