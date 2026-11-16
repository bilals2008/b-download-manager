# AGENTS.md

## Most important rule

For unfamiliar features, don't write from scratch. First search GitHub for
open-source projects that already solve it, compare the approaches, then get
started.

---

## Workflow

`.agents/WORKFLOW.md` is the workflow: plan in waves, implement the pieces in
parallel, keep every source file under 800 lines, use the skills, and prove the
change on the running application.

- Read `.agents/WORKFLOW.md` before starting anything that touches more than
  one file.
- A source file may not grow past **800 lines**. `pnpm check:file-size` fails
  the build when it does; split the file first, moving the code rather than
  rewriting it.
- Work that does not depend on each other is done in parallel, one agent per
  piece, one set of files per piece.
- A bug is not fixed until the running application has been driven through the
  loopback bridge (`pnpm probe:bridge`) and the behaviour has been seen.

---

## Build discipline

Repeated agent builds once grew `target/` to **70 GB** (311 stale incremental
caches). `Cargo.toml` now settles it in the profile instead, which is the part
that cannot be forgotten: the dev profile keeps line tables only, dependencies
keep no debug info at all, and `.pdb` files stop at the app. Do not repeat the
old workaround. The disk is the reader's, not a scratch pad.

- Verify with the smallest command that proves the change: `cargo test -p
  <crate>` or `cargo clippy -p <crate>` while iterating. Run `cargo test
  --workspace` and `pnpm verify` **once, at the end** — not after every edit.
- **Do not prefix builds with `CARGO_INCREMENTAL` or `CARGO_PROFILE_DEV_DEBUG`
  any more.** Each different value starts a new incremental cache beside the
  old one, and a pile of half-used caches is what made `target/` huge. One
  profile, the same command, one cache, reused.
- If `target/` has been left on old settings and is still huge after a profile
  change, `pnpm clean` once — the next build starts from nothing but the new
  profile.
- **Never run two cargo builds at the same time** in this workspace: they
  fight over the target lock and `link.exe` fails with a bogus "build tools
  missing" error. Wait for one to finish before starting the next.
- `pnpm tauri build` only when the running app must actually be driven (bridge
  probe, UI proof). If the change does not touch what the app binary uses,
  reuse the binary that is already in `target/release`.
- After a small fix, re-run the targeted command — cargo reuses what is
  built. Do not start a fresh full build, and do not `cargo clean` mid-task.
- If `target/` is already huge, report the size and let the reader decide
  about a clean; never delete it unasked.
- Build scratch binaries and examples only when they are about to be used,
  and remove scratch files when the task ends.

---

Project-specific rules for coding agents. Details: `docs/DEVELOPMENT.md`.
Organize files properly: Keep folders and files logically structured by feature and responsibility.
- Keep folders and files well-organized, modular, and maintainable.
- Use skills for UI/UX and for the desktop-app feel.
- Never put the entire application logic in one file. Separate concerns into appropriate modules and reusable components.
- Build reusable components, hooks, utilities, and shared logic wherever appropriate. Avoid code duplication and follow consistent naming conventions.
- Use the shared `Tooltip` (`src/components/ui/tooltip.tsx`) instead of native `title` for icon-only controls and truncated text.
- Use the `find-skills` skill to discover and apply relevant skills for this project. Check already-installed skills and never install or add duplicates.
- Use semantic design tokens for colors, typography, spacing, borders, and other design properties. Never hardcode design values in components; define and reuse centralized tokens.
- Add relevant theme reference URLs and useful resource links to `AGENTS.md`.
- Follow the existing project structure, conventions, and tech stack. Avoid unnecessary files, dependencies, and architectural changes.
- Prioritize clean architecture, maintainability, performance, accessibility, and consistency.
Also, structure the design system  to support multiple themes in the future. Switching themes should instantly update the entire UI, including backgrounds, colors, borders, and other design tokens. Use centralized semantic CSS variables and theme configurations to make adding or changing themes easy without modifying individual components.

## Problem solving

- When a problem is hard to diagnose or the fix is unclear, research it before
  writing code. Do not guess, and do not settle for an outdated method.
- Use the latest stable, recommended approach. No hacks, temporary fixes or
  workarounds.
- For anything uncertain, check official sources first: Tauri, Rust crate docs,
  React/Tailwind docs. Official documentation beats random tutorials.
- Confirm the version in use before copying a solution from the web.

## React

-Avoid useEffect: Do not use React's useEffect by default. Prefer event handlers, derived state, and proper data flow. Use it only when genuinely necessary.

## Commits

- Never add a `Co-Authored-By` trailer, an "Generated with" line, or any other
  attribution to a commit. Strictly no co-authors. One line, English,
  conventional prefix (`feat(scope): ...`), and no commit body.
- Commit a change as one commit. Do not split a working tree into a series of
  small per-layer commits; separate commits are only for work that is genuinely
  unrelated.

## Design slop indicators

Do not ship these. They are the tells of generic AI-generated UI:

- Tags (pill badges, label chips used as decoration)
- Italicized words
- Instrument Serif with negative kerning (or any "editorial" font pairing)
- Over-the-top detailed UI
- Dividers (decorative rules between sections)
- Many color shades and opacities (keep one accent, one neutral ramp)
- Containers with borders (nested cards, bordered panels inside bordered panels)
- Diagrams (the more detailed, the worse)
- Code breadcrumbs (`provider / model / version` paths in UI copy)
- Section numbering (01, 02, 03 labels)

## What's New

- `WHATS_NEW.md` is the single source of truth for reader-facing release
  notes. The in-app popup and the GitHub release body both read it.
- Before every release, add a `## x.y.z — date` section at the top with short,
  plain sentences a non-technical user understands. Never copy commit
  subjects or `CHANGELOG.md` text into it, and never show developer words in
  the popup.
- Run `pnpm check:whats-new` after editing. The release script refuses a
  version with no section.
- The startup popup shows once per newer version and never interrupts an
  active download. Settings > About and Help > What's new must always reopen
  the notes manually.

## Links
 
- Design tokens: `src/index.css`
- What's New content: `WHATS_NEW.md`
- Toolbar pack format: `docs/TOOLBARS.md`
- shadcn/ui themes: https://ui.shadcn.com/themes
- tweakcn theme editor: https://tweakcn.com
- Tailwind CSS v4 theme variables: https://tailwindcss.com/docs/theme
- Radix Colors: https://www.radix-ui.com/colors
- Tauri v2 docs: https://v2.tauri.app/
- Release, installer and update notes: `docs/RELEASES.md`
- Tauri updater config: https://v2.tauri.app/plugin/updater/
- Skill registry: https://skills.sh/
