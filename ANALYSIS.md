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
- **Ghidra's string inventory covers defined data only.** Literals in packed
  or undefined regions would be missing; an empty result does not prove the
  text is absent.
- **The `multipart/byteranges` path was not analysed**, only observed as a
  string.
- **Media-specific splitting** (MPEG-TS, MP4 DASH, HLS) exists in the binary
  (`CMp4DashReader`, `CTSAVMixer`, `CSidxParser`) and was not investigated.

---

## 8. How the findings map onto `my-idm-clone`

| Finding | Where it lives in this project |
|---|---|
| `Range: bytes=%I64d-%I64d` | `src-tauri/src/downloader.rs` → `fetch_range()` |
| `Accept-Encoding: identity` | `fetch_range()` and `probe()` |
| `Content-Range` validation | `fetch_range()` |
| `ChunksList` | `src-tauri/src/segmenter.rs` → `Segment`, `plan()`, `split()` |
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

