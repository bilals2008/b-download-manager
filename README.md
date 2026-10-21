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
   not segmented.
2. **Segment.** The file is split into contiguous byte ranges, one per
   connection, each with its own temporary part file. Small files are not
   split.
3. **Download.** Every connection asks for its own range with
   `Range: bytes=<start>-<end>` and `Accept-Encoding: identity`, and writes
   into its part file at the absolute offset.
4. **Reassemble.** When the last range lands, the parts are joined in order
   and renamed onto the destination.
5. **Resume.** A JSON manifest next to the part files records what is done.
   Re-running the same URL picks up where it stopped.

## Running it

```bash
npm install
npm run tauri dev      # the desktop app
npm run dev            # the UI only, in a browser (IPC calls will fail)
npm run build          # type-check and bundle the frontend
npm run check:palette  # WCAG contrast gate for the colour tokens
cd src-tauri && cargo test
```

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
- [decisions](./planning/decisions/) — ADRs

## Layout

```
src-tauri/src/
  segmenter.rs   the ChunksList analogue: byte ranges, part files, splitting
  downloader.rs  the engine: probe, range requests, retries, pause, assembly
  state.rs       the set of known downloads
  lib.rs         the Tauri commands the UI calls
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
| `remove_download(id)` | drop it from the list |
| `default_destination()` | a sensible starting path |

The backend pushes `download://update` events with the same `DownloadInfo`
shape, so the table stays live without polling.

## Colour

The palette is warm and soft on purpose — paper, sand and clay, with sage
used only for "finished". It lives in `src/index.css` and nothing else
hardcodes a colour. `npm run check:palette` reads those tokens and fails if
any pair the product renders drops below its WCAG 2.2 minimum.
