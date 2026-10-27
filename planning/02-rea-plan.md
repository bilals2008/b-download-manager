# Reverse engineering plan

The target is the reference product's 32-bit (x86) main binary, SHA-256
`e3fc655a335e17a6fdd09a9b315291d0c8863180f0d801e5dd7433c9e56afbd8`.

Current results are in [`../ANALYSIS.md`](../ANALYSIS.md). This file is the
**next** work: what is still unknown, where to look, and what evidence each
step should produce.

## Ground rules

- Analysis is **static** unless a task explicitly says otherwise. The binary
  is never executed, and no claim about runtime behaviour is made from static
  evidence.
- REA opens the target read-only. The session is ephemeral; annotate names
  inside it rather than editing the file.
- Every conclusion is labelled **observed** (a string, address, API call or
  instruction) or **inferred** (a structural reading of observed data).
- The reference product's publisher forbids reverse engineering. This work
  reads the binary to learn *behaviour*; nothing from it is copied into the
  clone.

## Priority 1 — the initial split arithmetic (highest value)

**Why:** we know the reference binary sends `Range: bytes=start-end`, and we
know it has a chunk list. We do **not** know how it decides *N* connections
and where the boundaries fall. That decision drives throughput more than
anything else.

**Where:**

- `FUN_0058fdf0` (11,863 bytes) — the HTTPConnection state machine. It
  contains `"Dwnl %I64d, Trucate at %I64d \n"` and is the largest unresolved
  function.
- `FUN_005b2ae0` (153 bytes) — logs `"Change chunk size to %ld"` and stores
  the value at `in_ECX + 0x3488`. Its input comes from `FUN_005b1330`, which
  is only `ntohl` — so the *producer* of the chunk size is elsewhere.
- The fields already identified on the connection object:
  `+0x60/+0x64` (start), `+0x68/+0x6C` (end), `+0x70/+0x74` (chunk start),
  `+0x78/+0x7C` (chunk end), `+0x80/+0x84` (bytes received), `+0x88`
  (folder object), `+0x94` (chunk counter).

**How:** `analyze_function` on `0x58fdf0`, then page the pseudocode with
`inspect_analysis_view` (it is too large for one frame). Look for the
division of the total size by a connection count, and for the writes to
`+0x60` / `+0x68` that are not the ones already found in `FUN_0058b320`.

**Evidence to produce:** the exact expression that turns
`(file size, connection count)` into a range, or a statement that the split
is server-driven (e.g. driven by a `Content-Range` hint) rather than
computed.

**If it cannot be found:** fall back to the measured behaviour. Run our own
multi-connection client against a range-capable server, sweep the connection
count, and pick the split that maximises throughput. That is a measurement,
not a guess, and it answers the question we actually care about.

## Priority 2 — binding method names to routines

**Why:** class and method names currently come from assertion strings. Those
strings are strong evidence that a method *exists*, but only some were traced
to a specific function. Several owning procedures were not recovered.

**Where:** the MSVC SEH scope tables in `.rdata` around `0x7646ac`–`0x76a428`.
Each scope record holds the filter function, which is the `Catch@…` routine
Ghidra created. Reading the table lets us find the function that installs the
handler.

**Evidence to produce:** a table of `class::method` → address, for at least:
`ChunksList::NeedNewConnection`, `ChunksList::FindMaxFreeChunk`,
`Connection::ProcessRecv`, `Connection::ReceiveData`,
`HTTPConnection::GetRetrRangeReply`, `HTTPSession::GetSlowestConnection`,
`HTTPSession::TryStartNewOptionalConnections`.

**How:** `read_bytes` on the scope table, then `resolve_containing_procedure`
on the table's owning function, or `procedure_callers` on
`__except_handler3`.

## Priority 3 — the scheduler

**Why:** we know the reference product compares the slowest connection and
adds "optional" connections. We do not know the trigger condition or the cap.

**Where:**

- `Catch@005973d2` — `HTTPSession::TryStartNewOptionalConnections()`
- `Catch@00597...` region, functions `FUN_005973f0`, `FUN_00597480`,
  `FUN_00597630`, `FUN_00597870` … `FUN_00599e20`
- `FUN_00564650` — `ChunksList::AddAlternative()`, already partly read
- Strings: `"Need optional connections."` (`0x6ce720`),
  `"Optional connections not needed."` (`0x6ce73c`),
  `"Optional connections, connecting..."` (`0x6bc590`),
  `"Optional connections have been created."` (`0x6bcc84`)

**Evidence to produce:** the condition under which a new connection is added,
the cap, and how the new connection's range is chosen. We already have a
hypothesis from `FUN_00564650`: the new connection takes the tail of the
parent's range at the parent's current position. Confirm or refute it.

## Priority 4 — the integrity and rollback path

**Why:** the reference binary compares a chunk's head against the file head
and, on a mismatch, drops to one connection. We currently only detect a
server that ignores the range entirely (`200` instead of `206`).

**Where:**

- `FUN_005634de` (3,938 bytes) — holds `"Try rollback, set chunksize to %I64d"`
  and `"Chunk order violation error, cInf.ID %ld, startPos %I64d"`, and
  `"Strart data for this chunk is matched with start of the file!!! s2"` /
  `"s3"`
- The product's own data-corruption message (`0x6b0d50`): it started to
  download the file using one connection because the newly received data in
  several connections did not match the same data downloaded earlier.
- `CSignVerifier`, `CSignVerifier_2`, `CSignVerifier_3` (RTTI) — a *different*
  verification mechanism; check whether it is about file signatures rather
  than chunk consistency.

**Evidence to produce:** the exact comparison performed, how many bytes are
compared, and what state the download is put into on failure.

## Priority 5 — the chunk list on disk and in the registry

**Why:** the resume format is a compatibility question. If we ever want a
user's existing download from the reference product to resume in our client,
the chunk list layout matters.

**Where:**

- `HKCU\Software\DownloadManager\<id>\ChList` — values named `"<n>a"`
  (16 bytes each), written by `FUN_00564650`
- `FUN_005634de` — reads and rebuilds that key
- `FUN_00562030` — `LocalFileName`, and the temp file created with
  `FILE_ATTRIBUTE_TEMPORARY`
- `FUN_0055ed40` — chunk file naming, `L"%s%ld"`

**Evidence to produce:** the 16-byte record layout, the meaning of the
trailing fields, and the file naming scheme in full.

**Caution:** this is the one area where reading the reference product's
on-disk format could cross from "learning behaviour" into "reading its users'
data". Stay on the format, not on any user's content.

## Priority 6 — protocols beyond HTTP

Each is self-contained and can be deferred:

| Area | Where | What to extract |
|---|---|---|
| FTP | `FTPConnection::StartGetChunk`, `InsInChunksList`, `SendRetr` | how the chunk list is driven from `RETR` |
| SOCKS | `CSocks`, `FUN_00568030`, `FUN_005681c0`, `FUN_00568310` | the 8 / 2 / 5 byte handshakes |
| HTTP proxy | `ProxyConnection::Connect`, `HttpsProxyConnect` | `CONNECT` handling |
| RTMP | `FUN_005b6cb0`, `FUN_005b7670`, `FUN_005b72e0` | the 1-byte handshake, `pheader` framing |
| MPEG-TS | `CTSAVMixer`, `CPMTStream` | how a TS stream is cut into segments |
| MP4 DASH / HLS | `CMp4DashReader`, `CSidxParser`, `CM3UParser` | segment index parsing, `Segment-Count` |
| Speed limit | `CSpeedLimiter` | the shaping algorithm |

## Priority 7 — optional, only if it changes the design

- `CProgressItem` and how the reference product computes displayed speed and
  ETA.
- `CDownloadLimits` and the scheduler, for the queue feature.
- The "Grabber" subsystem, which is out of scope for the product.

## Runtime observation, if it is ever needed

Static analysis cannot answer "what does the reference binary actually send
on the wire". If a question genuinely requires it:

- it must be answered against a **server you control**, or a capture of your
  own traffic;
- no traffic of the reference product is captured from third parties;
- the capture tooling is `REA`'s `inspect_web_network_capture` (retained HAR
  or mitmproxy) and the browser/Electron observation tools;
- the result is recorded as **runtime evidence** and clearly separated from
  static findings.

## Deliverable for each item

A short section appended to [`../ANALYSIS.md`](../ANALYSIS.md) containing:

1. the question;
2. the addresses and strings examined;
3. the observed answer, with the evidence inline;
4. what is still unknown;
5. the design consequence — what changes in `src-tauri/`, if anything.
