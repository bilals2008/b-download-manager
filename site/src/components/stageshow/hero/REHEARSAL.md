# Rehearsal: download-hero

Gold: `C:\Users\muham\.agents\skills\stageshow\examples\download-hero` (kept: scene geometry, tokens; changed: imports `../kit`, full-workflow story, mounted as Astro island then filmed to video).
Beats: pointer opens Add URL → link types into the field (lime caret, focus ring) → Add commits it and a new row arrives Downloading with speed + time left → row selected → Stop holds it Paused on a blue meter, Resume lights lime → click resumes, meter walks to the end → row lands Completed with the green check → pointer steps off. ~16s.
Kit parts added for the story: `Popover`, `Field`, `Icon` copied verbatim from the skill kit into `kit/` (trimmed barrel re-exports Field/Popover).
Captured: widths 1136 and 390; states add-open/added/selected/stopped/resumed/done; reduced motion shows the completed poster.
Gate: rehearsal passed on both widths (contact sheets filmed at 1136 and 390, every beat read, scratch sheets removed after review). Fixed on the way: popover ink padding (medium Add button with icon), clicks aimed at icons not labels, compact window bleeds off the bottom (near-square whole window flagged), compact status/pct fixed widths (no neighbour hop on word swap), arrival wrapper `flex: none` (flex shrink was crushing the new row to 0) with the row faint while the opening edge passes.
Accepted lines: typing keystroke jumps (discrete by nature), chip pop scale at done, same-box value/status swaps (the app's own in-place geometry).
Assumptions: Astro SSR renders the initial list; `client:load` hydrates; the capture pages play once per load.
Not checked: lighthouse; live-site dark mobile screenshot beyond the verify suite.

## Shipped as video (2026-11-16)

The landing hero no longer mounts the React island: `Hero.astro` embeds
`HeroVideo.astro`, a real `<video autoplay muted loop playsinline>` cut from
this scene, so playback no longer depends on hydration or scroll-into-view.

- Recorder: `site/scripts/record-hero.mjs` (`pnpm record:hero`, dev server +
  ffmpeg + Edge; playwright-core borrowed from the stageshow skill, nothing
  installed). Drives the kit's virtual clock frame-by-frame at 30fps, 2x
  device scale, on `/capture/hero[-compact][-dark]` — bare pages at the exact
  design size with the theme's page ground baked in. Videos keep the native
  2x pixels (wide 2240x1080, compact 716x660) so retina stays crisp.
- Cuts: `public/videos/hero-{wide,compact}-{light,dark}.{mp4,webm}` (~100-170 KB
  each) + `public/images/hero-poster-{wide,compact}-{light,dark}.webp`.
  The ending dissolves into the opening over 0.6s, so `loop` has no jump.
- `HeroVideo.astro` picks the cut by viewport (759px seam, matching the
  stage's compact breakpoint) and `data-theme`, swaps on change, and shows
  the poster instead of playback under `prefers-reduced-motion`.
- Verified on the running site with `site/scripts/verify-hero.mjs`
  (`--shots` for screenshots): desktop light/dark play and loop, runtime
  theme switch swaps the cut, mobile plays the compact cut, reduced motion
  shows the poster, no console errors — 19/19 checks passed.
- The scene itself is unchanged; re-record after editing it. The dev toolbar
  is hidden during capture so it never bakes into a frame.