# Target architecture

The finished alternative, split so that the parts that are hard to get right
(the engine) are separable from the parts that are easy (the shell).

## Workspace

```
my-idm-clone/
├── crates/
│   ├── idm-core/          the engine: no UI, no Tauri, no I/O it does not own
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── plan.rs        probe + segment planning
│   │   │   ├── segment.rs     the chunk list
│   │   │   ├── engine.rs      the scheduler and the per-connection tasks
│   │   │   ├── transport/     http.rs, ftp.rs, socks.rs  (feature gated)
│   │   │   ├── store.rs       part files and the resume manifest
│   │   │   ├── integrity.rs   chunk-head and order verification
│   │   │   ├── speed.rs       shaping and ETA
│   │   │   └── error.rs
│   │   └── tests/
│   ├── idm-tauri/         the Tauri shell: commands, events, state
│   └── idm-cli/           optional: a headless client, useful for testing
├── src/                   the React UI
└── src-tauri/             the Tauri app that wires the two together
```

`idm-core` must not depend on Tauri. That is the rule that makes the engine
testable without a window.

## Why a workspace

Today the engine lives in `src-tauri/src/`. That is fine while it is one
person and one screen. The moment there is a CLI, a library target, or a
second front end, the engine has to be its own crate. Moving it later is
cheap; moving it twice is not.

**Milestone:** M2 in [06-milestones.md](./06-milestones.md).

## Module responsibilities

| Module | Owns | Must not |
|---|---|---|
| `plan` | probing the server, deciding size and connection count | open sockets for data |
| `segment` | the chunk list, splitting, merging, ordering | know about HTTP |
| `engine` | the scheduler, retries, pause/cancel, progress | write files directly |
| `transport` | one request/response, ranged or not | decide how many bytes to ask for |
| `store` | part files, the manifest, atomic writes | parse HTTP |
| `integrity` | verifying what arrived | fetch anything |
| `speed` | rate shaping, ETA | throttle the network by itself |

## The core data model

```rust
/// One contiguous byte range of the target file.
pub struct Segment {
    pub id: SegmentId,
    pub start: u64,        // absolute offset of the first byte
    pub end: u64,          // absolute offset one past the last byte
    pub received: u64,     // bytes of this range already stored
    pub part: PathBuf,     // the file holding those bytes
    pub kind: SegmentKind, // Primary or Alternative
}

pub enum SegmentKind {
    /// The connection's own range.
    Primary,
    /// A range a second connection was given when the first one fell behind.
    Alternative,
}
```

`SegmentKind` exists because the reference product has it (`ChunksListItem`
field `+0x18`, value 1 for primary and 2 for alternative, observed in
`FUN_00564650`). It is how that binary represents "this connection is racing
that binary represents "this connection is racing another one over the same
bytes", and it is the mechanism behind the strings *"The file chunk %d is
reassigned to connection %d."*

## The scheduler

One task per live connection. The engine owns them in a `JoinSet`.

```
loop {
    select! {
        finished  = tasks.join_next()      => reap, and reassign the range if it failed
        _         = tick()                 => grow the connection count, re-check limits
    }
}
```

The tick does three things, in this order:

1. **Reap.** A connection that failed permanently hands its range back. A
   connection that finished hands its range to another one, or is retired.
2. **Grow.** If the slowest live connection is far behind the others, split
   its remaining range and start another connection — the reference product's
   *"Add alternative connection (%ld) for connection %ld"*.
3. **Save.** Write the manifest, so a crash costs at most one tick of work.

## The transport boundary

```rust
pub trait Transport: Send + Sync {
    /// Fetch [from, to] inclusive. Returns the bytes as they arrive.
    async fn fetch_range(&self, from: u64, to: u64) -> Result<ByteStream, TransportError>;
    /// Ask for the first byte only, to learn the size and the range support.
    async fn probe(&self) -> Result<Probe, TransportError>;
}
```

`fetch_range` must:

- send `Range: bytes=<from>-<to>` and `Accept-Encoding: identity`;
- fail if the server answers `200` for a range request;
- fail if `Content-Range` starts somewhere other than `from`;
- treat `416` as "this range is already complete", not as an error.

Those four rules are the whole of the contract observed in the reference
binary, and they are what `fetch_range` in `src-tauri/src/downloader.rs`
already implements.

## The manifest

One JSON file next to the part files.

```json
{
  "url": "https://example.com/file.zip",
  "total": 1234567890,
  "segments": [
    { "id": 0, "start": 0, "end": 524288000, "received": 524288000, "part": "file.zip.part0" }
  ]
}
```

Written atomically: write to `*.tmp`, then rename. A manifest that is half
written is a manifest that loses work on the next crash.

## IPC contract

The UI never polls. The backend pushes.

| Direction | Name | Payload |
|---|---|---|
| UI → backend | `start_download` | `{ url, dest, connections }` → `id` |
| UI → backend | `list_downloads` | → `DownloadInfo[]` |
| UI → backend | `pause_download` / `resume_download` / `cancel_download` | `{ id }` |
| UI → backend | `remove_download` | `{ id }` |
| backend → UI | `download://update` | `DownloadInfo` |

`DownloadInfo` is the single shape the UI renders. Adding a field to the
engine means adding it here, and the UI updates with no code change beyond
reading it.

## Global limits

Two separate limits, both configurable:

| Limit | Scope | Default |
|---|---|---|
| Connections per download | one download | 8 (the `MaxConnectionsNumber` default observed in the reference binary) |
| Connections in total | the whole app | 32 |

The reference product has both: *"Too many open network connections. Please
wait for some downloads to finish."* is in the binary, and so is the per-site
override mechanism under `HKCU\Software\DownloadManager\MCN`.

## Error handling

| Failure | Behaviour |
|---|---|
| Server does not support ranges | drop to one connection, restart the download from 0 |
| Server answers `200` for a range | treat as the previous case, loudly |
| `Content-Range` mismatch | fail the range, retry, then downgrade to one connection |
| Connection lost mid-range | retry the range from where it stopped, with backoff |
| Disk full | fail the download with a clear message; keep the parts |
| Server gone (404 / 410) | fail permanently, do not retry |
| Rate limited (429) | honour `Retry-After` if present |

Retries use exponential backoff with a cap. The current implementation uses
`400ms × attempt` up to 8 attempts; that is a starting point, not a policy.
