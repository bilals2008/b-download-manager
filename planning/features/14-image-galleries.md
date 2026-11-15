# Image galleries (gallery-dl)

> **Nov 2026:** implemented (page reader, dedupe, caps, folder named after the page). See REMAINING-FEATURES.md for the current proof.

**Wave 4 · Status:** ❌ not written

## Goal

Paste a gallery page — an image board, an artist's post — and the app downloads
every image in it at full speed, into a folder named after the page, with a
report of what it found.

## Where it stands

Nothing. `crates/tools` has no gallery module, the grabber walks HTML links and
would miss most galleries, and the engine has no notion of a set of files.

## Steps

1. Implement gallery extraction in this project rather than shelling out where
   possible: one module that takes a page's HTML and yields the image URLs and
   their names. The reference build uses gallery-dl as an external program;
   check per site whether HTML parsing is enough before adding that dependency.
2. A "Download gallery" entry in the add dialog when the pasted link looks like
   a gallery, with a count of images found before anything starts.
3. Start one download per image through the existing engine, grouped as a batch
   with one parent row (the grabber's hand-over already models a set), so the
   list shows one line and the report lives in it.
4. A naming policy per gallery: page title as the folder, index as the prefix,
   extension from the response.
5. A hard cap on images per gallery and a total size cap, both shown in the
   dialog.
6. Refuse a gallery that needs a session, unless the user's browser cookies are
   offered, as the video path already allows.

## Files

New `crates/idm-core/src/media/gallery.rs`, `src-tauri/src/gallery.rs`,
`src-tauri/src/lib.rs`, `src/components/workspace/add-download-dialog.tsx`,
`src/lib/ipc.ts`.

## Edge cases

- Infinite scroll: fetch the first page only, or ask for a page cap.
- Duplicate URLs across pages must download once.
- A gallery that is a single image is an ordinary download, not a batch.

## Tests

Unit: the extractor against saved fixtures, and the dedupe. Integration: one
local multi-page fixture served by the test server, every image written once.

## Proof

`cargo test -p idm-core`, then a real gallery in the app: one row, the right
number of files, the folder named after the page.

## Out of scope

Logins, paywalls, and any scraping that breaks a site's rules.
