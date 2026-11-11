# Google Drive and Docs links

**Wave 4 · Status:** ❌ not written

## Goal

A shared Drive file downloads straight from the app and is named correctly,
instead of saving an HTML page; a Docs, Sheets or Slides link is offered as an
export (PDF, DOCX) rather than a broken download.

## Where it stands

Nothing specific. A `drive.google.com` link is treated as an ordinary URL, so
the big-file confirmation page, the virus-scan warning page, and the export
paths all arrive as downloads of HTML.

## Steps

1. A site rule module in the engine that recognises the Drive link shapes
   (`/file/d/<id>/view`, `open?id=`, `uc?id=`, folder links) and the Docs
   editors (`/document/d/`, `/spreadsheets/d/`, `/presentation/d/`).
2. Resolve a Drive file to its download endpoint; for files over the confirm
   threshold, follow the confirmation POST the way a browser does, keeping the
   cookie the page set.
3. For an editor link, offer the export formats the document supports and use
   the export endpoint; the name comes from the document title in the response.
4. For a folder, list its files and hand them to the gallery batch (the plan
   that owns sets of files), not to one download.
5. Say when a file is private or needs a sign-in: never fall through to
   downloading the error page.

## Files

New `crates/idm-core/src/sites/gdrive.rs` (the reference keeps it under
`engine/src/sites/`), `crates/idm-core/src/transfer.rs` (the rule hook),
`src-tauri/src/actions.rs` (the start), `src/components/workspace/add-download-dialog.tsx`
(the export choice).

## Edge cases

- Drive changes its confirm page; the parser must fail with a sentence, not a
  wrong file.
- A file that is a Google Doc is never downloadable as a binary; always an
  export.
- Quota-exceeded responses must read as "the file's owner is over quota".

## Tests

Unit: every link shape to its resolved endpoint, and the export table per
document type. Integration: the local test server imitating each response shape.

## Proof

`cargo test -p idm-core`, then a real public Drive file in the app, downloaded
and named as the file, with the hash matching the one Drive shows.

## Out of scope

Signing in with a Google account; only links shared publicly or to anyone with
the link.
