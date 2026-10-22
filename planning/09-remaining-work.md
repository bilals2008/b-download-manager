# What is left

The state of the work at the end of this session. Everything that is done is
ticked; everything that is not is written down as it is, in the order it
makes sense to build it. The milestones in
[`06-milestones.md`](./06-milestones.md) say what each stage has to achieve;
this file says what is left to do.

## The engine

- [x] Multi-connection range downloader with a chunk list
- [x] Resume from the manifest, and a manifest that is not resumed from when
      its ranges overlap or are out of order
- [x] The chunk head check: a range that holds the head of the file is given
      back, and the download goes on with one connection
- [x] Retry with a cap, `Retry-After`, and no retry for an answer that will
      not change
- [x] The free space of the destination volume, before and during
- [x] A file whose size the server does not tell
- [x] The connection count: one per 10 KB of the file, the way IDM counts it
- [x] A global connection budget, per site caps, and a speed limit
- [x] The engine in its own crate, and a headless client for it
- [x] A diagnostics log, and the resume state written when the app closes
- [ ] **Media splitting**: HLS, MPEG-DASH, MPEG-TS. IDM cuts a stream into
      parts (`CMp4DashReader`, `CTSAVMixer`, `CSidxParser`); the clone can
      only fetch whole files today. The whole area is M6.
- [ ] **The file signature check.** IDM has `CSignVerifier` for the case
      where a server lies about the *content* of the first range — the head
      check only catches a range that was not honoured.
- [ ] **`multipart/byteranges`**: a server that answers a range request with
      a multipart body. IDM understands it (`Content-Range` at `0x6cf014`).
- [ ] **RTMP**: the one-byte handshake and the `pheader` framing
      (`FUN_005b6cb0`).
- [ ] **A queue inside the engine**, with priorities and a start time. The
      queue that the toolbar drives today lives in the user interface.
- [ ] **A fuzzed manifest reader.** The reader is careful, but a file that
      was written by a crashing machine can be anything.
- [ ] **A soak test**: a long run with random disconnects, and no handle
      that leaks.

## The protocols

- [x] HTTP and HTTPS
- [x] FTP: one data connection per range, `REST` for the offset
- [x] A proxy: HTTP or SOCKS, for the whole application
- [ ] **The addresses that must not go through the proxy.** IDM keeps a list
      of them; the clone sends everything through the proxy.
- [ ] **FTP over a proxy**, and FTPS (implicit TLS).

## The product around the engine

- [x] The window: title bar, menu bar, toolbar, the categories pane, the
      download list
- [x] The list of downloads survives a restart
- [ ] **The tray icon**, in the three styles IDM offers, and the tray only
      mode
- [ ] **The browser integration**: the detection, the keys, the context menu
- [ ] **The named queues and the scheduler**, with the schedule files
      (`s_%s.dt`, `q_%s.dt`)
- [ ] **The remaining options pages**: file types, save to, downloads,
      connection, sites logins, sounds
- [ ] **The context menus** on a row and on a category. The row menu is built
      (Resume, Pause, Stop, Redownload, Remove, Open file, Open containing
      folder; Properties is greyed out); the menu of a category only has
      "Select all downloads" until the categories themselves can be edited.
- [ ] **The export and import** of an IDM export file or a text file
- [ ] **A first run** that explains the connection setting in one sentence

## Quality

- [x] The engine tests run without a window: 62 tests over HTTP and FTP
- [x] The contrast gate, the type check and the production build
- [ ] **The keyboard pass** over the whole shell
- [ ] **The screen reader pass** over the tree, the list and the listbox
- [ ] **The tests of the store**: filtering, sorting, the queue transitions
- [ ] **A check in a real window**, done by hand
- [ ] **CI**: the tests, the build and the contrast gate on every push. There
      is no remote yet, so there is nothing to run it on.
- [ ] **A signed installer**
- [ ] **The connection sweep on a link that is not throttled.** The link that
      was measured moved by a factor of five between runs, so the question of
      how many connections are fastest is still open.

## The reverse engineering

- [x] The chunk list that is rebuilt when a download resumes, and the 16 byte
      record it is made of
- [x] The chunk check against the head of the file, and the rollback
- [x] The number of connections a file gets
- [ ] **The initial split arithmetic**: the boundaries inside a file, not the
      number of connections. It was not in the binary; the measurement is the
      way to answer it.
- [ ] **The method names of the classes**, bound to their routines. The names
      come from assertion strings; the routines behind `Connection::ProcessRecv`,
      `ChunksList::NeedNewConnection` and the others are still not bound.
- [ ] **The `multipart/byteranges` path**, which was only seen as a string.
- [ ] **The media splitting** in the binary, which was not investigated.
