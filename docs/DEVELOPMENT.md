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
cd src-tauri && cargo test
```

## Layout

```
src-tauri/src/
  main.rs        process entry
  lib.rs         Tauri builder, commands, event wiring
  downloader.rs  engine: probe, ranges, retries, pause, assembly
  segmenter.rs   byte ranges, part files, splitting
  state.rs       the set of known downloads
src/
  App.tsx        screen composition and command routing only
  main.tsx       React root
  types.ts       shared frontend types
  lib/           framework-free helpers (ipc, window, formatting)
  state/         React state containers
  components/
    shell/       title bar, menu bar, toolbar (one file each)
    workspace/   panes, tables, dialogs (one file each)
    ui/          shadcn/ui primitives
```

Rules:

- A new feature gets a new file or a focused module. Do not grow `App.tsx`,
  `lib.rs`, `downloader.rs` or `menu-bar.tsx` into a second layer of logic;
  extract instead.
- Rust: keep commands in `lib.rs` thin; engine behaviour stays in the engine
  modules.
- Frontend: no IPC or file-system work inside presentational components.
- No native `<select>`; use `components/ui/listbox.tsx`.
- Colours come from tokens in `src/index.css`. No raw hex or palette classes.

## Window chrome — the fix that must not regress

The native window decorations are off in `src-tauri/tauri.conf.json`
(`"decorations": false`) so there is exactly **one** title bar: the custom one
in `src/components/shell/title-bar.tsx`, labelled "Internet Download Manager
6.43". The window `title` in `tauri.conf.json` carries the same string for the
taskbar. Turning decorations back on, or renaming one of the two, brings the
duplicate bar back. Title-bar buttons call `windowAction` in `src/lib/window.ts`.

## Theme

Single source of truth: `src/index.css` (CSS variables + `@theme inline`), read
by `scripts/check-palette.mjs`. Warm paper/clay workspace, dark chrome family,
sage only for completed. Changing a colour means changing it there and
re-running `npm run check:palette`.

Theme references:

- shadcn/ui themes: https://ui.shadcn.com/themes
- tweakcn theme editor: https://tweakcn.com
- Tailwind CSS v4 theme variables: https://tailwindcss.com/docs/theme
- Radix Colors scales: https://www.radix-ui.com/colors
- Local design kit (tokens, taste, aesthetic systems):
  `D:/Dev/ux-ui-agent-skills/tokens/`, `D:/Dev/ux-ui-agent-skills/taste/`

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
