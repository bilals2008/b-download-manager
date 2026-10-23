# IDM clone — work in progress

Everything the rebuild still owes, in the order it makes sense to build it.
Done items stay in the list as a record of what is already standing.

## Shell (title bar, menu bar, toolbar)

- [x] Title bar with app mark, window text and the three window buttons
- [x] Menu bar with IDM's real menus, nested submenus, Alt+key access, hover switching
- [x] Toolbar with 11 hand-drawn SVG icons, roving tabindex, tooltips
- [x] Toolbar and menu items that have no screen yet are greyed out instead of dead
- [x] Toolbar button size setting (Large / Small Buttons) actually changes the icons
      — the size is kept between sessions and the menu ticks the one in use
- [ ] Toolbar customization dialog, icon picker, drag to reorder

## Categories pane (the sidebar)

- [x] Tree with the IDM groups All Downloads / Unfinished / Finished
- [x] The eight IDM categories: General, Video, Music, Documents, Programs, Games, Compressed, Other
- [x] Live count per entry, counts follow the download state
- [x] Click an entry filters the list, selection survives a filter change
- [x] Arrow keys, Home and End walk the tree, roving tabindex
- [x] View > Hide categories takes the pane off the screen
- [ ] Add, rename and delete a custom category (IDM: Add category, Editing category properties)
- [ ] Per-category default folder, file type rules and site rules
- [ ] Persist the tree state, IDM stores it under `Software\DownloadManager\FoldersTree`
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
- [x] The error of a failed download shows under its name, the way IDM shows
      it in the row
- [x] Delete key on a selection, Shift+click for a range
- [x] Arrow keys, Home and End walk the list, Shift+arrow extends the range
- [ ] Inline edit of file name and save path
- [x] Context menu on a row with the IDM entries: Resume, Pause, Stop,
      Redownload, Remove, Open file, Open containing folder, Properties
- [ ] Column chooser and drag to reorder (IDM: Customize URL List, Columns)
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
- [ ] Import and export of an IDM export file or a text file

## Queue and scheduler

- [x] One queue, driven by the two toolbar buttons
- [ ] Named queues, pick one from the dropdown like IDM does
- [ ] Queue order and the drag handles
- [ ] Scheduler windows and schedule files (`s_%s.dt`, `q_%s.dt`)
- [x] Speed limiter: on, off, settings and options — the engine has the global and the per download cap
- [ ] Download limits per schedule, the warning IDM shows when a limit is hit
- [x] A cap on the connections of the whole application, and a cap per site

## Adding downloads

- [x] Add new download dialog with URL, file name, category listbox and save path
- [x] The dialog starts a real engine download and reports a failed start inline
- [x] The category picker is an ARIA listbox, not a native select
- [x] Validation on the URL, inline error message
- [x] Read the real file name and size from the server headers; the category
      follows the name the server gives
- [ ] Batch download, many URLs in one dialog
- [ ] Add from clipboard, drop target, site grabber

## Download engine

- [x] Multi-connection range downloader with the chunk list IDM keeps
- [x] Resume from the `.idm.json` manifest, truncate to the recorded length
- [x] HTTPS and HTTP through the Tauri backend
- [x] The requests name themselves like a browser, the way IDM does; a server
      that only serves browsers answered `403 Forbidden` before that
- [x] FTP: every range opens its own data connection and asks for the file with `RETR`
- [x] A proxy: HTTP or SOCKS, for the whole application
- [x] Connection count per download, IDM's default of 8
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
      behind, the way IDM's alternative connections do, so a file of a few
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

- [x] About IDM box
- [x] Options dialog shell, with the General page (startup preferences, browser capture list)
- [x] Remaining Options pages: Save to, Downloads, Connection and Proxy / Socks
      work and reach the engine; File types, Sites Logins, Dial Up / VPN and
      Sounds stay greyed out
- [x] Browser extension that hands a link to the application: the right click
      menu on a link, and taking over the downloads the browser starts; the
      link opens the add dialog with its address and name filled in
- [x] Download complete window of its own, in IDM's words ("File ... download
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
- [ ] Tray icon in the three styles IDM offers, and the tray only mode
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
- [x] `cargo test` in `crates/idm-core`, 87 tests: the engine runs from end to end over HTTP and over FTP, resumes, refuses a broken chunk list, gives back a range that holds the head of the file, holds a real bandwidth cap, retries a range that answered `503`, pauses and resumes (including a file of an unknown size), releases a paused range when it is cancelled, refuses a stale resume, does not run a finished download again, adds a second connection to a file that started on one range, and reports stopped ranges as stopped
- [x] `cargo test` in `src-tauri`, 17 tests: the bridge, the completion window, and the progress window's payload, label, label parsing, tray id and title
- [ ] Keyboard-only pass over the whole shell
- [ ] Screen reader pass over the tree, the list and the listbox
- [ ] Unit tests for the store: filtering, sorting, queue transitions
- [ ] Visual check in a real window, done by hand

## Documentation

- [x] `ANALYSIS.md`, the reverse engineering notes and the interface strings
- [x] `PROGRESS.md`, this file
- [ ] A short note on what the clone is and is not, for the repository front page
