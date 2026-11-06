# Scope and goals

## The goal

A download manager that behaves like the reference product in the ways users
actually notice:

- a large file is pulled by several connections at once, not one;
- pausing, resuming and restarting a download keep the bytes already fetched;
- a download survives a crash, a reboot and a network drop;
- a server that misbehaves is detected instead of producing a corrupt file.

It does not have to look like the reference product, and it does not have to
implement every protocol the reference product does. The bar is: **on a
well-behaved HTTP
server, it downloads a large file at least as fast as a single-connection
client, and it can always resume.**

## In scope

The left column is the original scope. The right column says, with a date,
where the shipped product stands against it, because three of the "out of
scope" rows below turned into features anyway.

| Capability | State at v1.0.0 (measured 6 November 2026) |
|---|---|
| HTTP/1.1 with byte ranges | shipped (`crates/idm-core/src/http.rs`) |
| HTTPS | shipped (`reqwest` + `rustls-tls`) |
| Multi-connection segmentation | shipped (`segmenter.rs`, `downloader.rs`) |
| Per-connection part files | shipped (`ADR-0001`) |
| Manifest-based resume | shipped (`session.rs`, `ADR-0002`) |
| Pause / resume / cancel | shipped (`src-tauri/src/actions.rs`) |
| Dynamic connection growth | shipped; the ratio rule replaced the "most bytes remaining" rule (`downloader.rs`) |
| Server integrity verification | shipped for the chunk head and the range reply; a content-signature check is not |
| Retry with backoff | shipped, `Retry-After` honoured, capped at 8 attempts (`downloader.rs`) |
| Disk space checks | shipped on Windows; `free_space()` returns `None` on macOS and Linux (`src-tauri/src/lib.rs`) |
| Speed limiting | shipped twice: a global cap in Settings and a per-download cap in the progress window (`speed-limit-tab.tsx`) |
| A queue | shipped at the application layer (`src-tauri/src/queue.rs`); priorities yes, start times no |
| FTP | shipped (`ftp.rs`, `REST` per chunk) — was post-MVP, then built |
| A proxy, HTTP and SOCKS | shipped (`reqwest` `socks` feature) — was post-MVP, then built |
| Media: HLS, MPEG-DASH, plain-file tracks, quality panel, native remux | shipped (`media/`, `src-tauri/src/video/`) — was not in scope at all |
| Browser capture (extensions + loopback bridge) | shipped (`browser-extension/`, `src-tauri/src/bridge/`) — was declared out of scope |
| Site Grabber | shipped (`src-tauri/src/grabber/`) — was declared out of scope |

## Deliberately out of scope

| Not doing | Why |
|---|---|
| Copying any code, strings or assets of the reference product | licence and ethics; also the point of the exercise is the reimplementation |
| DRM circumvention | out of bounds |
| RTMP | one-byte handshake and `pheader` framing for a protocol with no real user demand in 2026 |
| Magnet links and torrent traffic | this is a range downloader, not a BitTorrent client; `transfer.rs` refuses a `magnet:` URL |
| Proxy chains and per-address proxy bypass | a single application proxy covers the actual use; a bypass list is a small, open item, not a protocol |
| FTPS (implicit TLS) and FTP over a proxy | each is its own project; plain FTP serves the remaining FTP users |

## Non-goals that are easy to get wrong

- **Not** "match the reference product's UI". Match its *mechanics*; the
  visual language is ours (see
  [05-ui-design-direction.md](./05-ui-design-direction.md)).
- **Not** "open a socket per file". Connection count is per download, capped
  globally, and the two limits are separate.
- **Not** "assume the server honours Range". The reference product does not
  assume it either, and neither do we.

## Success criteria

The product is done when all of these hold and are demonstrated, not asserted:

1. A 1 GiB file from a range-capable server downloads at a rate that scales
   with the connection count, measured on the same machine against a
   single-connection baseline.
2. Killing the process mid-download and restarting it resumes from the byte
   where it stopped, and the final file is byte-identical to the source.
3. A server that returns `200` for a range request produces a single
   connection and a correct file — not a corrupt one.
4. A server that returns the wrong bytes for a range is detected and the
   download is retried or downgraded, and the user is told.
5. The connection limit is honoured globally, and the UI shows the real number
   of live connections.
6. No colour, contrast or accessibility claim in the docs is unmeasured.

### Where these stand (measured 6 November 2026)

| # | Verdict | Evidence |
|---|---|---|
| 1 | **not yet earned** | no throughput table exists in `README.md` or `docs/`; `bench_local_segmented` and `bench_connections` are `#[ignore]`d and must be run and written down |
| 2 | engine-level, tested | the resume and manifest tests in `crates/idm-core/src/downloader/tests.rs`; not re-proven by a kill on a real machine |
| 3 | tested | `a_server_that_ignores_the_range…` family in `downloader/tests.rs` |
| 4 | tested | `a_server_that_sends_the_wrong_bytes_goes_on_with_one_connection` (`downloader/tests.rs:1095`) |
| 5 | shipped | `limits.rs` for the global budget; `src/components/windows/progress/connection-details.tsx` for the live count |
| 6 | partly | `pnpm check:palette` prints 70 pairs passing in all 9 appearances (run 6 November 2026); the keyboard and screen-reader passes are still not done |
