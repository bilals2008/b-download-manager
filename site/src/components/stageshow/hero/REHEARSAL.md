# Rehearsal: download-hero

Gold: `C:\Users\muham\.agents\skills\stageshow\examples\download-hero` (kept: scene geometry, tokens, story; changed: imports `../kit`, mounted as Astro island).
Beats: pointer clicks paused row → Resume lights lime → click turns row to Downloading (speed + time left) → meter walks to end → row lands Completed with green check → pointer steps off. Loop 8s.
Captured: widths 1136, 390; looks dark; states selected/running/done; reduced motion shows poster.
Gate: rehearsal passed (wide `shots/dl-done.png`, compact `shots/dl-compact3.png`).
Film: tight-inset notes on `p.dl-label "Downloads"` (7.5px) accepted — filler bleeds only.
Assumptions: Astro SSR renders initial paused state; `client:load` hydrates; IntersectionObserver plays once on view.
Unresolved: stage below the fold does not auto-play in a short viewport; plays on scroll-into-view.
Not checked: mobile dark-theme screenshot on the live site; lighthouse.