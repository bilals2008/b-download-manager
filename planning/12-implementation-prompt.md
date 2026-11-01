# 12 - Handoff prompt (paste this into the implementing agent)

Copy everything inside the block. Nothing else is needed; the plan and the
research are already in this repository.

---

```text
You are implementing media capture (HLS / DASH / YouTube) in this Tauri + React +
Rust download manager. Work in the repository as it is; do not restructure it.

Read first, in this order:
1. AGENTS.md and docs/DEVELOPMENT.md - house rules, they win over everything below
2. planning/11-media-download-plan.md - the work order: contracts, phases V0-V7, gates, risks
3. planning/10-video-capture-research.md - verified evidence for how the reference product behaves
4. planning/04-engine-spec.md and planning/07-verification-plan.md - the engine it must fit into

Work strictly phase by phase, in this order: V0, V1, V2, V3, V4, V5, V7, V6.

After each phase, all four must be green before you touch the next phase:
  cd crates/idm-core && cargo test
  cd src-tauri && cargo test
  pnpm build
  pnpm check:palette
Then tick that phase's acceptance list in planning/11-media-download-plan.md and
the M6 media checkbox in planning/06-milestones.md.

Fixed decisions - do not re-litigate:
- Remux goes behind ONE function: video::mux::join(inputs, output, container) in
  src-tauri/src/video/mux.rs. Ship V4a (ffmpeg sidecar, LGPL build, -c copy)
  first. V4b (native fMP4 writer) may replace the body later, but that signature
  and its callers stay unchanged. Record the decision in planning/decisions/.
- The quality panel is a separate small Tauri window opened from the existing
  overlay button. Never a native <select>.
- Parsing lives only in crates/idm-core/src/media/. The engine downloads tracks;
  it does not know what a video is.
- No re-encoding, ever. A transcode is a bug.
- Never write a YouTube cipher solver or an `n` solver. The default path reuses
  the URL the page already requested (plan V6). Appending a raw `s` as
  `&signature=` is the reference product's bug - do not copy it.
- DRM (application/vnd.yt-ump, #EXT-X-KEY, MPD ContentProtection) is detected and
  refused with one readable sentence. Never downloaded, never decrypted.
- Design: semantic tokens only, no hardcoded colours or sizes, no emoji, no pill
  tags, no decorative dividers, no native <select>.

If something in the plan is wrong or impossible, stop and report the file and
line with what you found. Do not invent a workaround.
```

---

Why this order: V7 (refusals) comes before V6 (YouTube) on purpose. The refusal
paths are what keep the feature honest, and they are cheap; YouTube is the only
phase that can rot without anyone touching the code.
