# Engine specification

Algorithm by algorithm. This is the contract the implementation should meet;
where the current implementation deviates, the deviation is called out.

## 1. Probe

**Goal:** learn the total size and whether the server honours ranges, with one
request.

```
GET <url>
Range: bytes=0-0
Accept-Encoding: identity
```

| Response | Meaning | Action |
|---|---|---|
| `206` + `Content-Range: bytes 0-0/TOTAL` | ranges work | `total = TOTAL`, segment normally |
| `200` + `Content-Length: N` | ranges ignored | `total = N`, **one connection only** |
| `206` with `Content-Range: bytes 0-0/*` | unknown size | `total` unknown; download to EOF |
| `416` | empty file | fail |
| `404` / `410` | gone | fail permanently, no retry |

**Current implementation:** `probe()` in `src-tauri/src/downloader.rs` does
this. It does not handle the `*` total case or `416` — both are gaps.

## 2. Plan

**Goal:** decide the number of connections and the boundaries.

```
count = clamp(ceil(total / SEGMENT_TARGET), 1, max_connections)
```

`SEGMENT_TARGET` is currently 16 MiB. The boundaries are then split evenly,
with the remainder spread over the first segments, so the ranges are
contiguous and cover the file exactly.

**Open question:** IDM's own rule is unknown. See
[02-rea-plan.md](./02-rea-plan.md) Priority 1. Until it is known, the
alternative is to **measure**: sweep the connection count against a
range-capable server and pick the split that maximises throughput.

**Current implementation:** `segmenter::plan()`.

## 3. Connect

**Goal:** one request per connection, for its own range.

```
GET <url>
Range: bytes=<start>-<end>
Accept-Encoding: identity
```

`Accept-Encoding: identity` is not optional. With a compressed encoding the
bytes on the wire no longer line up with the byte offsets in the file, and
every range is wrong. IDM sends it too (`0x6cf870`).

The response is validated before a single byte is written:

| Check | Failure |
|---|---|
| status is `206` | if `200` and `start > 0` → the server ignored the range |
| `Content-Range` start == requested start | mismatch → the server sent the wrong bytes |
| status `416` | the range is already complete; not an error |

**Current implementation:** `fetch_range()`. Done.

## 4. Write

**Goal:** store the bytes where they belong, and be able to stop at any byte.

Each connection owns a part file and writes at an **absolute** offset:

```
seek(part, start + received)
write(part, bytes)
received += bytes.len()
```

This is IDM's model: the chunk writer `FUN_0055db20` calls
`SetFilePointer(handle, offset, NULL, FILE_BEGIN)` before every write, so a
chunk file is a sparse, randomly addressed container.

Two consequences fall out of that choice and both are wanted:

- stopping mid-stream costs nothing but the current socket;
- a second connection can take over the tail of a range, because both write
  into the same coordinate system.

**Current implementation:** `fetch_range()` does the seek and the write. It
re-reads the segment end on every chunk, so a range that was shortened
underneath it stops at the right byte.

## 5. Rebalance — the part that makes it fast

**Goal:** use the whole link, and survive a slow connection.

Two mechanisms, both observed in IDM.

**Reassignment.** A connection that dies does not lose its range; another
connection picks it up. IDM: *"The file chunk %d is reassigned to
connection %d."* The implementation simply lets the failed task retry the
same range, which has the same effect.

**Growth.** When the slowest live connection lags, split its remaining range
and start another connection. IDM: *"Add alternative connection (%ld) for
connection %ld"*. The split point is the parent's current position, so the
parent keeps what it has and the new connection continues from there.

```
if connections < max_connections:
    slowest = the live connection with the least progress
    if slowest.remaining > MIN_SPLIT:
        new = split(slowest.at_current_position)
        start a connection for new
```

**Current implementation:** `maybe_grow()` in `downloader.rs`. It currently
picks the segment with the most bytes *remaining* rather than the one with
the least *progress*, which is a different thing. To be corrected: compare
progress ratios, not absolute remaining bytes. See M4 in the milestone plan.

## 6. Assemble

**Goal:** produce the final file.

```
for segment in segments (ordered by start):
    copy exactly segment.len() bytes from segment.part into the output
rename(output.tmp, destination)
delete the part files and the manifest
```

Copying **exactly** `segment.len()` bytes matters: after a crash a part file
may be longer than the manifest says, and copying the whole file would
append garbage.

The rename is what makes the download atomic from the user's point of view —
the destination either does not exist or is complete.

**Current implementation:** `assemble()`. Done, including the exact-length
copy and the rename.

## 7. Resume

**Goal:** after a crash, a restart, or a network drop, continue from where
the bytes stop.

The manifest holds, per segment, the range and the number of bytes received.
On start:

1. read the manifest; if the URL and the total match, keep the segments;
2. for each segment, truncate the part file to `received` bytes — a crash may
   have left extra bytes behind;
3. drop any segment whose part file is missing;
4. if the manifest does not match, start fresh.

IDM does the same thing, with the manifest in the registry and the same
truncation: *"Dwnl %I64d, Trucate at %I64d"*.

**Current implementation:** `restore()`. Done.

## 8. Verify

**Goal:** never hand the user a corrupt file.

This is the weakest part of the current implementation and it is important.

IDM compares the head of a chunk against the head of the file
(*"Strart data for this chunk is matched with start of the file!!!"*), checks
the chunk order (*"Chunk order violation error, cInf.ID %ld, startPos
%I64d"*), and on a mismatch drops to a single connection
(*"IDM started to download the file using one connection because of data
corruption…"*).

The rules to implement:

| Check | When | On failure |
|---|---|---|
| server honoured the range | before writing anything | downgrade to one connection |
| `Content-Range` start matches | before writing anything | retry, then downgrade |
| chunk head matches the file head | after the first chunk of a segment | rollback that segment |
| chunk order is monotonic | on every write | rollback the download |
| final size equals the expected size | after assembly | fail loudly |

**Current implementation:** only the first two. See M5.

## 9. Respect the limits

| Limit | Default | Where |
|---|---|---|
| connections per download | 8 | `DEFAULT_CONNECTIONS` |
| connections in total | 32 | not implemented |
| minimum segment size | 1 MiB | `MIN_SPLIT` |
| retry attempts per range | 8 | `MAX_ATTEMPTS` |

## 10. Report

The engine pushes a `DownloadInfo` to the UI at most every 120 ms, and
always on a state change.

| Field | Meaning |
|---|---|
| `total` | expected size, 0 if unknown |
| `downloaded` | sum of `received` over all segments |
| `connections` | live connection count |
| `status` | queued / running / paused / completed / failed / cancelled |
| `error` | one human-readable line, only when failed |

The UI never computes progress itself; it renders what it is told. That keeps
one source of truth for the numbers.
