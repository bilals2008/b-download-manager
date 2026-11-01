# 13 — Themes and icon styles

Status: proposal. Nothing here is started. Two independent tracks: **A** widens
the appearance system past light/dark, **B** brings the `fun-isometric` sticker
kit into the app's icon artwork. They share one mechanism (semantic tokens) and
one gate (`prefers-reduced-motion`), but ship separately.

## What was verified before writing this

| Fact | Where |
| --- | --- |
| A theme is one token block: `:root` declares 82 custom properties, `:root[data-theme="light"]` overrides 75 | `src/index.css` |
| Components never ask which theme is active — 0 `dark:` variants across `src/components` and `src/state` | grep over both trees |
| The six icon palettes are inside the theme blocks, so category art already follows the theme | `src/index.css:340-357` (dark), `:528+` (light) |
| The per-variant block pattern already exists nine times over for fonts (`[data-font="geist"]`, `bricolage`, …) | `src/index.css:371-419` |
| Theme choice is a three-way union, not an id list | `src/lib/settings.ts:340` (`"auto" \| "light" \| "dark"`) |
| `applyTheme` only sets `data-theme`; `resolveTheme` returns a binary; both run before first paint | `src/lib/theme.ts`, `src/main.tsx` |
| Every window re-applies saved appearance and follows the settings broadcast, so a new theme propagates without new plumbing | `src/lib/appearance.ts` |
| The native window fill reads a token, not the theme id | `src/lib/window.ts:87` (`--background`) |
| Sidebar rows draw category glyphs at `size-5` = 20px, 16 glyphs in the picker | `src/components/workspace/categories-pane.tsx:280-317`, `src/lib/category-icons.ts:100-137` |
| The app already ships multi-style vector sets, in code not assets | `src/lib/toolbar-art-isometric.ts`, `-3d.ts`, `-classic.ts` |
| A toolbar pack carries inline **SVG** per button, never canvas | `docs/TOOLBARS.md`, format block |
| `fun-isometric` is a dependency-free TS kit: `fun.ts` 34KB, `motions.ts` 11KB, 24 worked icons, `Sticker.tsx` | `C:\Users\muham\.agents\skills\fun-isometric\kit` |
| Toolchain: pnpm project, bun 1.4.3 present, **ffmpeg absent**, **Playwright Chromium absent** (`~/.cache/fun-isometric` holds the npm package only) | checked on this machine |

---

## Track A — more than two themes

### Why this is cheap

The architecture is already the one AGENTS.md asks for: components read
semantic tokens, one block decides the values, nothing branches on the theme
id. Adding theme N+1 is data, not a refactor. The only real work is replacing
the hardcoded binary `"light" | "dark"` with a registry that still knows
*which family* each theme belongs to, because three places need contrast, not
identity: `resolveTheme` for `auto`, the pale-plate ink rule in
`src/lib/category-icons.ts:222`, and the native window fill.

### Shape

- One CSS block per theme in `src/index.css`, keyed `:root[data-theme="<id>"]`,
  same layout as the existing font blocks — no new build machinery.
- `src/lib/themes.ts` registry: `{ id, label, family: "light" | "dark", swatch }`.
  `family` replaces every current use of the `"light" | "dark"` union except the
  persisted preference itself.
- Preference stays open-ended: `"auto" | <any registry id>`. `auto` resolves to
  the OS default *within* the family the user last had under auto, so auto never
  lands on a novelty theme by accident.
- Themes live in `planning` terms as tokens-only: a theme may not add a
  component, a variant, or an asset.

### Phases

| Phase | Work | Gate |
| --- | --- | --- |
| A1 | Extract the registry, thread `family` through `theme.ts`, `settings.ts`, `category-icons.ts` | `pnpm build` + `pnpm tauri dev`: light/dark behave exactly as before, zero visual diff |
| A2 | Add `scripts/check-tokens.mjs`: every theme block defines the same property set as `:root`, reports missing and extra | Script passes on the 2 existing blocks; fails when one variable is deliberately removed |
| A3 | Theme picker in Settings with swatch previews, reusing the existing broadcast path | Switching themes repaints every open window without a reopen |
| A4 | New themes, one block each, 2-3 at most to start | Each passes A2 and a manual contrast pass on chrome text, focus ring, and the six icon palettes |
| A5 | Site + docs: `site/src/styles/tokens.css` parity note, `docs/DEVELOPMENT.md` "adding a theme" recipe | Docs describe the registry, not a copy-paste of hex values |

### Decisions still needing your input

Which themes (a warm-paper light, a neutral high-contrast, a deep-slate dark —
or brand-driven variants), how many, and whether the toolbar-pack `style`
object should inherit from the active theme when a pack omits colours (it does
today, and that is probably right).

---

## Track B — `fun-isometric` icon styles

### What the kit actually gives us

Stickers are drawn from shape builders on a 500-unit canvas as slabs with a
thick ink outline and an extruded side, animated by presets in `motions.ts`
(After-Effects-style keyframes, no Lottie player). Colour is never baked: each
fill names a palette slot (`main gold tint ok hot deep white grey paper ink`)
and a palette is supplied at render time, swappable live through
`player.set({ palette })` without restarting the motion. Reduced motion shows
`icon.still`. One rAF loop drives all stickers, off-screen ones are skipped,
and nothing touches the DOM before `play()`, so it is SSR-safe. That palette
API is what makes it compatible with Track A: a palette can be derived from the
live tokens and re-derived when the theme broadcast fires.

### The honest blocker: 20px

The kit's own acceptance bar is "the sheet reads at 160px", and its outline is
5 units on a 500-unit canvas. At the sidebar's `size-5` that is a 0.5px stroke,
and slab sides plus sparkles collapse into mush inside a tree row — before
counting that animated icons in a scrolling list are distracting and cost a
repaint per row per frame. So the sidebar rows are the wrong surface for these
stickers, and the plan reflects that.

### Shape: two tiers

**Tier 1 — style variants for the small sizes (SVG, in-app).** Extend the
existing glyph system the way the toolbar packs already extend the bar:
`IconStyleId` ("flat" as today, "slab" in the isometric look, "3d"), one entry
in `src/lib/category-icons.ts` and one drawing per glyph per style in
`src/components/icons/desktop-icons.tsx`. Stays inline SVG, so it inherits the
theme tokens for free, stays crisp at 20px, costs no runtime. `fun-isometric`
supplies the geometry language (leaned faces, per-part thickness, crease line
down the slab corner, ink outline) rather than the pixels — the repo already
proves this translation in `toolbar-art-isometric.ts`.

**Tier 2 — live stickers where there is room for them.** 120-200px surfaces:
the category icon picker's preview, Settings appearance/toolbar panels, empty
states, the welcome or About panel, and the marketing site's feature sections.
Copy `fun.ts`, `motions.ts`, `Sticker.tsx` into `src/lib/stickers/` (kit files
untouched, our `icons.ts` beside them, per the skill's setup note) and add a
`themePalette()` bridge that reads the token slots with `getComputedStyle` and
re-applies through `player.set()` on `onSettingsChanged`.

### Boundary worth stating now

Toolbar packs are JSON with inline SVG icons, so a slab-style pack can ship as
one `.json` with no app change — canvas stickers cannot enter a pack. If the
goal is a user-installable icon style, Tier 1 is the only path.

### Phases

| Phase | Work | Gate |
| --- | --- | --- |
| B0 | One-time toolchain: `bun add playwright` in the skill folder plus its Chromium (~170MB) for `fi check` / `fi sheet` / `fi frame` | `fi check` runs against the kit's own `icons.ts` |
| B1 | Proof-of-one: render a single sticker (a folder) in a scratch page at 20, 40, 96 and 160px, `fi sheet` beside the kit's worked icons | A decision, written down: which tier the sidebar actually gets |
| B2 | `IconStyleId` in the icon system + one style ("slab") across all 16 glyphs | Picker shows the style; theme toggle recolours it with no extra code |
| B3 | `Sticker` on one real surface (picker preview or an empty state), palette from tokens | Reduced motion shows `still`; theme switch swaps palette without a restart; no rAF work while off-screen |
| B4 | Extra styles, extra surfaces, an ADR for the tier split | `docs/DEVELOPMENT.md` records how to add a glyph in every style |

### Cost and fit

`fun.ts` + `motions.ts` are dependency-free TypeScript, roughly 15KB minified
and gzipped together, no sidecar and no asset download — consistent with the
existing "installer stays ~3.3MB, no ffmpeg/yt-dlp" decision. `fi export`
(alpha webm) is unusable here until ffmpeg exists on the machine, and it is not
needed: the app renders live canvas, and the sticker set is code.

---

## Risks

- **Bundling the kit** means vendoring third-party source into `src/`. Pin the
  skill version in the file header and keep kit files unedited so a future
  upstream `fun.ts` swap is a straight copy.
- **Two palettes for one idea**: `CATEGORY_PALETTES` (token triples) and the
  kit's slot palettes will drift unless `themePalette()` is the only bridge.
- **Animated art next to a data-dense tree** can read as toy-ish against the
  current soft-warm direction (`planning/05-ui-design-direction.md`). Tier 1
  exists exactly to keep the row density serious.
- Track A's binary→family change touches the pale-plate ink rule; if a future
  theme sits mid-luminance, that rule needs a threshold, not a family.

## Suggested order

Track A first: it is mechanical, low-risk, and its registry is the prerequisite
for any palette-driven sticker work. Then B0/B1 as a one-evening spike — the
20px render is the only genuinely unknown outcome in either track, and B1
settles it with evidence instead of an argument.
