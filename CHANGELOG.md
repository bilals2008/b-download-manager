# Changelog

All notable changes to B Download Manager will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Sections are generated from the conventional commits each release carries;
edit one before its release ships if a bullet needs better words.

---

## [Unreleased]

### Added

- **media:** HLS, MPEG-DASH and plain-file capture with a quality window that pairs the picture with its sound
- **media:** Segment downloads resume through the existing engine, then remux with `-c copy` behind one `mux::join` call
- **media:** Origin-scoped cookies and authorization, forwarded only after one consent prompt and never stored
- **media:** YouTube uses the signed URL the page already fetched; an expired URL falls back to an optional `yt-dlp`
- **media:** DRM, live streams and protected init segments are refused with one sentence
- **media:** Firefox shows the quality list on the page and in the toolbar popup, like the reference Download Panel, instead of opening an app window
- **media:** A started media download appears in the home list with live progress, Stop and Remove
- **ui:** The floating Download button loses the permanent progress ring and sits on the video corner with a soft shadow
- **ui:** The tray icon menu works: three drawings of the mark, "Don't show", inks that follow the appearance, and a URL list for the clipboard watch

---

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
