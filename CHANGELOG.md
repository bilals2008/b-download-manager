# Changelog

All notable changes to B Download Manager will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Sections are generated from the conventional commits each release carries;
edit one before its release ships if a bullet needs better words.

---

## [0.1.0] - October 27, 2026

### Added

- **grabber:** Add the Site Grabber window with a run form and a results list
- **extension:** Add the page link window and a chunked batch hand-over
- **grabber:** Crawl a site on request and offer the files it links to
- **site:** Rework landing page around the reference download manager's page flow
- **categories:** Add custom icons and content-aware category removal
- **ui:** Add editable properties and contextual file actions
- **ui:** Add managed categories and status filtering
- **actions:** Add safe move, queue, and delete operations
- **core:** Allow category and description updates independently
- **media:** Enhance media handling for streaming sources and improve user prompts
- **extension:** Follow the app-owned capture policy
  - Add GET /v1/capture and make both extensions merge the app's capture, ask, file-type and skip-site rules over their local copy while it runs, falling back to the local rules when the app is closed. Update the extension checks and READMEs.
- **engine:** Apply saved settings at startup and on change
  - Push proxy, speed cap, connection limits, tray, progress and completion dialogs, clipboard watch, startup entry, browser folder and capture rules to the engine when the app starts and when they change. Add the clipboard watcher, progress-on-start, folder and proxy checks, verified browser status, app paths and a guarded reset. Use the saved connection count and duplicate policy when a download starts, and seed the add dialog from a clipboard or browser link.
- **settings:** Rebuild options into grouped pages
  - Replace the tabbed window with a compact grouped sidebar and eight pages: General, Appearance, Downloads, Save locations, Browser integration, Connections, Proxy and Advanced. Add shared setting rows, rule lists, number fields that can be cleared, and honest planned copy. Remove the placeholder Planned and Network tabs.
- **settings:** Add the settings model and IPC for every page
  - Extend the saved preferences with startup, clipboard, progress-window, browser capture and remembered-folder fields, with validation and rule caps. Add the IPC contracts for folder checks, proxy validation, app paths, browser status and the engine-owned settings. Add a self-check for the model.
- **site:** Build the landing page and rewrite the docs for users
  - - Landing page in the app's palette, with self-hosted Bricolage and Instrument Sans, and a persisted light/dark switch. - Docs carry the same palette, faces, logo and favicon instead of the Starlight defaults. - User docs rewritten in plain words; build and extension details moved to one page for developers. - Compact previous/next links, and the app's dev server no longer restarts when the site is built.
- **progress:** Give the download meter a state gradient and one shared fill map
- **engine:** Refuse to join bytes from a changed file and clear orphan parts
- **transport:** Send If-Range and tell a transient failure from a changed file
- **settings:** Rebuild the settings window as tabbed pages with network and planned pages
- **recovery:** Resolve destination conflicts and add retry, redownload and move
- **engine:** Keep the Replace answer and never overwrite a file that appeared meanwhile
- **add-download:** Derive the file name, explain a failed link check and default to Google Sans
- **ui:** Add a shared tooltip and replace the native titles across the app
- **windows:** Reveal the small windows after paint and follow the theme background
- **progress:** Draw a block per connection and unfold the details with the window
- **add-download:** Rebuild the dialog around the probed link, its checks and the note
- **backend:** Support queued starts, destination checks and windows revealed after paint
- **engine:** Keep the category and note of a download and refuse a host-less URL
- **extension:** Align the Chrome build with the shared loopback hand-over
- **progress:** Open a live progress window with tabs for status and speed limit
- **engine:** Report range progress and honour per-download speed caps
- **add-download:** Preview the file type, name and size before saving
- **toolbar:** Dim commands the selection cannot use and drop the duplicate bar
- **extension:** Add the Firefox build with shared icons and a self-check
- **app:** Find bar, keyboard list walk, animated categories pane, fixed type scale and a Settings window
- **settings:** Pick the face, weight and size, applied live
  - The options window now writes every change as it is made: no Save, no Cancel, just Close. The settings the engine owns are pushed to it once typing settles, so a proxy address is not sent on every keystroke. - Eight self-hosted faces, a three-step weight scale and three text   sizes, each a token on the root that the theme block references, so   switching is one attribute and nothing is compiled in. - The options window sits on the same canvas as the main window, as two   rounded panes, and scrolls without drawing a bar. - Addresses and folders are set in JetBrains Mono, where columns matter. - The small captions in the categories pane are tokens now, and no   arbitrary text size is left in the app. - Hints are one line each; the ones that only restated a label are gone. - check:fonts guards the face, weight and size wiring. src/index.css no longer ends in 108 NUL bytes, which is why its diff reads as a rewrite.
- **workspace:** Auto-filled downloads, a native folder picker and a real context menu
  - - the address fills the file name and the category until they are changed, and a link without a name asks the server for one - Save to folder opens the folder dialog of the system - the row menu carries icons, a danger Remove, copy address and properties, and names how many rows it acts on - a read-only properties window follows the download while it is open - the category listbox paints inside the dialog, so it is no longer clipped
- **app:** The tray icon, and the browser hands its links over
  - The application keeps a tray icon now. Clicking it brings the window back, its menu starts and stops the queue, and it is the way out of the application: closing the window only hides it, the way the reference download manager does. The switch is on the General page of the options window. The icon is the mark of the application, drawn by scripts/make-icons.py, so the task bar and the tray show the same arrow. A click in the browser can start a download here. The extension in browser-extension/ hands the address of a link to the application, which listens on 127.0.0.1:27500: the right click menu on a link, and the downloads the browser starts are taken over, the way the reference download manager's advanced integration does. A page in the browser is not let in, only the extension is. The store file also carries the folder tree that was being worked on, so the bridge listener and that work went into one commit.
- **settings:** The options window works
  - The pages of the configuration window are real now: Save to, Downloads, Connection and Proxy / Socks open, and the three settings the engine owns — the proxy, the bandwidth cap and the number of connections — are pushed to it when the window is saved and when the application starts. The pages that need a subsystem this build does not have stay greyed out. The add dialog was rebuilt around the address: the file name is taken from it when the box is left empty, the line at the bottom shows where the file will land, and the button that starts the download is the one accent-coloured action in the window. The engine says in words what a refusal means: a 403 tells the user that the link may have expired or that the site only answers a browser.
- **ui:** The context menus of a row and of a category
  - A right click on a download opens the menu of the task: Resume, Pause, Stop, Redownload, Remove, Open file and Open containing folder. The row under the pointer joins the selection first, so the menu acts on what the user sees. The menu of a category selects every download it holds; the entries that need editable categories stay greyed out. The popup menu moved out of the menu bar into shell/menu-popup.tsx, so the bar and the context menus share one keyboard behaviour and one look. It also learned radio rows, which is how the View menu ticks the toolbar size. Open file and Open containing folder call the new open_path command, which hands the path to the shell of the system. The Large and Small Buttons entries of the View menu now really change the toolbar, and the choice is kept between sessions.
- **ui:** Accept ftp addresses in the add download dialog
  - The engine fetches over FTP as well as over HTTP, so the address the dialog accepts has to say so.
- **cli:** A headless client for the engine
  - The engine does not need a window, so it can be driven from a terminal:     idm-cli <url> [destination] [--connections N] [--proxy URL] The client does the same work as the desktop application: the file is split into ranges, the ranges are fetched in parallel, and the parts are joined when they are all there. The progress is written to the terminal. It is also the tool the connection measurements are made with.
### Changed

- **backend:** Format the Rust sources touched by the grabber work
- **list:** Draw the download list on the thin themed scrollbar
- **extension:** Rebuild the Firefox build on shared modules and a page overlay
- **theme:** Add the Bricolage Grotesque face and settle appearance before first paint
- **bridge:** Split the loopback bridge into modules and share download actions
- **fonts:** Drop the import font feature and the custom-face storage
- **settings:** Rebuild the settings pages around label-and-control rows
- **categories:** Quieten the section labels and modernise the disclosure chevron
- **complete:** Rebuild the completion window around the saved file
- **theme:** Add Google Sans Flex and lift the dark canvas off black
- **brand:** Rename the app to B Download Manager and drop the Registration menu
- Borderless rounded chrome with a Windows-like palette
  - Surfaces, bars and rows separate by surface colour and spacing instead of dividers, and everything carries a radius: 12px for panes, menus and dialogs, 8px for controls and rows, 6px for checkboxes. Only things that float (menus, popups, dialogs) get a shadow. The palette is the neutral slate black of the Windows shell with the Fluent blue for actions; the light appearance is the matching neutral light, so both follow the system without a lime or green cast. Green is left for finished work and red for destructive actions. The frameless window asks the compositor for rounded corners (window_style.rs), on Windows 11 only.
- No em dashes in the comment of the options window
- **engine:** The engine in its own crate, with HTTP and FTP sources
  - The engine does not know about windows: it reports the state of a download through a Reporter, and the desktop app and the headless client each hand it one. That is what lets the whole suite run without a display. The engine is now crates/idm-core, and the application in src-tauri is a thin shell over it. The protocol work that came with it: - FTP: one data connection per range, RETR for the file, REST for the   offset of a range that does not start at the first byte. - A proxy: HTTP or SOCKS, for the whole application. - The chunk head check: a range that holds the head of the file is given   back, and the download goes on with one connection. - The chunk list is checked before it is resumed from: ranges that overlap   or are out of order are not resumed from. - The number of connections a file gets: one per   10 KB of the file. - A diagnostics log, and the resume state written when the app closes. The rules the analysis established are in ANALYSIS.md, section 10.
### Fixed

- **windows:** Keep dialogs, fold animations and completion windows in step
  - Subscribe the tray listener once, give each progress window its own fold token, close a stale completion window with its row, and start dialogs on the control that matters. Share one dialog action button across every footer.
- **extension:** Stop the options page echoing its own save and read the page media first
- **engine:** Report a finished download as fully downloaded
- **engine:** Stop a paused or cancelled download mid-stream
  - A range that was already streaming kept reading until the response ended, so a pause or a cancel looked like it did nothing until the next chunk arrived. The transfer loop now checks the control flags and drops the response instead, and Control reports whether it is paused.
- **engine:** Name the client like a browser
  - A server that only serves browsers answered 403 Forbidden to the video links of Google. The User-Agent the reference download manager sends was read out of the string table of the reference binary and every request now goes out with it. The test the_request_carries_the_browser_user_agent covers it. The list stays minimal as well: the row opens nothing under it any more, the panel with the address, the destination and the counters is gone, the progress is a number with a percent sign the way the reference download manager shows it, and the error of a failed download sits under its name. ANALYSIS.md gained the third pass: the User-Agent, the corpus of the reference binary's own method names, the function that starts the optional connections, and the hand-over of a tail to a new connection, which confirms what the clone already does.
- Make the custom title bar draggable and wire its window controls
### Changed

- Document the Site Grabber and the page link window
- Add self-checks for categories and download properties
- Refactor site components and styles for improved layout and accessibility
  - - Updated Header component to enhance navigation with mobile menu support and improved styling. - Revamped Hero component to include platform icons and a new app preview feature. - Removed HowItWorks component and replaced it with a Download component for better clarity. - Added new PlatformIcon component for platform representation. - Expanded Icon component to include additional icons for better functionality. - Adjusted CSS styles for palette and tokens to refine visual hierarchy and responsiveness.
- Add video registry and overlay tests for Firefox extension
  - - Included `video-registry.js` in the Firefox extension manifest. - Updated `check-extension.mjs` to assert the injection order of content scripts. - Created `check-overlay.mjs` to test the behavior of the video overlay in various scenarios, including YouTube player interactions and video loading. - Implemented a fake DOM in `fake-dom.mjs` to simulate a browser environment for testing.
- **repo:** Switch to pnpm workspaces and scaffold the Astro documentation site
- Remove redundant close-button and table-header tooltips
- Record the recovery actions and the destination questions
- Document the add dialog checks and the new window commands
- Record the progress window, the speed cap and the extension rebuild
- Rewrite the README around the current application
- Update AGENTS.md with file organization guidelines and React usage recommendations
- Add download complete window, browser handoff, its off switch and referer
- Add row selection, batch delete confirmation and theme switch
- **planning:** The work that is left, item by item
  - A list of everything that is still open, in the order it makes sense to build it, with what has been done ticked: - the engine: media splitting, the file signature check, multipart byteranges,   RTMP, a queue with priorities, the fuzzed manifest reader, the soak test - the protocols: the addresses that skip the proxy, FTP over a proxy, FTPS - the product around the engine: the tray, the browser integration, the   named queues and the scheduler, the remaining options pages, the context   menus, the import and export, the first run - quality: the keyboard pass, the screen reader pass, the tests of the   store, the check in a real window, CI, the signed installer, the   connection sweep on a link that is not throttled - the reverse engineering: the split arithmetic, the method names bound to   their routines, the multipart path, the media splitting in the binary
- Write down what the engine does now
  - - ANALYSIS.md gains a second pass: the chunk list that is rebuilt when a   download resumes and the 16 byte record it is made of, the check of a   chunk against the head of the file, the number of connections a file gets,   and the "chunk size" that turned out to belong to RTMP and not to the   HTTP downloader. The difference from the reference download manager about the part files is written   down too, with the reason. - README.md describes the engine as it is, the commands it takes, and what   the measurements said. The measurement found a fault of its own: a server   that is busy answers 429 with a body of one byte, and the probe read that   body as the size of the file. - PROGRESS.md and the milestones say what is done and what is not. - DEVELOPMENT.md points at the new layout.
- Add settings dialog and refine the download workspace
  - - Add the General settings modal with themed checkboxes, page rail, local preferences, and toolbar/menu entry points. - Fix dialogs rendering while closed by applying the flex layout only to open dialogs. - Refine the download list, category pane, palette gate, and shared error handling.
- Require semantic tokens ready for multiple themes
- Slim AGENTS.md and move details into docs/DEVELOPMENT.md
- Add AGENTS.md with layout, skills and theme references
- Initial Tauri download manager with a single custom title bar
