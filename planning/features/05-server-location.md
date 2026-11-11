# Server location: country and RTT

**Wave 2 · Status:** ✅ shipped (country code and RTT in the status tab; no probe of a loopback or local host)

## Goal

Every download says where its bytes come from: the country of the server it
talks to, and how long a round trip takes. "This computer" and "the local
network" are answers too.

## Where it stands

`crates/idm-core/src/geo.rs` with the bundled table
`geo/ipcountry.bin.xz` (674 KB) and `geo/countries.rs` answered distance and
country lookups already. `DownloadInfo` has no field for either, so the window
cannot show them, and the transfer layer does not record the peer address.

## Steps

1. Have the transfer report the peer IP of the connection it opened (the first
   successful one is enough) into the download's state.
2. Resolve that IP to a country in the engine, off the UI thread; cache the
   answer per IP in the run.
3. Measure a round trip: reuse the connect time already known, or a single
   small request; do not add a second connection to the server.
4. Add `country` and `rttMs` to the serialised `DownloadInfo` and to
   `src/types.ts`, both optional, so an old index still reads.
5. Show the flag and the RTT in the progress window's status tab and as a
   column in the download list (off by default).
6. Keep the flag drawing out of the engine: the engine gives a two-letter code.

## Files

`crates/idm-core/src/downloader.rs`, `crates/idm-core/src/geo.rs`,
`crates/idm-core/src/transfer.rs`, `src-tauri/src/lib.rs` (nothing new to
dispatch beyond the fields), `src/types.ts`,
`src/components/windows/progress/download-status-tab.tsx`,
`src/components/workspace/download-columns.ts`.

## Edge cases

- A private or loopback address has no country; say "this computer" or "local
  network" rather than a flag.
- IPv6 must go through the same table; if it has no entry, leave the country
  empty and keep the RTT.
- A download with no connection yet has neither value.

## Tests

Unit: the address-to-country rule including private ranges, and the RTT
formatter's bands. Integration: one download from the lab server, the country
empty and the RTT present.

## Proof

`cargo test -p idm-core`, then the real app showing a flag and a time for a
download from a public host, and "this computer" for the lab server.

## Out of scope

Geolocation of the *user*; nothing about the machine is read or sent.
