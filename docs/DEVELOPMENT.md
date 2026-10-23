# Development guide

Reference material moved out of `AGENTS.md`. Read this before larger changes.

## What this is

A multi-connection download manager modelled on IDM 6.43: a Tauri v2 (Rust)
backend in `src-tauri/` and a React 19 + TypeScript + Tailwind CSS v4 +
shadcn/ui frontend in `src/`. The reverse-engineering evidence behind the
engine is in `ANALYSIS.md`; the plans and ADRs are in `planning/`.

## Commands

```bash
npm install
npm run tauri dev        # desktop app (the only way to see real window chrome)
npm run dev              # UI only in a browser; IPC calls are no-ops
npm run build            # tsc --noEmit + vite build
npm run check:palette    # WCAG contrast gate for the colour tokens
cd crates/idm-core && cargo test   # the engine, without a window
cd src-tauri && cargo test         # the application
```

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
  bridge/        the loopback server the browser extensions talk to
    mod.rs         routes, the versioned contract and its tests
    http.rs        the HTTP/1.1 reader and writer
    dest.rs        safe, unique file names for hand-overs
browser-extension/
  manifest.json  the Chrome/Edge build
extension-firefox/
  manifest.json  the Firefox build; MV3 event page, video overlay, popup and settings
src/
  App.tsx        screen composition and command routing only
  main.tsx       React root
  types.ts       shared frontend types
  lib/           framework-free helpers (ipc, window, formatting)
  state/         React state containers
  components/
    shell/       title bar, menu bar, toolbar, the popup menu they share
    workspace/   panes, tables, dialogs, the context menus (one file each)
    windows/     the completion window, and progress/ for the live download
                 window (status tab, speed limiter tab, connections, meter)
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
in `lib.rs`, the download-complete window in `complete.rs` and each progress
window in `progress.rs`.

The frame owns the corners. Small windows must **not** put `rounded-lg` (or any
other radius) on their root: a second, different radius inside the compositor's
rounding shows a rounded inner border inside a square or differently curved
outer frame, which is exactly the mismatch the progress and completion windows
had. Keep their content square and let `round_corners` shape the window; on
Windows 10, where the API is a no-op, all four corners are square together.

## Theme

Single source of truth: `src/index.css` (CSS variables + `@theme inline`), read
by `scripts/check-palette.mjs`. Slate black chrome with the Windows accent blue
for actions, a neutral light appearance that follows the system, and green only
for finished work. Changing a colour means changing it there and re-running
`npm run check:palette`.

Surfaces are borderless: panes, bars and rows separate by surface colour and
spacing, not by divider lines. Radii are `rounded-lg` (12px) for panes and
popups, `rounded-chrome` (8px) for controls and rows, `rounded-sm` (6px) for
checkboxes. Floating things only (menus, popups, dialogs) get `shadow-float`;
panes are flat. The only visible borders left are on form fields and
checkboxes, where they are the affordance.

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
| `reverse-engineer-anything` | IDM analysis work (local) | `~/.agents/skills/reverse-engineer-anything` |
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
