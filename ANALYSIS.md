# IDMan.exe — reverse engineering notes

Target: `C:\Program Files (x86)\Internet Download Manager\IDMan.exe`
IDM 6.43.15.2 · PE32 (x86) · 6,201,712 bytes
SHA-256 `e3fc655a335e17a6fdd09a9b315291d0c8863180f0d801e5dd7433c9e56afbd8`

Analysed with REA (Ghidra 12.1.4, headless). 19,603 procedures, 7,261 strings.

**Nothing was executed.** Every statement below comes from static analysis of
the shipped binary. Where the analysis stopped, that is written down.

---

## 1. The architecture that fell out of the strings

IDM's own assertion strings name its classes and methods. That string corpus
is the map:

| Area | Names recovered from `IDMan.exe` |
|---|---|
| File pieces | `ChunksList`, `ChunksListItem` |
| Transport | `Connection`, `HTTPConnection`, `FTPConnection`, `ProxyConnection`, `CFTPProxyConnection`, `CInetConnector`, `CSocks` |
| Session / scheduling | `Session`, `HTTPSession`, `FTPSession` |
| Application | `CDownload`, `CProgressItem`, `CSpeedLimiter`, `CFragmentsList`, `CFilePart` |
| Media splitting | `CMp4Converter`, `CMp4DashReader`, `CSidxParser`, `CTSAVMixer`, `CPMTStream` |

RTTI type descriptors (`.?AV…@@` in `.rdata`) confirm the same set:
`CFragmentsList` @ `0x780410`, `CFilePart` @ `0x7803f8`, `HTTPConnection` @
`0x7805b0`, `CProgressItem` @ `0x77f490`, `CDownload` @ `0x77bcc4`.

---

## 2. Multi-threaded downloading

### 2.1 It speaks HTTP over raw sockets

There is **no WinHTTP and no WinInet** in the import table. The socket API is
`WS2_32.DLL`:

```
socket  EXTERNAL:0x20a      recv   EXTERNAL:0x212      send   EXTERNAL:0x20f
ioctlsocket EXTERNAL:0x20b   closesocket EXTERNAL:0x209
```

HTTPS is terminated by IDM itself with OpenSSL — the install directory ships
`libssl.dll` / `libcrypto.dll`, and the code has
`HTTPConnection::SendHTTPSGet()` and `HTTPConnection::RecvHTTPSData()`. The
socket read path even branches on the protocol:

```c
// FUN_005b32a0 — one read, two transports
if (*(int *)(*(int *)(in_ECX + 200) + 0x24) == 5) {     // 5 == TLS
    iVar3 = FUN_005b32a0(buf, len);                     // SSL_read
} else {
    iVar3 = recv(*(SOCKET *)(in_ECX + 0x31f8), buf, len, 0);
}
```

Threads come from `__beginthreadex` (`0x647702`), reached through
`CWinThread::CreateThread` (`0x61c453`) — one worker per connection.

### 2.2 The Range header is built by hand

Four format strings, verbatim from `.rdata`:

| Address | String |
|---|---|
| `0x6cf9bc` | `Range: bytes=0-%I64d\r\n` |
| `0x6cf9d4` | `Range: bytes=0-\r\n` |
| `0x6cf9e8` | `Range: bytes=%I64d-\r\n` |
| `0x6cfa00` | `Range: bytes=%I64d-%I64d\r\n` |

plus `&bytes=%I64d-%I64d` (`0x69c720`) and `&bytes=0-%I64d` (`0x6ca288`) for
URL-style range parameters.

All four are referenced from **one** function, `FUN_0058b320` (5,239 bytes),
which is the request builder. Decompiled, the core of it:

```c
// FUN_0058b320 — build the Range header for this connection
*(undefined4 *)(in_ECX + 0x68) = iVar7 + -1;        // range start
*(int *)      (in_ECX + 0x6c) = iVar16;             // range end
_sprintf(local_d4, "Range: bytes=%I64d-%I64d\r\n",
         *(undefined4 *)(iVar14 + 0x48),            // start
         *(undefined4 *)(iVar14 + 0x4c),            // end
         *(undefined4 *)(in_ECX + 0x68), iVar16);
```

The offsets `+0x60/+0x64` and `+0x68/+0x6c` are 64-bit file positions — the
whole coordinate system of the downloader is 64-bit, from the first byte
offset to the Content-Length arithmetic.

The same function also emits the request template found at `0x6cf81c`:
`Connection: Keep-Alive\r\n`, and the encoding header:

| Address | String |
|---|---|
| `0x6cf870` | `Accept-Encoding: identity\r\n` |
| `0x6cf88c` | `Accept-Encoding: %s\r\n` |
| `0x6cf8a4` | `Accept-Encoding: gzip, deflate\r\n` |

`identity` matters: with a compressed encoding the byte offsets on the wire
stop matching the byte offsets in the file, and every range would be wrong.

### 2.3 The reply is checked

`Content-Range:` lives at `0x6cf014`, and there are parsers for
`multipart/x-byteranges` (`0x6cf0ac`) and `multipart/byteranges`
(`0x6cf0c4`) — IDM understands a server that answers a range request with a
multipart body.

`HTTPConnection::GetRetrRangeReply()` (`0x6cf35c`) is the function that
validates the range reply. Three routines read the `Content-Range` header:
`FUN_00581c80`, `FUN_00582780`, `FUN_0058c7f0`.

---

## 3. Dynamic file piecing

### 3.1 The chunk list is the source of truth

`ChunksList` is not a metaphor — it is a linked list of `ChunksListItem`
objects, and its methods are named in the binary:

```
ChunksList::NeedNewConnection()            ChunksList::FindMaxFreeChunk()
ChunksList::CreateNewItem()                ChunksList::InitFirst()
ChunksList::AppendFile()                   ChunksList::JoinFile()
ChunksList::InsertFromReg()                ChunksList::ApplyAlternative()
ChunksList::AddAlternative()               ChunksList::CheckAlternative()
ChunksList::FindChunkWithZipCentralDir()   ChunksList::Erase()
ChunksList::EraseRegSubKey()               ChunksListItem::set_fileName()
ChunksListItem::JoinWithNext()
```

### 3.2 Each chunk is a byte range in its own file

`ChunksList::CreateNewItem()` (`FUN_0055ed40`) allocates a 0x70-byte item and
names the file from a directory plus a counter:

```c
iVar4 = FUN_00610e69(len);                        // wide string alloc
FUN_0040f3c0(iVar4, L"%s%ld",
             *(undefined4 *)(*(int *)(in_ECX + 0x88) + 0x10),  // folder path
             *(undefined4 *)(in_ECX + 0x94));                 // counter
```

and opens it with a deliberate access mask (`FUN_0056fdf0`):

```c
CreateFileW(path,
            0xc0000000,   // GENERIC_READ | GENERIC_WRITE
            1,            // FILE_SHARE_READ
            NULL,
            2,            // CREATE_ALWAYS
            0, NULL);
```

The item layout read straight out of `FUN_00564650`:

| Offset | Meaning |
|---|---|
| `+0x08`, `+0x0C` | start / end of the range (64-bit pair) |
| `+0x10` | file name (wide string) |
| `+0x14` | file handle |
| `+0x18` | kind: 1 = primary, 2 = alternative |
| `+0x58`, `+0x5C` | next / previous chunk pointers |
| `+0x60`, `+0x64` | current position (64-bit) |
| `+0x70`, `+0x74` | chunk start (64-bit) |
| `+0x78`, `+0x7C` | chunk end (64-bit) |
| `+0x80`, `+0x84` | bytes received in this chunk (64-bit) |

### 3.3 Bytes are written at an absolute offset

The chunk writer (`FUN_0055db20`) seeks before it writes:

```c
if (param_4 != 0) {
    SetFilePointer(local_30, param_4, (PLONG)0x0, 0);   // FILE_BEGIN
    local_48 = param_4;
    local_44 = 0;
}
...
WriteFile(local_40, lpBuffer, local_4c, &local_50, (LPOVERLAPPED)0x0);
```

so a chunk file is a sparse, randomly addressed container for its byte range.
The same function carries the memory ceiling: it uses `VirtualAlloc`/malloc
and switches strategy above `0x5fffff`, allocating `0x600000` (6 MiB).

### 3.4 The list survives a crash

The chunk list is written to the registry:

```
Software\DownloadManager\<id>\ChList
Software\DownloadManager\<id>\LocalFileName
```

with a 16-byte record per chunk (`FUN_00564650`):

```c
_sprintf(local_58, "%lda", param_1[0x18]);
RegSetValueExA(*(HKEY *)(local_60 + 0xdc), local_58, 0, 0,
               (BYTE *)&local_74, 0x10);
```

and the resume path truncates the file to the recorded length
(`FUN_0058fdf0`):

```
"Dwnl %I64d, Trucate at %I64d \n"
```

### 3.5 Connections come and go

Two mechanisms, both visible in the strings:

**Reassignment** — when a connection dies its chunk is handed to a live one:

```
"The file chunk %d is reassigned to connection %d."        (FUN_005b2830)
"The file chunk of connection %d is reassigned to connection %d." (FUN_00574510)
```

**Growth** — IDM adds connections on the fly:

```
"Add alternative connection (%ld) for connection %ld"      (FUN_00564650)
"Need optional connections." / "Optional connections not needed."
"Optional connections, connecting..." / "Optional connections have been created."
```

`FUN_00564650` is the interesting one: the new connection inherits the
parent's *current* position and takes over the tail of the parent's range,
while the parent continues with a fresh chunk at the same point.

```c
// the new chunk starts where the parent currently is
local_64[2] = *(int *)(*local_64 + 0x60);    // start = parent position (low)
local_64[3] = *(int *)(*local_64 + 100);     // end   = parent position (high)
*(int *)(iVar4 + 0x70) = *(int *)(iVar4 + 0x60);
*(int *)(iVar4 + 0x74) = *(int *)(iVar4 + 100);
```

Scheduling is driven by the slowest connection:
`HTTPSession::GetSlowestConnection()` and
`HTTPSession::TryStartNewOptionalConnections()`.

### 3.6 Integrity checks

IDM does not trust the server blindly:

```
"Strart data for this chunk is matched with start of the file!!! s2"
"Strart data for this chunk is matched with start of the file!!! s3"
"Chunk order violation error, cInf.ID %ld, startPos %I64d"
"Try rollback, set chunksize to %I64d"
"IDM started to download the file using one connection because of data
 corruption when newly received data in several connections did not match
 the same data downloaded earlier."
```

The first group compares the head of a chunk against the head of the file; a
mismatch rolls the download back to a single connection.

### 3.7 Assembly

The last step joins the parts and renames the result into place
(`FUN_00560ed0`):

```
"Assembling all downloaded portions into one file..."
"Rename temp file %s to %s."
"Step5, can not rename temp file %s to %s, errno = %ld"
```

The temp file is created with `FILE_ATTRIBUTE_TEMPORARY` (`0x80`,
`FUN_00562030`), and the join walks the chunk list in order.

---

## 4. Buffer management

Every constant below was read out of the code, not guessed.

| Size | Where | What it is |
|---|---|---|
| `0x200` (512) | `FUN_0057fd10`, `FUN_005b0d80` | HTTP header line buffer: `recv(s, buf, 0x200, 0)` |
| `0x3ff` (1023) | `FUN_005725a0` | whole header block: `memset(buf, 0, 0x3ff)` + `recv(s, buf, 0x3ff, 0)` |
| `0x2da0` (11,680) | `FUN_00574080` | stack copy buffer: `char local_2da8[11680]` |
| `0x600000` (6 MiB) | `FUN_0055db20` | chunk memory ceiling before memory-mapped I/O |

The 512-byte header line buffer is the interesting one: it is a *line* buffer,
so a header longer than 511 bytes is a problem the code has to handle.

Above the socket there is a second, sliding buffer
(`FUN_0057fc20`) that decouples "bytes arrived" from "bytes consumed":

```c
// FUN_0057fc20 — copy out of the receive buffer
uVar2 = in_ECX[0xd1f];                 // bytes buffered
if (uVar2 < param_3) param_3 = uVar2;
_memcpy(param_2, (void *)in_ECX[0xd1e], param_3);
piVar1 = in_ECX + 0xd1f;
*piVar1 = *piVar1 - param_3;
if (*piVar1 != 0) {
    _memmove((void *)in_ECX[0xd1e],
             (void *)((int)in_ECX[0xd1e] + param_3),
             in_ECX[0xd1f]);          // compact
}
```

The buffer is filled through a virtual call (`(**(code **)*in_ECX)()`), so the
transport (plain socket vs TLS) is pluggable underneath one reader.

Two more buffers show up in the SOCKS proxy path: 8 bytes for the SOCKS4
reply, 2 bytes for the SOCKS5 greeting, 5 bytes for the SOCKS5 connect reply
(`FUN_00568030`, `FUN_005681c0`, `FUN_00568310`).

### Chunked transfer encoding

`FUN_0055a670` is a `Transfer-Encoding: chunked` decoder. It reads a 4-hex-digit
chunk size into `in_ECX+0x2b`, converts it with `ntohl`, and rewrites the
header before writing the payload:

```c
uVar2 = ntohl(*(u_long *)(in_ECX + 0x2b));
hostlong = *(int *)(in_ECX + 0x54) + 0xb;
if (hostlong != uVar2) { ... }
WriteFile(local_14, (u_long *)(in_ECX + 0x2b), 4, &local_30, NULL);
```

---

## 5. Connection limits

`CMaxConnNumbers` reads `MaxConnectionsNumber` from
`HKCU\Software\DownloadManager\MCN`, and the observed default is **8**:

```c
local_120[0] = 4;
LVar2 = RegQueryValueExA(DAT_00788d08, "MaxConnectionsNumber", ...);
if (LVar2 != 0) {
    lpData[0] = '\b';      // 8
    lpData[1] = '\0';
    ...
}
```

Per-site overrides live in the same key as subkeys with a `number` value.
IDM also warns the user about the abuse risk of high connection counts, and
enforces a global cap:

```
"Too many open network connections. Please wait for some downloads to finish."
```

---

## 6. Guessed logic

This is inference, clearly separated from what was observed.

1. **One connection first, then a fan-out.** IDM starts a single connection,
   measures its throughput, and then adds *optional* connections that claim
   the tail of the range the first connection is on. It compares the slowest
   connection (`GetSlowestConnection`) against the others and grows the
   connection count while the server and the link keep up.

2. **The chunk list is the state machine.** Everything the downloader needs to
   resume — which ranges exist, which file holds each range, how many bytes of
   each range have landed — lives in the chunk list, persisted in the
   registry. A crash loses the sockets, not the plan.

3. **Sparse part files with absolute addressing.** Each chunk file is written
   with `SetFilePointer` at absolute offsets, so a connection can be
   interrupted at any byte and resumed at the same place. The chunk's byte
   range and its offset inside the part file are the same coordinate system.

4. **A last line of defence against a lying server.** The chunk-head
   comparison ("Strart data for this chunk is matched with start of the
   file!!!") and the "Chunk order violation" check exist because a range
   request can come back with the wrong bytes. When that happens IDM rolls
   back to one connection rather than producing a corrupt file.

5. **Small buffers, many of them.** The 512-byte header line buffer and the
   11,680-byte copy buffer are small by modern standards. IDM's throughput
   comes from having many connections in flight, not from one big buffer.

6. **TLS is a plugin, not a different client.** The `protocol == 5` branch
   inside the read loop means the same range/offset logic drives both plain
   HTTP and HTTPS.

---

## 7. What was not established

Being explicit about the edges of the analysis:

- **No runtime observation.** The binary was never executed, no packets were
  captured, no files were written. Everything above is static.
- **No debug symbols.** Procedure names are `FUN_…`. Class and method names
  come from assertion strings and MSVC RTTI, which is strong but indirect
  evidence — a string names a method, it does not bind it to a specific
  function unless the reference was followed, and only some were.
- **Some owning procedures were not resolved.** MSVC SEH filters are separate
  Ghidra functions (`Catch@0056aeff` and friends); for several of them the
  enclosing function was not recovered, so the method name could not be tied to
  a specific routine.
- **The initial split arithmetic was not located.** `FUN_0058fdf0`
  (11,863 bytes) holds the download state machine and the truncation/resume
  logic, but the exact rule that picks *N* and the boundary offsets was not
  traced. `FUN_005b1330`, the function behind "Change chunk size to %ld", is
  only `ntohl` — the producer of that value is elsewhere and was not found.
  (A second pass located the caller of that setter, at `0x5b7122`; see
  section 10.)
- **Ghidra's string inventory covers defined data only.** Literals in packed
  or undefined regions would be missing; an empty result does not prove the
  text is absent.
- **The `multipart/byteranges` path was not analysed**, only observed as a
  string.
- **Media-specific splitting** (MPEG-TS, MP4 DASH, HLS) exists in the binary
  (`CMp4DashReader`, `CTSAVMixer`, `CSidxParser`) and was not investigated.

---

## 10. The second pass: the resume repair and the chunk check

A second analysis run, with the same Ghidra provider, read the procedures
that the first pass had only guessed at. The tools were driven through the
REA command line, because the REA MCP server is not reachable from this
harness; the analysis profile, the evidence and the limitation list are the
same either way.

### 10.1 Rebuilding the chunk list (`FUN_005634de`)

This is the function that reads the chunk list back out of the registry when
a download is resumed. The strings in it are the protocol:

```
"Loading old file size %I64d Bytes from registry"        (value "FileSize")
"Rebuilding old downloaded files before resume.\n"
"Read record num. %s from registry: ID = %ld, startPos = %I64d, nextID = %ld"
"Deleting record %s from registry"
"Adding record with ID %ld, startPos %I64d, nextID %ld"
"Chunk order violation error, cInf.ID %ld, startPos %I64d"
"Try rollback, set chunksize to %I64d"
"Set curID %ld, curPos %I64d"
```

**The record layout is now known.** Each value under
`Software\DownloadManager\<id>\ChList` is 16 bytes, and the log line names
the three fields in order:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | `ID` — the chunk id |
| 4 | 8 | `startPos` — where the chunk starts in the file |
| 12 | 4 | `nextID` — the chunk that follows it |

The value name is `"<n>a"`, and `n` is the chunk id.

**The order check and the rollback.** The rebuild walks the records and keeps
the position it has reached. When a record's `startPos` lies *before* that
position, the chunk list is out of order:

```
"Chunk order violation error, cInf.ID %ld, startPos %I64d"
```

and the chunk is rolled back: the expected size of the chunk is computed as
`startPos(this record) - startPos(the previous record)`, logged as

```
"Try rollback, set chunksize to %I64d"
```

and the chunk file is truncated to it with `SetFilePointer` plus
`SetEndOfFile`. The bytes that are already in the file are then fetched
again.

**What this changes.** The clone keeps the chunk list in a JSON manifest
instead of the registry, so the same damage can arrive through a crash in the
middle of a write. The engine now checks the manifest before it resumes
from it: the ranges have to be contiguous, in order, and no range may hold
more bytes than its length. A manifest that fails the check is not resumed
from — the download starts over. The part files are still cut back to the
recorded length, the way IDM truncates the file when it resumes.

### 10.2 The chunk check (`FUN_00566f10`)

The strings that the first pass attributed to the rollback path are in
reality in a different procedure, at `0x566f10`:

```
"Strart data for this chunk is matched with start of the file!!! s2"
"Strart data for this chunk is matched with start of the file!!!"
"Strart data for this chunk is matched with start of the file!!! s3"
"There is real matching"
"no match, step 4"
```

The shape of the procedure, as read from the decompilation:

1. The first time a chunk gets data, the head of the file is read once into
   a buffer of `0x80c` bytes (2,060) and kept for the whole download
   (`FUN_0055b130` fills it, the pointer is stored next to the download).
2. The head of the chunk is compared with that buffer by `FUN_00566730`.
3. A chunk that holds the *head of the file* is a chunk whose range was not
   honoured — a server that sends the whole file whatever was asked for.
4. On that failure the chunk is **rolled back**: the bytes it had are taken
   off the totals again, the chunk's received counter is set back to zero
   (`0x567234`: `MOV dword ptr [ESI + 0x80], EBX`), and the chunk is fetched
   again.

The result codes the procedure returns are `-8`, `-9` and `-10`, which the
caller turns into "this connection is not usable". The message text reads
like a success ("is matched") but it is printed on the path that rolls the
chunk back: the data *is* the head of the file, and that is the fault. The
clone does not copy the wording.

**What this changes.** The engine keeps the head of the first range and
compares the head of every other range with it. A range that starts with
the head of the file is rolled back and fetched again, and the download
reports the problem instead of writing a file that is quietly wrong.

### 10.3 The comparison itself (`FUN_00566730`)

The function that does the comparison is short enough to read whole:

```c
uint FUN_00566730(uint param_1, int *param_2, int *param_3, int *param_4, char *param_5)
{
  *param_3 = 0;                                    // bytes matched so far
  *(int *)(in_ECX + 0x804) += 1;                   // a counter of comparisons
  iVar2 = *param_4;                               // the offset already matched
  uVar1 = *(int *)(in_ECX + 0x800) - iVar2;        // what is left of the head
  if ((int)param_1 < (int)uVar1) uVar1 = param_1;  // the length to compare
  piVar4 = (int *)(iVar2 + in_ECX);                // the head, at that offset
  for (uVar5 = uVar1; 3 < uVar5; uVar5 = uVar5 - 4) {
    if (*piVar4 != *param_2) goto mismatch;        // four bytes at a time
    param_2 = param_2 + 1;
    piVar4 = piVar4 + 1;
  }
  ... the last three bytes ...
  if (all bytes equal) {
    iVar2 = uVar1 + iVar2;
    *param_3 = iVar2;
    if (((*(int *)(in_ECX + 0x808) == 0) || (iVar2 < 0x40)) && (iVar2 < 0x200)) {
      *(int *)(in_ECX + 0x804) -= 1;
      *param_4 = iVar2;          // the matched offset advances
      *param_5 = 1;
      return (uint)uVar3 << 8;
    }
    *(int *)(in_ECX + 0x808) += 1;                // counted as a mismatch
    return CONCAT31(uVar3, 1);
  }
mismatch:
  *param_5 = 0;
  return 0;
}
```

Read together with its caller, this says three things:

1. **The comparison is incremental.** The offset that has already been
   matched (`*param_4`) is kept between calls, and the data is compared with
   the head of the file from that offset on. The data arrives in pieces, so
   the check runs over the pieces.
2. **The head of the file is at most `0x80c` bytes** (the size is kept at
   `in_ECX + 0x800`), and a match is only *clean* up to `0x200` bytes. A
   longer match, or a match after a mismatch has been seen
   (`in_ECX + 0x808`), is counted as a mismatch of its own.
3. **The return value is zero when the data does not match the head**, and
   non-zero when it does. The caller acts on the non-zero case: it logs
   "Strart data for this chunk is matched with start of the file!!!" and
   rolls the chunk back.

So the check is: *the data of a chunk must not start with the head of the
file*. A chunk that does holds the beginning of the file where the middle of
the file belongs — a server that answered the range request with the whole
file. The rollback gives the bytes back and fetches the range again, and
when that happens again the download goes on with a single connection.

**What this changes.** The clone compares the first `0x200` bytes of a range
with the head of the file, keeps a head buffer of `0x80c` bytes, and when a
range is found to hold the head of the file it gives the bytes back and
re-fetches the range. The test
`a_server_that_sends_the_wrong_bytes_goes_on_with_one_connection` in
`crates/idm-core/src/downloader/tests.rs` drives that path against a server that
answers every range that does not start at the first byte with the head of
the file.

### 10.4 The chunk size that is not about files

The first pass left one thread open: `FUN_005b2ae0` logs "Change chunk size
to %ld" and stores the value at `in_ECX + 0x3488`, and `FUN_005b1330` (only
`ntohl`) was assumed to be its producer. The caller of the setter is at
`0x5b7122`, and that address is in the **RTMP** packet dispatcher
(`FUN_005b70d0`): its cases are "Flex stream send", "Flex shared object",
"Flex message" and "shared object", and its unknown case reads "Unknown
packet type received: 0x%02x".

The chunk size that `FUN_005b2ae0` changes is therefore the chunk size of
the **RTMP stream**, not the size of a range in an HTTP download. The
initial split arithmetic of the HTTP downloader is not behind that setter,
and it was not found elsewhere either.

**What this changes.** The clone does not pretend to know IDM's split rule.
It splits the file into equal ranges, one per connection, and it adds
connections while the slowest range lags behind. The number of connections
that is fastest is a question for a measurement, not for the binary; the
headless client `crates/idm-cli` measures it.

### 10.5 How many connections a file gets

The function that answers that question is `FUN_005bad00`, the one the
"Need optional connections." branch calls. It is short enough to read whole,
and it computes the number of connections from the size of the file:

```c
uVar5 = *(uint *)(in_ECX + 0x1c0);          // the total size, low word
iVar7 = *(int *)(in_ECX + 0x1c4);           // the total size, high word
...
uVar6 = uVar5 - uVar2;                     // the bytes that are left
...
lVar8 = __alldiv(uVar6, iVar7, 0x2800, 0); // the bytes left / 10240
iVar7 = min(*(int *)(in_ECX + 0x1ac), (int)lVar8);
iVar4 = *(int *)(in_ECX + 0xf0);           // the connections the user allows
if ((iVar7 < iVar4) && (iVar7 = ..., iVar4 <= ...)) iVar7 = iVar4;
if (0x20 < iVar7) iVar7 = 0x20;            // never more than 32
if (*(int *)(in_ECX + 0x188) != 0) {
   if (protocol == 2 && 4 < iVar7) iVar7 = 4;
   else if (8 < iVar7) iVar7 = 8;
}
```

**The rule is one connection per 10,240 bytes of the file.** The result is
capped by the number of connections that are allowed, raised to the number
the user asked for when that number is the smaller one, and never more than
32 (or 8, or 4 for one protocol, when a flag is set). When less than
`0x2801` bytes are left, the number the user asked for is used.

The caller adds the optional connections up to that number: the strings
"Need optional connections.", "Optional connections not needed.",
"Optional connections, connecting..." and "Optional connections have been
created." all belong to that path, and the connections themselves are 15,000
bytes apart in the session (`iVar5 * 15000`).

**What this changes.** The clone uses the same rule:
`segmenter::connections_for(total, max)` gives one connection per 10 KB of
the file, at least one and at most the number that is allowed. A 20 KB file
is fetched with two connections, a 100 KB file with eight.

### 10.6 One difference from IDM, on purpose
IDM writes every chunk into its own file at the **absolute** offset of the
chunk, so a chunk file is a sparse container with a hole in front of it
("The chunk's byte range and its offset inside the part file are the same
coordinate system"). The clone writes each range into its own part file
**from the first byte of that range**, and the part files are joined in
order.

The reason is the size on the disk: with the absolute layout, the part files
of a 1 GB download with eight connections hold 1 + 2 + … + 7 parts of
nothing but holes. The coordinate system of the *download* is unchanged —
every range still knows where it belongs in the file — but the part files
hold exactly the bytes of their range, so they can be joined by copying them
one after the other.

This was a bug before it was a decision: the first version wrote at the
absolute offset and joined the part files from their first byte, which
produced a file full of holes for every download with more than one range.
The test `a_file_of_several_ranges_is_joined_in_the_right_order` in
`crates/idm-core/src/downloader/tests.rs` covers it.

### 10.7 What the measurement found

The rule for the number of connections was put to the test with the headless
client (`crates/idm-cli`) against a server that allows ranges. The link was
throttled and the numbers moved by a factor of five between runs, so the
measurement does not settle the question of how many connections are
fastest.

It did find a fault of its own. A run that the server answered `429` left a
**one byte file** behind: a busy server answers `429` with a body of one
byte, and the probe read the length of that body as the length of the file.
The probe now fails on every answer that is not a success, and the download
waits and asks again — the same rule the other requests follow. The test
`a_server_that_is_busy_is_not_mistaken_for_a_small_file` covers it.

---

## 8. How the findings map onto `my-idm-clone`

| Finding | Where it lives in this project |
|---|---|
| `Range: bytes=%I64d-%I64d` | `crates/idm-core/src/http.rs` → `open_range()` |
| `Accept-Encoding: identity` | `open_range()` and `probe()` |
| `Content-Range` validation | `open_range()` |
| `ChunksList` | `crates/idm-core/src/segmenter.rs` → `Segment`, `plan()`, `split()` |
| `ChunksListItem` (own file per range) | `Segment.part` + `segmenter::part_path()` |
| Registry `ChList` | `<dest>.idm.json` manifest |
| Truncate on resume | `restore()` → `file.set_len(segment.downloaded)` |
| "Add alternative connection" | `segmenter::split()` + `maybe_grow()` |
| "Reassigned to connection" | the segment task retries the same range; a split hands the tail over |
| "Assembling all downloaded portions" | `assemble()` |
| Rename temp file into place | `assemble()` → `fs::rename` |
| `0x2da0` copy buffer | `COPY_BUF` in `downloader.rs` |
| `0x600000` chunk memory ceiling | `CHUNK_MEM_LIMIT` (documented constant) |
| `MaxConnectionsNumber` = 8 | `DEFAULT_CONNECTIONS` |
| Chunked transfer decoding | handled by `reqwest`; IDM's decoder is documented in this file |
| The 16 byte chunk record (`ID`, `startPos`, `nextID`) | the manifest in `downloader.rs`; the layout is in section 10.1 |
| "Chunk order violation error" | `Manifest::is_consistent()` — a list that is out of order is not resumed from |
| "Try rollback, set chunksize to %I64d" | the part file is cut back to the recorded length |
| "Strart data for this chunk is matched with start of the file!!!" | `verify.rs` → `verify::check()` |
| "IDM started to download the file using one connection because of data corruption" | `downloader.rs` → `downgrade_to_one_connection()` |
| `0x80c` head of the file, `0x200` clean match | `verify::HEAD_BUFFER` and `verify::HEAD_BYTES` |
| "Too many open network connections." | `limits.rs` → `ConnectionBudget` |
| `HKCU\Software\DownloadManager\MCN` per site overrides | `limits.rs` → `SiteLimits` |
| The chunk list under `Software\DownloadManager\<id>\ChList` | the index in `session.rs` and the manifest next to the parts |
| One connection per `0x2800` bytes | `segmenter::connections_for()` |
| "Optional connections, connecting..." | `downloader.rs` → `maybe_grow()` |
| `FTPConnection::StartGetChunk`, `SendRetr` | `ftp.rs` — one data connection per range, `RETR`, `REST` for the offset |
| `ProxyConnection::Connect`, `CSocks` | `limits.rs` → the proxy of the application, HTTP or SOCKS |

---

## 9. The interface strings

The window furniture of IDM 6.43 was read out of the same string tables: the
UTF-16 resource strings for the menus and the ASCII strings for the toolbar
tooltips and the categories.

### 9.1 The main menu, in the order the resource stores it

```
&Tasks      &Add new download | Add &batch download | Add batch download from &clipboard
            Run site &grabber | Show &drop target
            E&xport -> To IDM export &file | To &text file
            &Import -> &From IDM export file | From &text file
            &Exit
&File       &Stop Download | &Remove | &Download Now | R&edownload
&Downloads  &Pause All | &Stop All | &Delete All Completed
            Find (Ctrl-F) | Find Next (F3) | S&cheduler
            Start &queue -> Start
            S&top queue -> Stop
            Speed &Limiter -> Turn o&n | Turn of&f | S&ettings... | &Options
&View       Hide &categories
            &Arrange files -> By &Order Of Addition | By &File Name | By &Size
                           | By St&atus | By &Time Left | By Transfer &Rate
                           | By &Last Try Date | By &Description | By Save &Path
                           | By R&eferer | By Parent &Web Page
            &Toolbar -> &Customize... | &Large Buttons | &Small Buttons
                     | Look for &new toolbars | Neon
            &IDM tray icon -> 3&D style | C&lassic style | Do&n't show
                            | C&ustomize URL List... | Dark Mode support
            Font -> Choose Font... | Reset to default Font
            &Language -> &English
&Help       H&elp contents | &Tutorials | &Scheduler and queues | &Grabber Help
            | Tip of the &Day... | IDM &Home Page | &Contact IDM Support
            | Check for &updates... | &About IDM | Tell a &friend
&Registration &Order online | R&egistration
```

The ampersand marks the access key, so the top level is reached with
Alt+T, Alt+F, Alt+D, Alt+V, Alt+H and Alt+R.

### 9.2 The toolbar tooltips

One run of ASCII strings holds the whole set, in toolbar order:

```
Add new download address manually
Start/Resume downloading selected file(s)!
Stop downloading selected file(s)
Stop all downloads
Remove selected file(s) from the list
Remove all completed files from the list
Browsers/System integration, File types, Proxy, Passwords and others
Scheduler and download queues
Start main download queue, or choose a queue to start from drop-down list
Stop main download queue, or choose a queue to stop from drop-down list
Write a letter to your friend about IDM!
```

The last two confirm that Start queue and Stop queue are dropdown buttons in
IDM, not plain buttons.

### 9.3 The categories

The category names sit in one block of the resource:

```
General | Documents | Games | Video | Music | Programs
```

and the tree of the main dialog adds three groups above them:

```
All Downloads | Unfinished | Finished
```

Two more strings decide whether the pane is on screen at all:

```
Show categories
Hide categories
```

`Hide &categories` is the item in the View menu, and `CIDMMainDlgTree` /
`CIDMFoldersTree` are the classes that own it. The clone follows that: the
categories pane is a tree with those three groups and the eight categories,
and View > Hide categories takes it off the screen.

### 9.4 The download complete window

A finished file gets a window of its own, not a box in the list. The strings
that describe it are in the same table:

```
0x6bb1d4  Download complete
0x6be2e0  File %s download complete.\n\nDownloaded %s (%I64d Bytes)\nFile saved as %s
0x6c9d30  Show download complete dialog
0x69fd78  CompleteDlg.htm
```

The first is the title, the second is the body with the file name, the
formatted size, the byte count and the save path, and the third is the option
that turns the window on. `CompleteDlg.htm` is the name of the page the
dialog shows, which is why IDM's window carries more text than a message box.

The "Open folder" button is in the same block (`0x6be2e0` has no buttons of
its own, but `Open folder` is one of the strings around it), and
`0x6bc5b4` (`Download complete. Downloaded %I64d Bytes Total`) is the line the
status bar writes at the same moment.

The clone opens its own window from `src-tauri/src/complete.rs` with those
words, a second webview so the list stays usable, and the buttons Open file,
Open folder and Close. The option at `0x6c9d30` lives on the Downloads page of
the options window as "Show download complete dialog"; it is stored with the
other preferences and pushed to the engine, which reads it before it builds
the window.

The referer the browser hands over is kept with the download and set as a
default `Referer` header on the HTTP client, so every range request carries
the page the link was on. It is validated as a header value, because it comes
from the browser, and it survives a restart with the rest of the index entry.


---

## 11. The third pass: the request header, the method names, the scheduler

A third run of the same tools (REA command line, Ghidra 12.1.4 headless, the
same binary and the same digest) added three things the earlier passes had
left open: the name the client gives itself on the wire, the full corpus of
IDM's own method names, and the function that starts the optional
connections.

### 11.1 The User-Agent

Two strings in the table say who the client is:

```
U16  Mozilla/5.0 (Windows NT 6.1; WOW64) AppleWebKit/537.36 (KHTML, like Gecko)
A8   Mozilla/5.0 (compatible; MSIE 10.0; Windows NT 6.1; Trident/6.0)
A8   Referer:
A8   User-Agent for manually added downloads:
```

The first is the name IDM sends on a normal request; the second is the older
IE name it keeps for the servers that want it. The clone sent its own name
(`my-idm-clone/0.1.0`), and a server that only serves browsers answered
`403 Forbidden` — the video links of Google are the everyday case.

**What this changes.** `http::USER_AGENT` now carries the string that was read
out of the binary, and every request goes out with it. The test
`the_request_carries_the_browser_user_agent` covers it.

### 11.2 The names of the methods

IDM names its own methods in the strings it uses for error reporting. The
pattern is always the same:

```
Unknown error during <Class>::<Method>()
```

One run of the string table lists the engine's classes and their methods:

```
ChunksList::AddAlternative()      ChunksList::AppendFile()
ChunksList::ApplyAlternative()     ChunksList::CheckAlternative()
ChunksList::FindMaxFreeChunk()     ChunksList::InitFirst()
ChunksList::InsertFromReg()        ChunksList::JoinFile()
ChunksList::NeedNewConnection()    ChunksListItem::JoinWithNext()
Connection::CheckConnect()         Connection::OnChangeBounds()
Connection::OnComplete()           Connection::ProcessRecv()
Connection::ReceiveData()          Connection::SelectForRead()
Connection::SetCompleted()         HTTPConnection::GetRetrRangeReply()
HTTPSession::GetSlowestConnection()  HTTPSession::TryStartNewOptionalConnections()
HTTPSession::ReceiveData()         HTTPSession::InitSession()
FTPSession::CreateOptionalConnections()  FTPSession::GetSlowestConnection()
FTPSession::InitSession()          FTPConnection::StartGetChunk()
FTPConnection::SendRetr()          FTPConnection::InsInChunksList()
FTPConnection::CheckRetrReply()    FTPConnection::RollBack()
Session::set_fileSize()            Session::CheckFormat()
CContentTypeChecker::Check()       CContentTypeChecker::CheckHTML()
```

**This is the map the earlier passes were missing.** `InitFirst` is the
initial split, `FindMaxFreeChunk` is the chunk with the most room left,
`AddAlternative` and `ApplyAlternative` are the hand-over of a tail to a new
connection, and `NeedNewConnection` is the question "is another connection
warranted". The names are evidence of the *existence* of the methods; the
addresses behind them are still to be bound.

The window furniture is named the same way, and it confirms the two context
menus the clone now has:

```
CDownloaderDlg::OnRclickUrllist()     the right click on a row
CDownloaderDlg::CreateTreePopupMenu() the right click on a category
CDownloaderDlg::OnProperties()        the properties of a row
CIDMListCtrl::OnBegindrag()           the drag handles of the queue
```

### 11.3 The optional connections

`FUN_0057cfa0` starts the extra connections. Its strings give it a name:

```
"Need optional connections.\n"
"Optional connections not needed.\n"
```

and it calls `FUN_005bad00` — the function that counts the connections a file
deserves (section 10.5) — before it decides. The connections live in one
array, 15,000 bytes apart (`iVar5 * 15000`), and each one has an "optional"
flag. The count it works to is the number the user allows, at
`in_ECX + 0xf0`, and it never goes past what `FUN_005bad00` asks for.

The check that opens the way for another connection is `FUN_00572000`, a
160-byte function that asks a virtual method of the session and returns zero
when the way is clear:

```c
iVar1 = (**(code **)(*in_ECX + 0x18))(in_ECX + 0xc81);
if (iVar1 != 0) { *(int *)(in_ECX[0xc85] + 0x14c) = iVar1; return iVar1; }
FUN_005b0a80(1);
_memset(in_ECX + 0xc94, 0, 0x3ff);
return 0;
```

**Observed:** the structure above. **Inferred:** that `FUN_0057cfa0` is the
FTP session's `CreateOptionalConnections` and not the HTTP one, because the
HTTP family of routines sits in the `0x597xxx` range
(`Catch@005973d2`, `HTTPSession::TryStartNewOptionalConnections`) while this
one sits at `0x57cfa0`, next to the FTP machinery. The binding of the HTTP
routine is still open.

**What this changes.** The clone already asks the same two questions before
it adds a connection: how many the file deserves (`segmenter::connections_for`)
and how many are allowed (`limits::ConnectionBudget`, `limits::SiteLimits`).
The order of the answers is now known: the count of the file first, the
allowance second.

### 11.4 The progress is a percentage

IDM writes the progress of a download as a plain number with a percent sign
(`%d %%`), and the string table also holds the window furniture that shows
it. The clone shows the same number in its Progress column.

### 11.5 The hand-over of a tail (the hypothesis, confirmed)

`FUN_00564650` carries the string

```
"Add alternative connection (%ld) for connection %ld\n"
```

so it is `ChunksList::AddAlternative()` — the function that gives the tail of
one connection to another. Its body is short enough to read whole, and it
answers the question the plan had left open.

The record of a connection holds a start (`+0x60`), an end (`+0x78`), the
bytes that have arrived (`+0x80`) and the position it has reached (`+0x70`).
What `AddAlternative` does is:

```c
// the parent moves on by what it has already received
parent.start  += parent.received;
parent.received = 0;

// the new connection takes the tail
new.start  = parent.start;      // the point the parent has reached
new.end    = parent.end;        // the end of the parent's range
new.received = 0;
```

**Observed.** The new connection starts exactly where the parent has got to
and runs to the end of the parent's range; the parent keeps the bytes it has
already written and starts counting again from zero.

**This confirms the hypothesis** of section 5 of the plan: the new connection
takes the tail of the parent's range at the parent's current position. The
clone does the same thing in `segmenter::split()` — the parent keeps
`[start, split_at)` with the bytes it has, and the new segment takes
`[split_at, end)`.

The one difference is the layout on the disk, and it is the one that was
chosen on purpose (section 10.6): IDM moves the *start* of the parent's
range forward and writes at absolute offsets, the clone shortens the
*end* of the parent's range and keeps one part file per range.

### 11.6 The next lead: the scope table of `InitFirst`

The method that holds the *first* split of a file is named in the strings:
`ChunksList::InitFirst()`. Its error string sits at `0x6cac34` and is used
by the tiny filter routine `Catch@0055ff79` (26 bytes), which the compiler
puts inside the function it protects. The pointer to that filter lives in
the MSVC scope table at `0x765168` in `.rdata`, inside the block
`0x7646ac`–`0x76a428` the plan pointed at.

**What this changes.** The plan can be followed from the other end: the
table record at `0x765168` belongs to the function that installs the
handler, and that function is the caller of `InitFirst()`. Reading the
scope table back is the next step; the addresses are written down here so
the next pass does not have to find them again.
