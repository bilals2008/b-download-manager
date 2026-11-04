# Development guide

Reference material moved out of `AGENTS.md`. Read this before larger changes.

## What this is

A multi-connection download manager modelled on the classic Windows download
manager: a Tauri v2 (Rust)
backend in `src-tauri/` and a React 19 + TypeScript + Tailwind CSS v4 +
shadcn/ui frontend in `src/`. The reverse-engineering evidence behind the
engine is in `ANALYSIS.md`; the plans and ADRs are in `planning/`.

## Commands

```bash
pnpm install
pnpm tauri dev           # desktop app (the only way to see real window chrome)
pnpm dev                 # UI only in a browser; IPC calls are no-ops
pnpm build               # tsc --noEmit + vite build
pnpm test                # frontend unit tests (Vitest + Testing Library)
pnpm verify              # check:all + typecheck + unit tests: the pre-push gate
pnpm check:all           # every self-check script, one summary at the end
pnpm check:release       # the release contract: versions, updater, bridge, notes
cargo test --workspace   # the app and the engine crate together
cargo test -p idm-core   # the engine only, without a window
cargo clippy --workspace --all-targets -- -D warnings
pnpm site:dev            # Astro docs site (site/)
```

The Rust side is one Cargo workspace (`Cargo.toml` at the root), so `target/`
and `Cargo.lock` live at the root, not in `src-tauri/`. `[profile.release]`
belongs in the root manifest — Cargo ignores profiles in member manifests.

The `check:*` scripts and `check-release-integrity.mjs` are read-only and do
not touch the network by contract; that is what lets CI run all of them on
every pull request. `bridge-probe.mjs` is the exception: it drives a running
desktop app, so it stays a local, manual check.

## Layout

```
crates/
  idm-core/       the engine: no window toolkit, no application
    downloader.rs   engine: probe, ranges, retries, pause, assembly
    http.rs         the wire format: range headers, Content-Range, retries
    ftp.rs          the FTP client: login, PASV, REST, RETR
    transfer.rs     where the bytes come from: HTTP or FTP
    verify.rs       the check that the data of a range belongs to that range
    limits.rs       connection budget, per site caps, speed limit, proxy
    disk.rs         free space, replacing a file safely
    segmenter.rs    byte ranges, part files, splitting
    session.rs      the index of known downloads
    log.rs          the diagnostics log
  idm-cli/        a headless client for the engine
src-tauri/src/
  main.rs        process entry
  lib.rs         Tauri builder, commands, event wiring
  state.rs       the set of known downloads
  staging.rs     the application's own storage for unfinished bytes
  migrate.rs     the start-up move of older layouts into the staging tree
  bridge/        the loopback server the browser extensions talk to
    mod.rs         routes, the versioned contract and its tests
    auth.rs        the origin and host rules, and their tests
    http.rs        the HTTP/1.1 reader and writer
    dest.rs        safe, unique file names for hand-overs
    batch.rs       the Download all links batch: offered folders, per-link answer
  video/         the media layer around the engine
    capture.rs     the bridge handlers, job parsing, the panel commands
    probe.rs       the manifest GET: redirect and size caps, origin headers
    download.rs    the coordinator: tracks, then mux::join, then the file
    mux.rs         join(inputs, output, container): the only remux entry
    policy.rs      the headers a page may forward, scoped to its origin
    ytdlp.rs       the optional last resort for an expired YouTube URL
    window.rs      the quality panel window (`video`)
  grabber/       the Site Grabber
    mod.rs         the one run, its commands, and the hand-over to the engine
    crawl.rs       the walk: scope, limits, robots, redirects, cancellation
    policy.rs      addresses, private ranges, exclusions, file types, names
    robots.rs      robots.txt groups, rules and crawl delay
    extract.rs     the tolerant link scanner
    window.rs      the grabber window
browser-extension/
  manifest.json  the Chrome/Edge build
extension-firefox/
  manifest.json  the Firefox build; MV3 event page, video overlay, popup and settings
  lib/link-rules.js  what a link is, shared by the reader and the link window
  lib/manifest.js    what a stream manifest is, and the playback headers
  content/page-links.js  the time-sliced reader of a page's links
  links/         the Download all links window (filters, selection, report)
src/
  App.tsx        screen composition and command routing only
  main.tsx       React root
  types.ts       shared frontend types
  lib/           framework-free helpers (ipc, window, formatting)
  state/         React state containers
  components/
    shell/       title bar, menu bar, toolbar, the popup menu they share
    workspace/   panes, tables, dialogs, the context menus (one file each)
    windows/     the completion window, progress/ for the live download
                 window (status tab, speed limiter tab, connections, meter),
                 video/ for the quality panel (rows, polling hook), and
                 grabber/ for the Site Grabber (form, results, hook)
    ui/          shadcn/ui primitives
```

Rules:

- A new feature gets a new file or a focused module. Do not grow `App.tsx`,
  `lib.rs`, `downloader.rs` or `menu-bar.tsx` into a second layer of logic;
  extract instead.
- Rust: keep commands in `lib.rs` thin; engine behaviour stays in the engine
  crate, and the engine must not know about windows — it reports through the
  `Reporter` trait.
- Frontend: no IPC or file-system work inside presentational components.
- No native `<select>`; use `components/ui/listbox.tsx`.
- Colours come from tokens in `src/index.css`. No raw hex or palette classes.

## Where the bytes live

The folder the reader chooses holds finished files and nothing else. Every
temporary of a transfer — the ranges, the manifest, the join temporary — stays
inside the application's own storage, so the download folder is clean while a
transfer runs.

```
<app data>/staging/
    downloads/<id>/       one folder per unfinished download:
                          its .part ranges and its .idm.json manifest
    media/<job>-<name>/   the tracks and join files of a video job
<volume root>/.bdm-staging/   the same, on the volume of a destination
<app log dir>/idm-clone.log
```

Rules:

- The staging tree is **application data**, not cache. A resumable download
  must survive a restart and must never be cleared by a cache purge.
- A start refuses when the tree cannot be made. Writing the ranges into the
  reader's folder instead is never the fallback.
- The tree is not a setting. There is one place for it, resolved in `setup`,
  and the window has no way to change it.
- A destination on another volume gets a folder on that volume, so the
  finished file is put in place with a rename instead of a copy across a
  disk. A volume that cannot hold the folder falls back to the one beside the
  application data.
- Folders are hidden (the hidden flag, where the platform has one). Files are
  not: a file with that flag cannot be opened for writing on some systems,
  and the copy still has to fill it. A temporary file therefore carries a
  leading dot (`.name.idmmove`) instead.
- The staging folder of a download is named by its id and is removed with the
  download: when it finishes, when it is cancelled and forgotten, and when a
  start-up sweep finds a folder no row claims.
- `migrate.rs` moves the resume state of older releases into the tree at
  start-up: the bytes are **copied** and measured, the index is written, and
  only then does the old copy go. A move that cannot be verified leaves the
  download exactly where it was, still resumable.

## Window chrome — the fix that must not regress

The native window decorations are off in `src-tauri/tauri.conf.json`
(`"decorations": false`) so there is exactly **one** title bar: the custom one
in `src/components/shell/title-bar.tsx`, labelled "B Download Manager". The
window `title` in `tauri.conf.json` carries the same string for the taskbar. Turning decorations back on, or renaming one of the two, brings the
duplicate bar back. Title-bar buttons call `windowAction` in `src/lib/window.ts`.

`core:default` does **not** include window dragging or the control buttons. The
capability in `src-tauri/capabilities/default.json` must keep
`core:window:allow-start-dragging`, `allow-minimize`, `allow-toggle-maximize`,
`allow-hide` and `allow-close`, or the title bar silently stops dragging and
the buttons do nothing. Double-click maximize is handled by the drag region; do
not add a second `onDoubleClick` toggler, it cancels itself out.

With decorations off the window has square corners, so
`src-tauri/src/window_style.rs::round_corners` asks the compositor for rounded
ones (Windows 11 only, and a no-op elsewhere). It is called for the main window
in `lib.rs`, the download-complete window in `complete.rs`, each progress
window in `progress.rs` and the Site Grabber in `grabber/window.rs`.

A window label must also be in `src-tauri/capabilities/default.json`. The
grabber is `grabber` and the quality panel is `video`; without them, dragging
and the close button do nothing.

The frame owns the corners. Small windows must **not** put `rounded-lg` (or any
other radius) on their root: a second, different radius inside the compositor's
rounding shows a rounded inner border inside a square or differently curved
outer frame, which is exactly the mismatch the progress and completion windows
had. Keep their content square and let `round_corners` shape the window; on
Windows 10, where the API is a no-op, all four corners are square together.

## Theme

Single source of truth: `src/index.css` (CSS variables + `@theme inline`), read
by `scripts/check-palette.mjs`. Six appearances: the deep green-black Dark
base, the neutral Light, and four fixed looks — Cyberpunk (near-black with a
cyan accent), Volt (ink-black with a teal cast and lime accent), Soft Black
(one warm neutral black, off-white text and a monochrome action) and Ayu Dark
(blue-black with Ayu's warm gold accent). Green only
for finished work and red only for destruction in every appearance. Changing a
colour means changing it there and re-running `pnpm check:palette`, which
merges the base block with each `:root[data-theme="…"]` block and checks the
WCAG pairs in all six.

Adding an appearance takes five places, or it will silently fall back to the
system light/dark look:

- `src/index.css`: a `:root[data-theme="…"]` block with the full token set.
- `src/lib/settings.ts`: the `ThemePreference` union and the `THEME_PREFERENCES`
  list the loader validates against.
- `src/lib/theme.ts`: `isLightTheme` when the new canvas is pale, so the tray
  icon picks the right inks.
- `index.html`: the pre-paint `fixed` list, which mirrors the saved ids.
- `scripts/check-palette.mjs`: the `APPEARANCES` list, so the gate fails when a
  block or one of its pairs is missing.

The Appearance page lists the appearances in `appearance-settings.tsx`.

Surfaces are borderless: panes, bars and rows separate by surface colour and
spacing, not by divider lines. Radii are `rounded-lg` (12px) for panes and
popups, `rounded-chrome` (8px) for controls and rows, `rounded-sm` (6px) for
checkboxes; a fixed theme may change `--radius` (Cyberpunk is sharp). Floating
things only (menus, popups, dialogs) get `shadow-float`; panes are flat. The
only visible borders left are on form fields and checkboxes, where they are the
affordance.

Every window of the application is a separate document: the main window, each
progress window, and the completion window. `src/main.tsx` calls
`startAppearanceSync()` in `src/lib/appearance.ts` before the React root, which
applies the saved theme, face and weight and then follows two sources of change:
the operating system when the theme is Auto, and the `settings://changed` event
that the Options window broadcasts (`src/lib/app-events.ts`). Do not apply a
theme, a face or a weight in one window on its own. `index.html` carries a small
pre-paint copy of the theme decision so a light window does not open with a dark
flash; keep it in step with `src/lib/theme.ts`.

Theme references:

- shadcn/ui themes: https://ui.shadcn.com/themes
- tweakcn theme editor: https://tweakcn.com
- Tailwind CSS v4 theme variables: https://tailwindcss.com/docs/theme
- Radix Colors scales: https://www.radix-ui.com/colors
- Local design kit (tokens, taste, aesthetic systems):
  `D:/Dev/ux-ui-agent-skills/tokens/`, `D:/Dev/ux-ui-agent-skills/taste/`

## What's New

`WHATS_NEW.md` at the project root is the only reader-facing release record.
It is imported into the bundle at build time (`src/lib/whats-new-content.ts`),
so the popup works offline and the words cannot change after signing. The
technical `CHANGELOG.md` stays for maintainers and CI; it is never shown in
the app.

- Add a `## x.y.z — date` section before every release, newest first. The
  release script refuses to tag a version without one, and the release page
  uses the same words.
- `src/lib/whats-new.ts` parses the file and decides whether to show it. A
  machine with no saved choices at all is a first install and only records the
  version: there is no earlier release to describe. A machine that already has
  the app's other saved choices is an install from before this feature, so the
  current version's notes are offered once. The same version never opens
  twice; a newer version opens once. The seen version lives in
  `localStorage` under `idm-clone.whats-new.v1` and is written when the notes
  actually open, so a restart cannot repeat them and an offer deferred by a
  running download is tried again on the next quiet launch.
- `useWhatsNew` waits for the download list to load and for every transfer to
  stop, and stands down when another dialog is open. On a launch that has
  fresh notes, the startup tip and the automatic update check do not open;
  this keeps startup to one window instead of a stack.
- Settings > About, the About box and Help > What's new call `openManually`,
  which ignores the dismissal record and is allowed during a download.
- Keep the box a reader's page: the app mark, version, day, short groups and
  one line per change. No cards, tags, rules or developer vocabulary. The
  self-check rejects markdown marks, developer words and overly long lines.

## Download progress window

- `src-tauri/src/progress.rs` owns the window: its label, its fixed size and
  the payload that hands it the download before the first render.
- The page follows `download://update` and filters on its own id
  (`src/state/use-live-download.ts`); the engine already throttles those events.
- Which controls can act is decided by `progressControls()` in
  `src/lib/progress.ts`, not by conditions scattered through the markup. Add a
  state there and cover it in `scripts/check-progress.mjs`.
- Hide/Show details asks the backend to resize the window; the page never
  assumes a height it set itself.
- The tray button hides the window (it does not close it): `hideWindow()` needs
  `core:window:allow-hide`.
- Every progress window owns a tray icon (`progress::add_tray`), created after
  the window and removed from the `Destroyed` handler in `lib.rs`. Its ID is
  `progress-tray-<id>`, its tooltip is the window title, and right-clicking it
  offers **Show window** / **Close window**. Keep the icon while the window is
  hidden — it is the way back — and remove it only when the window is really
  destroyed. The application's own tray icon in `lib.rs` stays a separate
  thing; do not rebuild it for progress windows.

## Tray icon

The application's own icon, its menu and the choices that go with them. Keep
these invariants when changing it:

- The tray choices live in `desktop.json` (`src-tauri/src/prefs.rs`), not in
  `localStorage`: the icon, the close button and the clipboard watch all work
  before a page has read anything. The window reads them with
  `desktop_preferences()` and writes each one with its own command.
- `tray::set_visible()` is the only place that puts the icon up or takes it
  down, and it is the only place that sets `tray_available`. The close and
  minimize paths consult that flag and fall back to closing and minimizing
  when it is false — never hide a window the reader cannot bring back.
  `tray::refresh()` returns early while the icon is away, so a hidden icon does
  not turn every progress report into a warning.
- The pair of drawings comes from `scripts/make-icons.py`, which packs
  `src-tauri/icons/source.png` — the brand logo — into the app icons and into
  `tray-on-dark.png` (the logo under a light rim) and `tray-on-light.png` (the
  logo as it is). Change the logo, run the script, and `tray::drawing()` keeps
  its two arms in step. The tray is the same picture as the app icon, never a
  second interpretation of it.
- The window owns the appearance and reports it with `set_tray_appearance`
  (from `src/lib/appearance.ts`, so every window keeps the icon in step). The
  backend ignores the report unless `trayDarkMode` is on; the default drawing is
  the dark one, which is what the icon has always used.
- The clipboard watch matches a copied address against `urlProtocols`. The
  cleaning rules are written twice on purpose — `clean_url_protocols` in
  `prefs.rs` and `normalizeUrlProtocols` in `src/lib/tray.ts` — and
  `scripts/check-tray.mjs` (`pnpm run check:tray`) fails when the two drift
  apart. An empty list falls back to the three defaults: a watch with nothing
  to match offers nothing.

## Site Grabber

The grabber is the only part of the application that reads pages it was not
handed. Keep these invariants when changing it:

- Every page and every redirect goes through `policy::check_public`, which
  checks the spelling **and** resolves the host. Do not add a path that
  requests a URL without it. The loopback allowance in `crawl.rs` is
  `#[cfg(test)]` only and covers loopback exclusively.
- The crawl only offers candidates. It never fetches a file; the hand-over in
  `grabber/mod.rs` calls `actions::start` with `Choice::Rename`, so a grabber
  download is an ordinary download and a taken name is never replaced.
- Scope is the start host plus its `www.` twin. Subdomains and other hosts are
  not crawled. `robots.txt` is fetched per origin, obeyed, and a failure to
  read it blocks the origin rather than allowing it.
- The limits are constants in `policy.rs` (depth, pages, page size, file count,
  interval, redirects, robots size). Do not let the window raise them past
  those.
- No cookies, credentials, sign-in, paywall or DRM logic belongs here. A page
  that asks for a session is reported, not worked around.
- The window must not tick files by default. Reading a site and downloading a
  site are separate actions on purpose.
- The client is built with `redirect(Policy::none())`; redirects are followed
  by `crawl.rs` so each hop can be checked. Turning on automatic redirects
  would skip the SSRF checks.

`docs/SITE-GRABBER.md` is the user-facing contract; update it with any rule or
limit that changes.

## Settings persistence

Who owns what, and where it is written. Keep these invariants when a setting
is added:

- **`settings.json`** (`src-tauri/src/settings_store.rs`) is the engine's own
  versioned file: proxy, speed cap, connection counts, the browser hand-over
  folder and connection count, the extension's capture rules, the video-button
  appearance, the unfinished folder and category folders, and the
  complete/progress/clipboard switches. It is loaded and applied once during
  setup — **before the bridge binds** — so a hand-over arriving at start-up
  meets the reader's settings, not the defaults. A proxy stored there never
  carries a username or password: a credential is stripped on both write and
  read (`sanitize`, `strip_credentials`), matching `validate_proxy`, which
  refuses one.
- Every write to these goes through `AppState::modify_settings`, which
  sanitizes, applies the whole object to the running engine, and saves the
  file. Never set a runtime field and a file field separately; never read the
  runtime field to decide what the file says.
- The webview keeps a cache of the engine's values in `localStorage`
  (`idm-clone.settings.v1`). At start-up `syncEngineSettings()`
  (`src/lib/settings.ts`) reads `saved_settings()` and copies the file into the
  cache. `saved_settings()` returns null when there is **no file yet** — a
  first run or an upgrade — and then the webview's copy is the seed: the boot
  pushes create the file, freeing no user value. After that the file wins.
  Never seed from defaults when the file is absent.
- `desktop.json` (`src-tauri/src/prefs.rs`) stays the owner of the tray, the
  close and minimize choices and the clipboard protocols — same rules, its own
  file. The engine settings file does not duplicate it.
- There is no pairing file and no secret to copy. The bridge binds loopback
  only, accepts the `Host` it owns, requires `X-BDM-Client: browser-extension`,
  and refuses any `Origin` that is not an extension's. That is the whole door:
  install the app and the extension and they work together.
- Appearance, fonts, toolbar packs and layouts, tips, the last folder, the
  update-check stamp and the release the reader put off are read only by the
  webview and live only in `localStorage`. Pick the owner by who reads it
  without a window: if the engine, the tray, or the bridge needs it, it
  belongs in one of the two backend files; otherwise `localStorage` is enough.

## Settings search

The field at the top of the options window filters a prepared catalogue, not
the DOM. `src/lib/settings-search.ts` holds the pure index/search/group code;
`src/components/settings/settings-search-data.ts` is the one catalogue of
names, hints, synonyms and anchors. The index is built once at module load
and reused for every query, and the query is deferred so typing never waits on
a result render. Nothing here asks the engine or the network, so there is no
loading state to fake.

When a setting is added or renamed:

- Give its `SettingRow` (or the `Section` for a whole group) a `settingId` that
  matches an entry `id` in the catalogue.
- Add or update that entry's title, description and keywords. Keep the words
  a reader would actually use: "bandwidth" beside "speed", "autostart" beside
  "startup".
- The catalogue test (`settings-search-data.test.ts`) fails when an entry has
  no rendered anchor or a duplicate id, so a broken jump cannot land silently.

Shortcuts come from `src/lib/shortcuts.ts`, which both the Shortcuts page and
the catalogue read; add a key there once. A result opens the page, scrolls the
row into view, focuses it and leaves a short ring (`use-setting-reveal.ts`).

## Media capture


The browser extension hands over a manifest or a playback address; the app
parses it, shows the quality panel, downloads each track, and remuxes. Keep
these invariants:

- Parsing lives only in `crates/idm-core/src/media/`. The engine downloads
  tracks; it does not know what a video is. A track is one ordinary download
  per segment, with the existing resume state.
- Every remux goes through `video::mux::join(inputs, output, container)`.
  V4a runs an optional LGPL `ffmpeg` sidecar with `-c copy`; a native writer
  may replace the body later without touching a caller. A transcode is a bug.
- DRM is refused, never touched: `#EXT-X-KEY`, MPD `ContentProtection`,
  `application/vnd.yt-ump` and a `pssh`/encrypted init all produce one
  sentence. Live (`no #EXT-X-ENDLIST`, `MPD@type="dynamic"`) is refused too.
- No cipher solver and no `n` solver. YouTube uses the signed URL the page
  already requested; an expired URL is one `yt-dlp` attempt if the optional
  binary exists, else one readable failure.
- Headers are accepted only for `Cookie`, `User-Agent`, `Referer`,
  `Origin` and `Authorization`, only on the manifest's own origin, never
  persisted, never logged. A cookie or authorization value waits for the
  one-time consent in the panel (`desktop.json`).
- Firefox draws the quality list **inline** in its overlay, the way the
  reference's Download Panel does. The `video` window remains for clients with
  no overlay (the Chrome build) and lists the same pairings. The list follows
  listbox semantics, never a native `<select>`; the last quality per host is
  remembered in `desktop.json`.
- A started media job is a row in the home list, driven by `media_jobs`. It
  carries live progress, Stop and Remove; the engine's `Download` commands are
  never sent for a media row.

## Skills

These are already installed globally (`~/.agents/skills`) and load on demand.
Do **not** install a second copy or a near-duplicate of any of them.

| Skill | Use it for | Source |
|---|---|---|
| `tauri-v2` | config, commands, IPC, permissions, packaging | https://skills.sh/nodnarbnitram/claude-code-extensions/tauri-v2 |
| `tauri-development` | app structure, window and build workflow | https://skills.sh/mindrally/skills/tauri-development |
| `rust-best-practices` | ownership, error handling, engine refactors | https://skills.sh/apollographql/skills/rust-best-practices |
| `desktop-app-design` | data-dense desktop layout, keyboard-first UX | https://skills.sh/phazurlabs/ux-ui-mastery/desktop-app-design |
| `design-engineer` | component polish, states, motion, QA | https://skills.sh/farming-labs/design-eng-skills/design-engineer |
| `shadcn` | primitives, composition, registry | https://skills.sh/shadcn/ui/shadcn |
| `tailwind-design-system` | token and utility structure | https://skills.sh/wshobson/agents/tailwind-design-system |
| `better-ui` | visual detail, hover/focus, micro-interactions | https://skills.sh/jakubkrehel/skills/better-ui |
| `better-accessibility` | focus, keyboard, ARIA, screen readers | https://skills.sh/jakubkrehel/skills/better-accessibility |
| `security-and-hardening` | untrusted URLs, file paths, IPC boundaries | https://skills.sh/addyosmani/agent-skills/security-and-hardening |
| `project-structure-enforcer` | keeping files small and modular (local) | `~/.agents/skills/project-structure-enforcer` |
| `spec-driven-development` | planning engine milestones | https://skills.sh/addyosmani/agent-skills/spec-driven-development |
| `reverse-engineer-anything` | reverse-engineering analysis work (local) | `~/.agents/skills/reverse-engineer-anything` |
| `code-review` | reviewing a branch before merge | https://skills.sh/mattpocock/skills/code-review |
| `debugging-and-error-recovery` | failing tests and broken downloads | https://skills.sh/addyosmani/agent-skills/debugging-and-error-recovery |
| `performance-optimization` | large-file throughput and UI jank | https://skills.sh/addyosmani/agent-skills/performance-optimization |
| `frontend-ui-engineering` | production-quality React work | https://skills.sh/addyosmani/agent-skills/frontend-ui-engineering |
| `documentation-and-adrs` | `planning/decisions/` entries | https://skills.sh/addyosmani/agent-skills/documentation-and-adrs |
| `deslop` | removing generated-code noise before commit | https://skills.sh/brianlovin/claude-config/deslop |

Optional, not installed (only add if one of the above proves insufficient):
`tauri-window`, `tauri-ipc`, `tauri-build` from
https://skills.sh/full-stack-skills/tauri-skills.

See https://skills.sh/ to search for anything else.
