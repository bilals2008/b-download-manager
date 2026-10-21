# UI design direction

## The direction

**Warm, soft, paper-and-clay.** A download manager is a tool you leave open
in a corner of the screen all day. It should feel like paper on a desk, not
like a dashboard in a control room.

Explicitly rejected:

- cool greys and the "developer tool" palette;
- purple and blue gradients;
- glassmorphism and glow;
- high-contrast neons;
- more than one accent colour.

The current palette lives in `src/index.css` and is checked by
`npm run check:palette`. That script is the gate: it reads the tokens out of
the stylesheet and fails if any pair the product actually renders drops below
its WCAG minimum.

## The palette

| Role | Token | Hex | Used for |
|---|---|---|---|
| page | `--background` | `#f7f3ec` | the app background |
| surface | `--card` | `#fdfbf7` | cards, the table |
| ink | `--foreground` | `#33291f` | body text, headings |
| secondary ink | `--muted-foreground` | `#6e6256` | labels, metadata |
| action | `--primary` | `#a94e36` | the primary button, the progress bar |
| on action | `--primary-foreground` | `#fdf8f2` | text on the primary button |
| soft surface | `--secondary` | `#f0e7db` | the selected filter pill |
| accent surface | `--accent` | `#e9e3d5` | hover states |
| finished | `--success` | `#4f6b4c` | the completed badge, and nothing else |
| danger | `--destructive` | `#b4514b` | cancel, delete, failure |
| hairline | `--border` | `#e5dccf` | table rules, card edges |
| control edge | `--input` | `#948572` | input borders, at 3:1 so the control is identifiable |
| focus | `--ring` | `#a94e36` | the focus ring |

### Rules the palette obeys

1. **One accent.** Clay is the only colour that means "act". Sage appears
   only where something finished. Nothing else is chromatic.
2. **The neutrals are warm.** Every neutral shares the same hue family as the
   clay, so nothing reads as a separate, colder layer.
3. **No gradients, anywhere.** Depth comes from a hairline border and, on the
   cards only, one very soft shadow.
4. **The accent is a clay, not a red.** The destructive colour is a separate,
   brick-toned value, so "cancel" never looks like "download".

## Type

| Role | Size | Weight | Tracking |
|---|---|---|---|
| Page title | 24 px | 600 | -0.02em |
| Card title | 16 px | 600 | 0 |
| Body | 14 px | 400 | 0 |
| Table head | 12 px | 500 | 0.04em, uppercase |
| Metadata | 12 px | 400 | 0 |

One family: the platform UI stack. No display face, no second family. The
tone comes from the colour and the space, not from the typeface.

## Space

A 4 px base, with these steps in use: 4, 8, 12, 16, 20, 24, 32, 48.

Vertical rhythm between the three regions of the screen — the form, the
filter bar, the table — is 24 px. Inside a card, 20 px. Inside a table row,
12 px.

The page is one column, centred, max 1152 px. It is a tool, not a document,
so it does not stretch.

## Radius

| Element | Radius |
|---|---|
| card | 12 px |
| input, button, badge | 8 px |
| progress bar | full |

## Surfaces and depth

- Page: `--background`, flat.
- Card: `--card`, 1 px `--border`, shadow `0 1px 2px rgb(51 41 31 / 0.04)`.
- Table row hover: `--muted` at 50%.
- Nothing else has a shadow.

## Motion

| Thing | Duration | Curve |
|---|---|---|
| progress bar fill | 200 ms | ease-out |
| hover and focus states | 120 ms | ease-out |
| badge and status change | 150 ms | ease-out |
| anything else | none | — |

`prefers-reduced-motion: reduce` removes the progress bar transition. There is
no spring, no bounce, and nothing that moves on its own.

## Component rules

**Button.** One filled clay button per view — "Start download". Everything
else is quiet: a ghost button for row actions, an outline button where a
border helps. Icon-only buttons carry an `aria-label` and a `title`.

**Input.** A visible border, a 12 px radius, placeholder in
`--muted-foreground`. Focus is a 2 px clay ring at 2 px offset. A label above
every field, always — no placeholder-as-label.

**Progress bar.** 8 px tall, `--secondary` track, `--primary` fill. The
percentage is written next to it, never inside it.

**Badge.** Status only. Clay for active, sage for finished, brick for failed,
outline for paused and cancelled. The label carries the meaning, so colour is
never the only signal.

**Table.** A hairline rule between rows, no zebra striping, no vertical rules.
The header is 12 px uppercase in `--muted-foreground`. Columns are sized to
their content except the name and progress columns, which take the slack.

**Empty state.** It owns the card, centred, with one sentence of guidance. It
is not a bare sentence at the top of a table.

## The one screen

```
┌──────────────────────────────────────────────────────────┐
│ my-idm-clone                                             │  title + one-line explanation
│                                                          │
│ ┌──────────────────────────────────────────────────────┐ │
│ │ New download                                          │ │
│ │ Source URL            │ Save to                      │ │
│ │ [                    ] │ [                           ] │ │
│ │ Connections [8]              [ Start download ]      │ │
│ └──────────────────────────────────────────────────────┘ │
│                                                          │
│ [All][Active][Finished]                  [ search      ] │
│ ┌──────────────────────────────────────────────────────┐ │
│ │ File        Size   Progress      Status  Streams  ⋯  │ │
│ │ …                                                     │ │
│ └──────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
```

The form is the only thing on the page that is filled. The table is the only
thing that moves. There is no third region, no settings dialog and no
sidebar — those come later, and only if the milestones say so.

## What is not claimed

No screenshot of this direction was captured in the environment it was built
in, so **the visual result is not verified**. What *is* verified:

- the contrast gate passes for all 11 pairs the product renders
  (`npm run check:palette`);
- the DOM renders with the expected tokens applied, the expected labels, and
  no emoji;
- the type check and the production build pass.

Before this direction is called done, it needs a look on a real screen.
