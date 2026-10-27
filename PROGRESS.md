# B Download Manager — work in progress

Everything the rebuild still owes, in the order it makes sense to build it.
Done items stay in the list as a record of what is already standing.

## Shell (title bar, menu bar, toolbar)

- [x] Title bar with app mark, window text and the three window buttons
- [x] Menu bar with the reference download manager's real menus, nested submenus, Alt+key access, hover switching
- [x] Toolbar with 11 hand-drawn SVG icons, roving tabindex, tooltips
- [x] Toolbar and menu items that have no screen yet are greyed out instead of dead
- [x] Toolbar button size setting (Large / Small Buttons) actually changes the icons
      — the size is kept between sessions and the menu ticks the one in use
- [ ] Toolbar customization dialog, icon picker, drag to reorder

## Categories pane (the sidebar)

- [x] Tree with the groups All Downloads / Unfinished / Finished
- [x] The eight categories: General, Video, Music, Documents, Programs, Games, Compressed, Other
- [x] Live count per entry, counts follow the download state
- [x] Click an entry filters the list, selection survives a filter change
- [x] Arrow keys, Home and End walk the tree, roving tabindex
- [x] View > Hide categories takes the pane off the screen
- [ ] Add, rename and delete a custom category (reference: Add category, Editing category properties)
- [ ] Per-category default folder, file type rules and site rules
- [ ] Persist the tree state, the reference download manager stores it under `Software\DownloadManager\FoldersTree`
- [x] Right-click menu on a category: Select all downloads works; the entries
      that need editable categories stay greyed out until they exist

## Download list

- [x] Columns File name, Size, Progress, Speed, Time left, Status
- [x] Rows come from the Rust engine; the list starts empty and fills as downloads run
- [x] Loading, empty, no-results and engine-error states
- [x] Drag-resizable columns, with arrow-key resizing and a double-click reset
- [x] Sort by every column, click a header to flip the direction
- [x] Row selection, Ctrl+click to add to the selection
- [x] Progress as a percentage, transfer rate, time left, status with a dot and
      a word; the row stays one line and opens nothing under it
- [x] The error of a failed download shows under its name in the row, and
      opens the Download Failed window
- [x] Download Failed window: the failure as a headline, one hint, the file
      and its folder, how much arrived and whether it can be resumed
- [x] The engine's raw sentence waits behind Technical details
- [x] Retry resumes from the parts on disk, Start over drops them and fetches
      the file again, and Choose another location moves the parts before it
      continues
- [x] Network, refusal, missing file, busy server, disk space, permission,
      invalid destination and a short assembly each get their own words
- [x] A file that appeared during the transfer offers only a new location: a
      retry at the same name cannot work and is not offered as if it could
- [x] Delete key on a selection, Shift+click for a range
- [x] Arrow keys, Home and End walk the list, Shift+arrow extends the range
- [ ] Inline edit of file name and save path
- [x] Context menu on a row with the entries: Resume, Pause, Stop,
      Redownload, Remove, Open file, Open containing folder, Properties
- [ ] Column chooser and drag to reorder (reference: Customize URL List, Columns)
- [ ] Show file description, referer and parent web page columns
- [ ] Group the list by category or by date

## Commands

- [x] Resume, Stop, Pause, Redownload, Remove on the selection
- [x] Pause All, Stop All, Delete All Completed
- [x] Start queue and Stop queue
- [x] Arrange files by every supported key
- [x] Open file and open containing folder, from the menu of a row
- [ ] Refresh download address, Properties
- [ ] Move item up, move item down, remove item from download queue
- [x] Find (Ctrl+F) and Find next (F3) over the list, by name or address;
      the match is selected and scrolled into view
- [ ] Import and export of a download-manager export file or a text file

## Queue and scheduler

The full queue and scheduler interface is not built yet. The Scheduler entry
in the Downloads menu and the toolbar says "coming soon" instead of opening a
half-working screen; the one queue below is the part that exists.

- [x] One queue, driven by the two toolbar buttons
- [x] The Scheduler is marked "coming soon" in the menu and the toolbar
      tooltip, instead of looking like a control that is merely greyed out
- [ ] Named queues, pick one from the dropdown
- [ ] Queue order and the drag handles
- [ ] Scheduler windows and schedule files (`s_%s.dt`, `q_%s.dt`)
- [x] Speed limiter: on, off, settings and options — the engine has the global and the per download cap
- [ ] Download limits per schedule, and the warning shown when a limit is hit
- [x] A cap on the connections of the whole application, and a cap per site

## Adding downloads

- [x] Compact Add URL dialog: the address, the file it becomes and the save folder
- [x] The dialog starts a real engine download and reports a failed start inline
- [x] URL validation beside the field: a missing scheme, a space and an
      unsupported scheme each say what to change
- [x] Read the real file name, size and resume support from the server
      headers; the name and category follow the server's answer
- [x] A refused check (sign-in, expired link, unreachable server, redirect
      loop) is a line of advice rather than a block
- [x] Double submits are ignored while a start is in flight, and a link that
      is already in the list is noted without being refused
- [x] The dialog reopens on the folder of the last download, then the
      configured default, then the system Downloads folder
- [x] A taken destination is the engine's question, not a second press:
      Rename, Replace, Skip or Cancel in a compact File Exists window
- [x] The conflict is settled again while the start is claimed, so a file
      that appears after the dialog opened, or a second start of the same
      download, is a question and never a silent overwrite
- [x] Rename picks the first free `name (2).ext` beside the file, keeping the
      extension and the name under the length the file system allows
- [x] Replace is offered only for a file no other download uses; a file a
      row still holds can only be saved under a new name
- [x] A finished download whose file appeared meanwhile is not overwritten:
      the transfer fails with a sentence that says exactly that
- [x] Redownload asks the same question without dropping the row first, so a
      cancelled question leaves the download where it was
- [x] Batch redownload asks per file and can apply one answer to the rest
- [x] Enter starts, Escape cancels and Ctrl+N opens the dialog from anywhere
      in the shell
- [x] Download all links from the browser: a compact selection window with
      category and type filters, an opt-in for page links, the app's folders,
      the queue and the connection count; links are sent in chunks of ten and
      every link is answered on its own (started, queued, already in the list,
      not downloadable, or refused with the app's sentence)
- [ ] Batch download, many URLs in one app dialog
- [ ] Add from clipboard and drop target

## Site Grabber

- [x] **Tasks → Run site grabber** opens a window of its own; the main list
      stays usable while a crawl runs
- [x] Start address with its own validation, depth 1–5, page budget, file
      groups and extra extensions, path exclusions, folder, queue and
      connections; the form remembers its choices
- [x] Breadth-first read of the start host and its `www.` twin only, each page
      once, with depth, page and file caps and a link cap per page
- [x] Root-relative, absolute and protocol-relative links, plus `src`,
      `srcset` and `<base href>`; comments, scripts and styles are never read,
      `rel=nofollow` links are offered but not followed
- [x] `robots.txt` per origin, with agent groups, wildcards, `$` anchors and a
      capped `Crawl-delay`; a 5xx, a 429 or a redirect of the file blocks the
      site instead of allowing it
- [x] Address policy: http(s) only, no credentials, no single-label or local
      names, no private, loopback, link-local, CGNAT, documentation,
      multicast or reserved ranges, no cloud metadata endpoints; IPv4-mapped,
      compatible and NAT64 IPv6 spellings are recognised
- [x] Every page and redirect is resolved and checked before the request; a
      redirect that leaves the site is not followed
- [x] 300 ms between pages, `robots.txt` delay respected up to 5 s, 20 s
      timeout, 2 MiB page cap, compressed or non-HTML pages reported
- [x] Files are offered, never fetched; a private file address is dropped with
      a log line
- [x] Live list with text and type filters, select shown and clear, a log of
      blocked, excluded, repeated and unsupported links; files start unticked
- [x] Stop notices the cancellation between chunks; closing the window leaves
      the crawl running and reopening it shows its snapshot
- [x] Selected files go through `actions::start` in chunks, with the existing
      queue, categories, duplicate handling and a free name instead of a
      replace; each file is answered on its own
- [x] Selected addresses are resolved again at hand-over, so a changed name is
      refused rather than sent to a private network
- [x] Rust tests: policy, robots, the link scanner, request limits, and a crawl
      against a loopback server covering depth, duplicates, exclusions, page
      limit, in-site and off-site redirects, a metadata redirect, a
      robots-forbidden page and cancellation
- [x] `pnpm check:grabber`: the window helpers, preferences and report
- [x] Limitations documented in `docs/SITE-GRABBER.md`: no JavaScript-rendered
      links, no charset conversion, no compressed pages, no sitemap, no crawl
      resume, DNS rebinding remains theoretical
- [ ] A crawl that can be paused and resumed, and a rendered-page fallback

## Download engine

- [x] Multi-connection range downloader with the chunk list it keeps
- [x] Resume from the JSON manifest, truncate to the recorded length
- [x] HTTPS and HTTP through the Tauri backend
- [x] The requests name themselves like a browser; a server
      that only serves browsers answered `403 Forbidden` before that
- [x] FTP: every range opens its own data connection and asks for the file with `RETR`
- [x] A proxy: HTTP or SOCKS, for the whole application
- [x] Connection count per download, with a default of 8
- [x] Crash-safe chunk list: the manifest is written to a temporary file and renamed
- [ ] File integrity check, chunk head comparison against the server
- [x] The chunk head check: a range that holds the head of the file is given back and fetched again, and the download goes on with one connection
- [x] The chunk list is checked before it is resumed from: ranges that overlap or are out of order are not resumed from
- [ ] Media splitting for MP4 DASH, HLS and MPEG-TS
- [x] A file whose size the server does not tell is downloaded with one connection
- [x] The destination volume is checked for free space before and during the download
- [x] A failure while the parts are joined leaves the destination alone
- [x] A download that is removed stops before it leaves the list
- [x] A download that was cancelled or failed starts again when it is resumed
- [x] The list of downloads survives a restart of the application
- [x] The engine is its own crate, without a window toolkit
- [x] A headless client for the engine, `crates/idm-cli`
- [x] A diagnostics log of what the downloads did
- [x] The resume state is written when the application closes

## Download progress window

- [x] A window of its own for one download, opened by a double-click on its
      row or by *Download progress window* in its menu
- [x] Download status tab: state, file size, downloaded bytes and percentage,
      transfer rate, time left, resume capability and the cap in force
- [x] A meter fed by `download://update`, indeterminate while the server has
      not told the size of the file
- [x] Speed limiter tab: a real per-download cap, in KB/s or MB/s, applied by
      the engine to every connection
- [x] A connection row per range, with the bytes it holds and its actual
      state: waiting, connecting, receiving, retrying, paused, stopped,
      complete, failed
- [x] Hide details and Show details, with the window resized to match
- [x] Pause, Resume and Cancel, disabled when the download cannot take them;
      a finished download offers Open file and Close instead of dead controls,
      and a stopped one has no second stop
- [x] A paused download whose server cannot resume from a range says Resume
      while its transfer is still open, and Restart once it has stopped
- [x] Closing the window leaves the download running; a row that leaves the
      list takes its progress window with it
- [x] The engine reports the ranges and their states with every update, and
      whether the file can be resumed once the probe has answered
- [x] A cancel wakes a range that is waiting on the pause control, and two
      runs of one download cannot overlap
- [x] A cap below one read's worth of bytes paces that read in pieces instead
      of waiting for a bucket that can never hold it
- [x] A file that starts on one range adds connections as that range falls
      behind, so a file of a few
      megabytes shows several live connections instead of one
- [x] A file whose size the server never told can be paused and resumed: the
      dropped response is no longer mistaken for the end of the file, which
      used to leave a short file reported as complete
- [x] A run claimed for a resume is covered by a stop that lands before the
      task starts, and a second resume after the file finished does not fetch
      it again
- [x] A stopped download reports its unfinished ranges as Stopped, not as
      Waiting; a completed download restored from the index says its parts
      were joined
- [x] The resume state is written every couple of seconds and on pause, stop
      and exit, instead of several times a second for the whole download; a
      crash costs at most a couple of seconds and the part files are truncated
      back to the recorded length
- [x] The theme, face and weight follow the Options window in every open
      window immediately (the progress and completion windows included), the
      application-wide speed limit is read live, and the theme is on the root
      before the first paint
- [x] Escape closes the window unless a control used it first, and the taskbar
      title names the file so two progress windows can be told apart
- [x] A Minimize to tray button beside the minimize button hides the window
      without stopping the download; every open progress window owns a tray
      icon of its own, labelled with the file name, and clicking it brings that
      window back (right-click: Show window / Close window)
- [x] The window takes its corners from the compositor alone: the content no
      longer draws a second, smaller radius inside the frame, so all four
      corners are the same shape on every Windows version
- [x] A failed attempt's message goes when the user retries or stops the
      download instead of describing the row that replaced it

## Dialogs and window furniture

- [x] About box
- [x] Options dialog shell, with the General page (startup preferences, browser capture list)
- [x] Remaining Options pages: Save to, Downloads, Connection and Proxy / Socks
      work and reach the engine; File types, Sites Logins, Dial Up / VPN and
      Sounds stay greyed out
- [x] Browser extension that hands a link to the application: the right click
      menu on a link, and taking over the downloads the browser starts; the
      link opens the add dialog with its address and name filled in
- [x] Download complete window of its own, in the reference download manager's words ("File ... download
      complete."), with Open file, Open folder and Close
- [x] "Show download complete dialog" switch on the Downloads page: the engine
      reads it before it opens the window, and the saved value is pushed at
      startup
- [x] The referer the browser hands over reaches the server: it is kept with
      the download, set as a default `Referer` header, restored with the index
      entry, and used again by Redownload and by the name probe
- [ ] Browser integration behind the General page: detection, Restart, Add browser, keys and context-menu editors
- [ ] Apply the startup and clipboard preferences through the operating system
- [ ] Scheduler dialog
- [ ] Registration and Order online
- [ ] Tell a friend, Tip of the Day, Tutorials, Grabber Help
- [x] Tray icon with the mark of the application, and "keep the application in
      the tray when the window is closed": the tray menu brings the window
      back, starts and stops the queue, and is the way out
- [ ] Tray icon in the three styles the reference download manager offers, and the tray only mode
- [ ] Font picker and language list
- [ ] Single instance behaviour and a tray-only mode

## Quality gates

- [x] `npm run check:palette`, 50 foreground and background pairs, all pass
- [x] `npm run check:find`, 8 checks: the walk, the wrap and the first press
- [x] `npm run check:progress`, 74 checks: the percentage and unknown sizes,
      the connection words and summary, the control states, the rate sampling
      (including the sample after a pause), and the speed-limit conversions
- [x] `tsc --noEmit` and a production build, both clean
- [x] `cargo clippy --all-targets -- -D warnings` on both Rust crates, clean
- [x] Smoke test of the rendered shell, categories pane and list
- [x] `cargo test` in `crates/idm-core`, 92 tests: the engine runs from end to end over HTTP and over FTP, resumes, refuses a broken chunk list, gives back a range that holds the head of the file, holds a real bandwidth cap, retries a range that answered `503`, pauses and resumes (including a file of an unknown size), releases a paused range when it is cancelled, refuses a stale resume,      does not run a finished download again, adds a second connection to a
      file that started on one range, reports stopped ranges as stopped, keeps
      a file that was not allowed to be replaced, puts a confirmed replace
      over the old file, and drops the parts and manifest on Start over
- [x] `npm run check:recovery`, 61 checks: the conflict as a question, the
      answers a running download rules out, the batch label, the start loop
      (a free name asks nothing, an answer that fails is reported instead of
      asked again, one answer covers a batch), and the sentence each engine
      failure becomes
- [x] `cargo test` in `src-tauri`, 130 tests: the bridge, the batch hand-over
      (offered folders, duplicates, categories, request shape), the completion
      window, the progress window's payload, label, label parsing, tray id and title,
      the destination question (a taken name, a free name beside it, a row
      restarted from its own file), the start request the window sends, and the
      Site Grabber (address policy and private ranges, robots groups and
      wildcards, the HTML scanner, limits, depth, duplicates, exclusions,
      redirects on and off the site, a metadata redirect, a robots-forbidden
      page, and cancellation against a loopback server)
- [ ] Keyboard-only pass over the whole shell
- [ ] Screen reader pass over the tree, the list and the listbox
- [x] `npm run check:grabber`, 75 checks: the address wording, rule and type
      parsing, preference round-trip and clamping, event folding, snapshot
      merging, filters, selection, chunking and the report
- [x] `npm run check:extension`: the manifest, every page reference, the shared
      tokens, the link rules and the time-sliced page scan, the background
      against a mocked bridge (hand-over, link scan and batch), the video
      overlay in a fake page, and the Chrome build's contract
- [ ] Unit tests for the store: filtering, sorting, queue transitions
- [ ] Visual check in a real window, done by hand

## Documentation

- [x] `ANALYSIS.md`, the reverse engineering notes and the interface strings
- [x] `PROGRESS.md`, this file
- [x] `docs/SITE-GRABBER.md`, the grabber's scope rules, defaults, tests and limitations
- [ ] A short note on what the clone is and is not, for the repository front page
