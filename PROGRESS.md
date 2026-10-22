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
- [ ] Delete key on a selection, Shift+click for a range
- [ ] Inline edit of file name and save path
- [x] Context menu on a row with the IDM entries: Resume, Pause, Stop,
      Redownload, Remove, Open file, Open containing folder; Properties stays
      greyed out until there is a properties window
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
- [ ] Find (Ctrl+F) and Find next (F3) over the list
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
- [ ] Read the real file name, size and category from the server headers
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

## Dialogs and window furniture

- [x] About IDM box
- [x] Options dialog shell, with the General page (startup preferences, browser capture list)
- [ ] Remaining Options pages: File types, Save to, Downloads, Connection, Proxy / Socks, Sites Logins, Dial Up / VPN, Sounds
- [ ] Browser integration behind the General page: detection, Restart, Add browser, keys and context-menu editors
- [ ] Apply the startup and clipboard preferences through the operating system
- [ ] Scheduler dialog
- [ ] Registration and Order online
- [ ] Tell a friend, Tip of the Day, Tutorials, Grabber Help
- [ ] Tray icon: 3D style, classic style, don't show, dark mode support
- [ ] Font picker and language list
- [ ] Single instance behaviour and a tray-only mode

## Quality gates

- [x] `npm run check:palette`, 45 foreground and background pairs, all pass
- [x] `tsc --noEmit` and a production build, both clean
- [x] Smoke test of the rendered shell, categories pane and list
- [x] `cargo test` in `crates/idm-core`, 58 tests: the engine runs from end to end over HTTP and over FTP, resumes, refuses a broken chunk list, and gives back a range that holds the head of the file
- [ ] Keyboard-only pass over the whole shell
- [ ] Screen reader pass over the tree, the list and the listbox
- [ ] Unit tests for the store: filtering, sorting, queue transitions
- [ ] Visual check in a real window, done by hand

## Documentation

- [x] `ANALYSIS.md`, the reverse engineering notes and the interface strings
- [x] `PROGRESS.md`, this file
- [ ] A short note on what the clone is and is not, for the repository front page
