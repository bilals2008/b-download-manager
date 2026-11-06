# What is left

The state of the work, measured against the source on **6 November 2026 at
v1.0.0**. Everything that is done is ticked and names the file that proves it;
everything that is not is written down as it is, in the order it makes sense to
build it. The milestones in [`06-milestones.md`](./06-milestones.md) say what
each stage has to achieve; this file says what is left to do.

The earlier version of this file was written at v0.1 and was never re-measured,
which is why several of its unchecked items were already shipped. Rule 5 of
[`README.md`](./README.md) exists to stop that happening again.

## The engine

- [x] Multi-connection range downloader with a chunk list — `downloader.rs`,
      `segmenter.rs`
- [x] Resume from the manifest, and a manifest that is not resumed from when
      its ranges overlap or are out of order — `session.rs`, tested by
      `a_manifest_with_broken_ranges_is_not_resumed_from`
- [x] The chunk head check: a range that holds the head of the file is given
      back, and the download goes on with one connection
- [x] Retry with a cap (`MAX_ATTEMPTS` is 8), `Retry-After`, and no retry for
      an answer that will not change
- [x] Unknown total size — `plan_unknown`, `spawn_unknown_size_server`
- [x] The connection count: one per `0x2800` bytes (`BYTES_PER_CONNECTION`),
      the way the reference product counts it
- [x] A global connection budget (`state.rs:925` resizes it live) and a speed
      limit, global and per download (`speed-limit-tab.tsx`)
- [x] The engine in its own crate, and a headless client for it
- [x] A diagnostics log, and the resume state written when the app closes
- [x] **Media splitting.** The earlier version of this file said "the clone can
      only fetch whole files today". That was wrong. Each segment of an HLS or
      MPEG-DASH track is fetched by the ordinary engine, a segment already on
      disk is not fetched again, and the track joins in order
      (`crates/idm-core/src/media/download.rs`).
      - [ ] MPEG-TS as an input container. `sidx` cues are deliberately not
            parsed: `dash.rs:540` records that the index would only change how
            a range is split, not the bytes.
      - [ ] Live streams: the manifest is parsed and the flags are set, no
            recording path exists
- [ ] **The free space check on every platform.** `free_space()` is a Windows
      `GetDiskFreeSpaceExW` call; on macOS and Linux it returns `None` and the
      check is skipped silently (`src-tauri/src/lib.rs`, `crates/idm-core/src/
      disk.rs`). A `statvfs` branch closes this.
- [ ] **The file signature check.** The reference product has `CSignVerifier`
      for the case where a server lies about the *content* of the first range —
      the head check only catches a range that was not honoured.
- [ ] **`multipart/byteranges`**: a server that answers a range request with a
      multipart body. The studied binary understands it.
- [ ] **A queue inside the engine**, with priorities and a start time. The
      queue that the toolbar drives today lives in `src-tauri/src/queue.rs`:
      priorities yes, start times nowhere.
- [ ] **Proxy credentials.** `proxy-settings.tsx` tells the user plainly that
      "a user name or password in the address does not work".
- [ ] **A fuzzed manifest reader.** The reader is careful, but a file written
      by a crashing machine can be anything. There is no fuzz or property
      dependency in the workspace.
- [ ] **Tests for the headless client.** `crates/idm-cli` reports `0 passed`
      in `cargo test --workspace`.
- [ ] **A soak test**: a long run with random disconnects, and no handle that
      leaks.
- [ ] **A flaky test to fix.**
      `pausing_an_unknown_size_download_does_not_finish_it_short` failed inside
      the full-suite run on 6 November 2026 and passed alone twice. Pace the
      fixture rather than the assertion.
- RTMP stays out of scope; see [`01`](./01-scope-and-goals.md).

## The protocols

- [x] HTTP and HTTPS — `http.rs`
- [x] FTP: one data connection per range, `REST` for the offset — `ftp.rs`
- [x] A proxy: HTTP or SOCKS, for the whole application — the `socks` feature
      of `reqwest`, `Limits::proxy`
- [ ] **The addresses that must not go through the proxy.** The reference
      product keeps a list of them; the clone sends everything through the
      proxy.
- [ ] **FTP over a proxy**, and FTPS (implicit TLS).
- [x] **Cookies and credentials for media.** `src-tauri/src/video/policy.rs`
      forwards only `Cookie`, `User-Agent`, `Referer`, `Origin` and
      `Authorization`, only to the manifest's own origin, never a value over
      8 KiB, nothing written to disk or the log, and only after the one-time
      consent held in `DesktopPreferences::cookie_consent`. The Firefox add-on
      fills those headers from the browser's own cookies
      (`extension-firefox/background.js:169`), so a protected stream plays. The
      referer travels for ordinary links too (`bridge/batch.rs:239` into
      `transfer.rs:183`), and a referer that would start a header is refused.
- [ ] **Credentials on the other paths.** The Chrome/Edge extension declares
      the `cookies` permission and never reads it — no file under
      `browser-extension/` uses it — so a manifest captured there arrives
      without a session. A plain-file hand-over still carries only a URL and a
      referer, and there is no stored site-login list to fall back on.

## The product around the engine

- [x] The window: title bar, menu bar, toolbar, the categories pane, the
      download list
- [x] The list of downloads survives a restart
- [x] **The tray icon**, with the three choices the reference product offers:
      "Don't show" (`tray_hidden`), "Dark Mode support" (`tray_dark_mode`,
      which picks between the two drawings `tray-on-dark.png` and
      `tray-on-light.png`), and "Customize URL List..."
      (`src/components/shell/url-list-dialog.tsx`, opened from Settings >
      General and mounted in `src/App.tsx`). The clipboard watch matches the
      saved protocol list (`clipboard.rs`), and the tray menu carries show,
      pause-all, resume-all and quit (`src-tauri/src/tray.rs`). The older
      version of this file called these "three icon styles"; they are one mark
      in two surface treatments.
- [ ] **The tray only mode**: start with no window at all, and open one from
      the icon. `minimize_to_tray` and `close_to_tray` exist; a start that is
      hidden from the first frame does not.
- [x] **The browser integration**: the pairing code, the context menu, the
      takeover of the browser's own downloads, the file-type list and the site
      skip list. Both extensions ship: `browser-extension/` for Chrome and
      Edge, `extension-firefox/` published on AMO.
      - [ ] The Chrome/Edge cookie hand-off; Firefox already does it, as the
            protocols section says.
      - [ ] The popup shows the takeover switch and whether the app is running
            (`browser-extension/popup.html`); it does not show the live
            downloads, their total speed, or a way to pause them.
      - [ ] A native-messaging path, if the loopback bridge ever proves
            unreliable at startup; today the pairing code is the whole
            handshake (`bridge/auth.rs`).
- [ ] **The named queues and the scheduler**, with the schedule files. Nothing
      in the repository stores a start time, a queue name or a schedule; the
      menu item says "coming soon" and is disabled (`shell/menus.ts:102`).
- [x] **The options pages**, in four groups (`settings-nav.tsx`): Basics —
      General, Appearance, Toolbar, Shortcuts, Tips; Downloads — Downloads,
      Save locations, Browser integration, Video download button; Network —
      Connections, Proxy; Application — Advanced, About. Search at the top of
      Settings reaches every one of them.
      - [ ] Sounds: there is no completion sound and no sound picker.
      - [ ] Site logins, as above.
      - [ ] Dial-up and VPN are **not** planned. The reference product's pages
            are for a connection style that ended; leaving them out is a
            decision, not a gap.
- [x] **The context menus.** Both are built. The row menu carries Resume,
      Pause, Stop, Add to queue, Remove from queue, Show error and retry,
      Redownload, Open file, Open containing folder, the progress window, the
      two copy items, Move to category, Properties, Remove from list and Delete
      file. The category menu has Select all, Add category, Category properties
      and Remove category, and categories are editable
      (`category-context-menu.tsx`, `category-dialogs.tsx`).
- [ ] **The export and import** of an export file in the reference product's
      own format, or of a text file. Four disabled menu items
      (`menus.ts:61-73`) and no writer anywhere.
- [ ] **Add batch download** and **Show drop target**: also disabled stubs
      (`menus.ts:51`, `menus.ts:58`). The bridge already accepts a batch from
      the extension (`bridge/batch.rs`), so the in-app version is a UI job.
- [x] **A first run** notice in the add dialog (`firstRunDone`). It explains
      where files land, not the connection setting — the sentence the original
      plan asked for is still not written.

## Quality

- [x] The engine tests run without a window. Measured 6 November 2026:
      `cargo test --workspace` → **448 passed, 5 ignored** (178 in `idm-core`,
      270 in the app; the ignored five are two benchmarks and three tests that
      reach YouTube). `pnpm test` → **132 passed in 19 files**.
      `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo fmt --all --check` are both clean.
- [x] The contrast gate, the type check and the production build:
      `pnpm check:palette` prints "all 70 pairs pass in all 9 appearances"
- [x] **CI**: tests, clippy, the contrast gate and a signed production bundle
      on every push to `main`, on Linux and Windows
      (`.github/workflows/ci.yml`)
- [x] **A signed installer**, and signed builds for three platforms with an
      updater that verifies the signature before installing
      (`.github/workflows/release.yml`, `docs/RELEASES.md`)
- [ ] **The keyboard pass** over the whole shell. Shortcuts are defined,
      rebindable and tested at the hook level (`use-shell-shortcuts.test.tsx`);
      nothing walks the real tab order.
- [ ] **The screen reader pass** over the tree, the list and the listbox. There
      is no axe run in the suite.
- [ ] **The tests of the store**: filtering, sorting, the queue transitions.
      `src/lib/` covers selection, find, progress, settings, errors and the
      folder a category saves into (`downloads.test.ts`); the store's own
      filtering, sorting and queue transitions are still untested.
- [ ] **A check in a real window**, done by hand, on macOS and Linux. Those
      paths are built by CI and have never been opened here.
- [ ] **The connection sweep on a link that is not throttled.** The link that
      was measured moved by a factor of five between runs, so the question of
      how many connections are fastest is still open, and no throughput number
      appears in `README.md`. Run
      `cargo test --release bench_ -- --ignored --nocapture`.

## The reverse engineering

The old version of this file pointed at `ANALYSIS.md` for the evidence. That
report was removed from the repository in `433e99e`; the addresses below are
quoted from the studies that remain here.

- [x] The chunk list that is rebuilt when a download resumes, and the 16 byte
      record it is made of
- [x] The chunk check against the head of the file, and the rollback
- [x] The number of connections a file gets
- [ ] **The initial split arithmetic**: the boundaries inside a file, not the
      number of connections. It was not in the binary; the measurement is the
      way to answer it. The third pass named the method that holds it,
      `ChunksList::InitFirst()`, and the scope table that protects it
      (`0x765168` in `.rdata`); the function that installs that handler is
      the one to find next.
- [x] **The hand-over of a tail**: `ChunksList::AddAlternative()` (at
      `0x564650`) gives the tail of a range to a new connection, starting at
      the point the parent has reached. The clone already does this.
- [ ] **The method names of the classes**, bound to their routines. The names
      come from assertion strings; the routines behind `Connection::ProcessRecv`,
      `ChunksList::NeedNewConnection` and the others are still not bound. Two
      are bound so far: `ChunksList::AddAlternative()` at `0x564650` and, by
      inference, `FTPSession::CreateOptionalConnections()`.
- [ ] **The `multipart/byteranges` path**, which was only seen as a string.
- [ ] **The media splitting in the binary**, which was not investigated. The
      clone's own version is shipped, so this row is about matching the
      reference product, not about needing the feature.
