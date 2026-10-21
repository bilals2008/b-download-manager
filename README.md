# my-idm-clone

A multi-connection download manager built with Tauri v2 (Rust) and React +
TypeScript + Tailwind CSS v4 + shadcn/ui.

The engine is a port of the mechanics reverse engineered out of
`IDMan.exe` (IDM 6.43.15.2). Read [`ANALYSIS.md`](./ANALYSIS.md) for the
evidence — the exact strings, addresses and pseudo-code the design is based
on, and for an explicit list of what could *not* be established.

## How it works

1. **Probe.** A one-byte range request (`Range: bytes=0-0`) reveals the total
   size and proves the server honours ranges. A server that answers `200` is
   not segmented. Over FTP the size comes from `SIZE`.
2. **Segment.** The file is split into contiguous byte ranges, one per
   connection, each with its own temporary part file. The number of ranges is
   the one IDM counts: one per 10 KB of the file, at most as many as the
   user allows.
3. **Download.** Every connection asks for its own range with
   `Range: bytes=<start>-<end>` and `Accept-Encoding: identity`, and writes
   into its part file. Over FTP every range opens its own data connection and
   asks for the file with `RETR`.
4. **Check.** The head of the file is kept and compared with the head of
   every other range. A range that holds the head of the file is a range the
   server did not honour: its bytes are given back, the range is fetched
   again, and the download goes on with a single connection.
5. **Reassemble.** When the last range lands, the parts are joined in order
   and renamed onto the destination.
6. **Resume.** A JSON manifest next to the part files records what is done.
   Re-running the same URL picks up where it stopped. A manifest whose
   ranges overlap or are out of order is not resumed from.

## Running it

```bash
npm install
npm run tauri dev      # the desktop app
npm run dev            # the UI only, in a browser (IPC calls will fail)
npm run build          # type-check and bundle the frontend
npm run check:palette  # WCAG contrast gate for the colour tokens

cd crates/idm-core && cargo test   # the engine, without a window
cd src-tauri && cargo test         # the application
```

The engine is its own crate and knows nothing about windows, so the whole
suite runs headless: `cargo test` drives the engine against a small HTTP
server and a small FTP server on the loopback interface, from the first
request to the finished file.

There is a headless client as well:

```bash
cd crates/idm-cli
cargo run -- https://example.com/big.bin ./big.bin --connections 8
```

## What the measurements said

IDM's rule for the number of connections is one per 10,240 bytes of the
file, capped by the number the user allows (`FUN_005bad00`, see
`ANALYSIS.md`). What that is worth depends on the server and the link, so it
was measured with the headless client against a server that allows ranges
(20 MB, the same server, back to back):

| Connections | Run 1 | Run 2 |
|---|---|---|
| 1 | 110.6s | 23.0s |
| 8 | 19.5s | 429.9s — the server answered `429` |

The link moved by a factor of five between the runs, so only the two fast
runs can be compared: 19.5s with eight connections against 23.0s with one.
The link was the limit in both, and the honest conclusion is that this
measurement does not settle the question — it needs a link that is not
throttled.

What the measurement did settle was a bug: the run that answered `429` left
a **one byte file** behind. A server that is busy answers `429` with a body
of one byte, and the probe read that body as the size of the file. The
probe now fails on any answer that is not a success, and the download asks
again; the test `a_server_that_is_busy_is_not_mistaken_for_a_small_file`
covers it.

## Where the planning lives

[`planning/`](./planning/) holds everything needed to finish the alternative
and to continue reverse engineering IDM:

- [scope and goals](./planning/01-scope-and-goals.md)
- [the reverse engineering plan](./planning/02-rea-plan.md) — the next
  targets, with addresses, in priority order
- [target architecture](./planning/03-target-architecture.md)
- [engine specification](./planning/04-engine-spec.md)
- [UI design direction](./planning/05-ui-design-direction.md)
- [milestones](./planning/06-milestones.md)
- [verification plan](./planning/07-verification-plan.md)
- [risks and open questions](./planning/08-risks-and-open-questions.md)
- [what is left](./planning/09-remaining-work.md) — the work that is still
  open, item by item
- [decisions](./planning/decisions/) — ADRs

## Where the code lives

```
crates/
  idm-core/       the engine: no window toolkit, no application
    downloader.rs   the download itself: ranges, retries, pause, assembly
    http.rs         the wire format: range headers, Content-Range, retries
    ftp.rs          the FTP client: login, PASV, REST, RETR
    transfer.rs     where the bytes come from: HTTP or FTP
    verify.rs       the check that the data of a range belongs to that range
    limits.rs       the connection budget, the per site caps, the speed limit
    disk.rs         free space on the destination volume, replacing a file
    segmenter.rs    the ChunksList analogue: byte ranges and part files
    session.rs      the index of known downloads, so the list survives a restart
    log.rs          the diagnostics log
  idm-cli/        a headless client for the engine
src-tauri/src/
  lib.rs         the Tauri commands the UI calls
  state.rs       the set of known downloads
src/
  App.tsx        the single screen: new download, live table
  lib/ipc.ts     typed wrappers over the Tauri commands
  components/ui/ shadcn/ui primitives (button, progress, table, input, card, badge)
```

## Tauri commands

| Command | Purpose |
|---|---|
| `start_download(url, dest, connections)` | begin a download, returns its id |
| `list_downloads()` | current state of every download |
| `pause_download(id)` / `resume_download(id)` | suspend and continue |
| `cancel_download(id)` | stop and keep the parts for a later resume |
| `remove_download(id)` | drop it from the list, after stopping it |
| `set_speed_limit(bytes_per_second)` | the bandwidth cap of the application |
| `set_download_speed_limit(id, bytes_per_second)` | the bandwidth cap of one download |
| `set_site_connections(host, connections)` | the connection cap of one site |
| `site_connections(host)` | the connection cap of one site, when there is one |
| `set_proxy(url)` | the proxy new downloads go through, HTTP or SOCKS |
| `default_destination()` | a sensible starting path |

The backend pushes `download://update` events with the same `DownloadInfo`
shape, so the table stays live without polling.

## Where the bytes come from

| Address | What happens |
|---|---|
| `http://…`, `https://…` | a range request per connection, `Accept-Encoding: identity` |
| `ftp://user:pass@host/path` | a data connection per range, `REST` for the offset |
| either, with a proxy set | the proxy the user chose, HTTP or SOCKS |

## Colour

The palette is warm and soft on purpose — paper, sand and clay, with sage
used only for "finished". It lives in `src/index.css` and nothing else
hardcodes a colour. `npm run check:palette` reads those tokens and fails if
any pair the product renders drops below its WCAG 2.2 minimum.
