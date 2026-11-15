# BitTorrent support

> **Nov 2026:** implemented (magnet links, .torrent files, window with progress, peers and pieces; seeding off by default). See REMAINING-FEATURES.md for the current proof.

**Wave 6 · Status:** ❌ not written (the reference build has `crates/torrent`)

## Goal

Magnet links and `.torrent` files download, in order or not, with the files
chosen, and can seed afterwards if the user wants that.

## Where it stands

Nothing. The engine speaks HTTP and FTP only. There is no peer protocol, no
tracker client, no DHT, and the list model assumes one file per row.

## Steps

1. Decide the peer engine before writing anything: the reference build uses
   `librqbit`, which is a large dependency with its own tokio model. Either
   adopt it as a separate crate under `crates/` or decide not to support
   torrents — this plan assumes adoption.
2. A magnet or `.torrent` link is a job, not a download: it holds the metadata,
   the file list, the piece map and the peers.
3. The add dialog gains a file picker for what to download, and the progress
   window gains tabs for files, peers, trackers and pieces.
4. Sequential order by default for a single-file download, as the reference app
   does, so a film can be played while it finishes.
5. Storage stays in the app's staging tree until the job completes, then moves
   like a download does; the existing policy of not writing parts into the
   user's folder applies unchanged.
6. Seeding is a setting, off by default, with a clear note about upload traffic.

## Files

New crate `crates/idm-torrent`, `src-tauri/src/torrent.rs`,
`src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`,
`src/components/windows/progress/*`, `src/components/workspace/download-row.tsx`.

## Edge cases

- A magnet that never finds peers: a visible "no peers yet" state, not a hang.
- A torrent whose name escapes the folder: the same guard as archives.
- The user removes the job while it seeds: ask, because removing stops uploads.

## Tests

Unit: the magnet/metainfo parser and the file-choice model. Integration: a local
swarm example (the reference has one) seeding a small file, the job completes
and the bytes match.

## Proof

`cargo test -p idm-torrent`, then a real magnet in the app against a local
swarm, the file present and its hash correct.

## Out of scope

A built-in tracker, web seeds beyond what librqbit does, and any private-site
authentication.
