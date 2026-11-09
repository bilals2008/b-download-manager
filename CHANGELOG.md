# Changelog

All notable changes to B Download Manager will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Sections are generated from the conventional commits each release carries;
edit one before its release ships if a bullet needs better words.

---

## [1.1.1] - 9 November 2026

### Added

- **extension:** Follow the app theme
- **video:** Install yt-dlp like Mister Download Manager does
- **video:** Keep a yt-dlp the person downloaded when the release host fails

### Changed

- Satisfy rustfmt across the workspace
- **whats-new:** Describe 1.1.1 for readers

### Fixed

- **release:** Read a changelog with Windows line endings
- **video:** Satisfy clippy in the helper lookup
- **video:** Resume a stopped download from its bytes
- **video:** Repair downloads, merging and the browser panel


## [1.1.0] - 8 November 2026

### Added

- **extension:** Capture the media the browser's players fetch
- **appearance:** Open a light system in Bloom and paint a swatch per theme

### Changed

- **video:** Satisfy rustfmt in the YouTube planner
- **video:** Rustfmt download.rs and satisfy clippy
- **whats-new:** Describe 1.1.0 for readers
- **media:** The live YouTube download mirrors the panel's referer
- Measure the plans against the source and keep the record in English
- **categories:** Cover the folder each category saves into
- Bump upload-artifact to v7 for the node24 runtime

### Fixed

- **video:** Recover YouTube downloads with request leases and strict remux
- **media:** Download YouTube tracks with the identity the URL was signed for
- **video:** Fall back to the ANDROID_VR player client and log failed re-resolves
- **categories:** Save downloads in the folder of their category like the reference manager


## [1.0.0] - 6 November 2026

### Added

- **theme:** Add Graphite, Bloom and Hive, one token file per appearance
- **video:** Own YouTube resolution and native remux, drop yt-dlp and ffmpeg

### Changed

- **release:** Add the 1.0.0 notes

### Fixed

- **changelog:** Give the leftover commits one Changed heading


## [0.3.2] - 4 November 2026

### Added

- **ui:** Tighten the tooltip and give it its own shadow
- **site:** Add a QuickPrompt card to the docs rail and lighten pagination
- **update:** Brand the installer and show real update progress
- **storage:** Keep unfinished downloads out of the user's download folder
- **ui:** Redesign settings and tips, add the Ayu Dark theme

### Changed

- **ui:** Drop the nested borders and compact the complete window
- **settings:** Compact the settings pages
- **release:** Add the 0.3.2 notes

### Fixed

- **extension:** Show the video button for a pointer already on the player
- **download:** Save across volumes, restart a changed file, keep a silent track
- **extension:** Read page qualities and keep the overlay panel open

## [0.3.1] - 2 November 2026

### Added

- **site:** Point Firefox users at the published extension

### Fixed

- **release:** Keep the ffmpeg sidecar out of the macOS bundle

## [0.3.0] - 2 November 2026

### Added

- **settings:** Add instant search across every settings page
- **appearance:** Replace mica with a soft black theme
- **appearance:** Add selectable cyberpunk, volt and mica themes
- **bridge:** Connect the browser extension without a pairing code
- **bridge:** Pair the extension with a code and refuse every other caller
- **settings:** Persist engine settings in a versioned file
- **extension:** Rework the options page
- **video:** Join tracks with a pinned ffmpeg sidecar
- **queue:** Start downloads one at a time in a saved order
- **whats-new:** Show friendly notes once after an update
- **tray:** Add icon styles, hide, dark mode and the clipboard URL list
- **extension:** Show stream qualities in the browser overlay and popup
- **media:** Capture HLS and DASH with a quality window

### Fixed

- **window:** Create progress and grabber windows off the IPC thread
- **video:** Keep the ytdlp test import on Windows only
- **video:** Gate Windows-only test helpers behind cfg(windows)
- **video:** Clear the two clippy failures CI treats as errors

### Changed

- Copy only the Linux payload files for verification
- Verify the Linux bundle from one flat artifact folder
- Make the settings and verify checks platform independent
- Satisfy the ffmpeg sidecar check in the compile jobs
- **extension:** Check icon sizes and Firefox manifest keys
- Run every workspace test and verify releases before publishing
- **agents:** Keep a change in one commit
- **agents:** Forbid co-author trailers in commits
- **planning:** Record the media research, muxer decision and theme plan
- **site:** Document the tray icon choices
- Drop browser verification rule from AGENTS.md

## [0.2.0] - 31 October 2026

### Added

- **toolbar:** Five built-in styles, a Customize window, size and label switches, and JSON packs with per-style layouts
- **settings:** A Toolbar page for styles, size, labels and pack management
- **share:** Tell a Friend opens the system share sheet, with the clipboard as fallback
- **site:** A toolbar feature page with previews and a guide to installing and making packs

### Changed

- **icons:** Refined the gloss on category icons

---

## [0.1.3] - 30 October 2026

### Added

- **settings:** Pick the save folder for each category and turn the browser's video download button on or off
- **engine:** Unfinished parts now stay in the list until you remove them, and each category saves to its own folder
- **icons:** Known file types draw with the bundled Material Icon Theme artwork
- **ui:** Release notes read as plain lines instead of commit subjects
- Removing a download now clears its parts from disk and closes the windows it left open

---

## [0.1.2] - 29 October 2026

### Added

- **icons:** Draw file icons as pages with a type band
  - A known extension now draws the way a desktop draws it: a light page with a folded corner and a coloured band carrying a white mark, one shape per family (video, audio, archive, program, pdf, spreadsheet, slide, image, code, font) and a plain ruled page for text. An unknown type keeps the category glyph, and a saved category icon still wins.
- **site:** Put real screenshots on the landing page
  - The hero and the four feature rows now show the app itself, and the browser hand-off and tray rows are gone until those parts can be photographed.
- Add a tip of the day and a startup update check
  - The Help menu and the first start share one cursor, so a tip already read does not come back, and a tip never opens over the add dialog. The one update check the reader agreed to runs after the window is up.

### Fixed

- **scripts:** Keep chore commits out of the release notes
  - A release commit (chore: release vX.Y.Z) landed in the next version's notes as a raw subject. Chore is housekeeping, so it never reaches the reader.
- **ci:** Stop the release commit from skipping its own build
  - A tag push starts no workflow when the tagged commit says [skip ci], so v0.1.1 shipped a tag the Release job never saw. The release commit drops the marker and release.yml gains workflow_dispatch for a re-run from the tag.
- **scripts:** Pass release git arguments as an array
  - ExecFileSync wants an argv array, so the clean-worktree check and the branch lookup threw ERR_INVALID_ARG_TYPE before any release could start.

## [0.1.0] - 27 October 2026

First release.
