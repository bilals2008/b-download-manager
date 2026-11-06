# Planning

Everything needed to take this from "a working core" to "a complete
alternative", and to keep reverse engineering the reference download manager
in a state where the next person can pick it up without re-deriving
everything.

## Read in this order

| File | What it is for |
|---|---|
| [01-scope-and-goals.md](./01-scope-and-goals.md) | what "complete" means, and what is deliberately out of scope |
| [02-rea-plan.md](./02-rea-plan.md) | the next reverse engineering targets, in priority order, with the addresses and the evidence each one should yield |
| [03-target-architecture.md](./03-target-architecture.md) | the module layout the finished alternative should have |
| [04-engine-spec.md](./04-engine-spec.md) | the download engine, specified algorithm by algorithm |
| [05-ui-design-direction.md](./05-ui-design-direction.md) | the soft warm visual direction, tokens and component rules |
| [06-milestones.md](./06-milestones.md) | phased plan with acceptance criteria |
| [07-verification-plan.md](./07-verification-plan.md) | how we prove each piece works instead of assuming it |
| [08-risks-and-open-questions.md](./08-risks-and-open-questions.md) | what could go wrong and what is still unknown |
| [09-remaining-work.md](./09-remaining-work.md) | the checked list of what is still to build, measured against the source |
| [10-video-capture-research.md](./10-video-capture-research.md) | how the reference manager captures video (HLS/DASH/YouTube), with evidence |
| [11-media-download-plan.md](./11-media-download-plan.md) | the implementation plan for video capture: contracts, phases V0-V7, gates, risks |
| [12-implementation-prompt.md](./12-implementation-prompt.md) | the copy-paste prompt to hand this work to an implementing agent |
| [13-theme-and-icon-style-plan.md](./13-theme-and-icon-style-plan.md) | the proposal for more themes and for isometric icon styles |
| [14-own-youtube-and-native-muxer-plan.md](./14-own-youtube-and-native-muxer-plan.md) | the plan that removed `yt-dlp` and `ffmpeg` and rebuilt both in-house |
| [decisions/](./decisions/) | architecture decision records |

## Where the evidence lives

`ANALYSIS.md` and `PROGRESS.md` were removed from the repository in
`433e99e`. Do not link them; the living evidence is here:

- [`../docs/DEVELOPMENT.md`](../docs/DEVELOPMENT.md) — the commands, the gates
  and the layout the project actually uses.
- [`../CHANGELOG.md`](../CHANGELOG.md) and
  [`../WHATS_NEW.md`](../WHATS_NEW.md) — what shipped, and when.
- [10-video-capture-research.md](./10-video-capture-research.md) — the reverse
  engineering evidence for the media path, with addresses.
- [decisions/](./decisions/) — the ADRs, each naming the alternative it
  rejected.
- Addresses and method names quoted in these plans point at the studied
  reference binary, not at this source tree.

## Current state

Measured against the source on 6 November 2026, at v1.0.0. Every line names
the file that proves it.

| Area | State | Where |
|---|---|---|
| Range requests, part files, assembly, resume manifest | implemented | `crates/idm-core/src/downloader.rs`, `session.rs` |
| Pause / resume / cancel over IPC | implemented | `src-tauri/src/actions.rs` |
| HTTP, HTTPS and FTP | implemented | `transfer.rs` (`ftp://` parse), `ftp.rs` (`REST` per chunk) |
| Proxy | implemented for HTTP and SOCKS, whole-application; no bypass list, and credentials are stripped on save | `settings_store.rs` (`proxy_url`, `strip_credentials`), `state.rs:926`, `limits.rs` (`Limits::proxy`) |
| Connection count, dynamic splitting, range-capability probe | implemented | `segmenter.rs`, `limits.rs` |
| Speed limiting | implemented globally (Settings) and per download (the Speed limit tab of the progress window, `setDownloadSpeedLimit`) | `limits.rs`, `src/components/windows/progress/speed-limit-tab.tsx` |
| Queue with priorities | implemented at the application layer, not inside the engine | `src-tauri/src/queue.rs` |
| Scheduler (start times, named queues) | not implemented; the menu item is disabled | `src/components/shell/menus.ts` |
| Media capture (HLS / DASH / file) | implemented: parsers, quality panel, segment resume, native remux | `media/{hls,dash,download,quality}.rs`, `src-tauri/src/video/mp4.rs` |
| YouTube extraction | implemented in-house; `yt-dlp` and `ffmpeg` removed | `src-tauri/src/video/yt.rs`, [14](./14-own-youtube-and-native-muxer-plan.md) |
| Browser capture | implemented: Firefox and Chrome/Edge extensions, paired loopback bridge, referer, batch hand-over | `browser-extension/`, `extension-firefox/`, `src-tauri/src/bridge/` |
| Site Grabber | implemented: crawl, robots, filters, its own window | `src-tauri/src/grabber/` |
| Integrity verification of chunk heads | implemented | `downloader.rs`, tested in `downloader/tests.rs` |
| File-signature verification (content of a range) | not implemented | — |
| Media credentials (Cookie, Authorization) | implemented behind a one-time consent, to the manifest's own origin only | `src-tauri/src/video/policy.rs`, `video/capture.rs` |
| `multipart/byteranges`, RTMP, FTPS, proxy bypass, proxy credentials, Chrome-side cookies, site logins | not implemented | — |
| Disk space check | implemented on Windows only; `free_space()` returns `None` elsewhere | `src-tauri/src/lib.rs`, `crates/idm-core/src/disk.rs` |
| Graceful shutdown, retry backoff, diagnostics log | implemented | `lifecycle.rs`, `downloader.rs`, `limits.rs` |
| Export / import, drop target, per-download start time in the UI | stubs only; the menu entries are disabled | `src/components/shell/menus.ts` |

## Working rules

1. **Never state a number that was not measured.** A contrast ratio, a
   throughput figure, a test count — run the thing and report what it printed,
   and say when it was run.
2. **No binary is copied.** Everything here is reimplemented from observed
   behaviour. The reference product's publisher forbids reverse engineering;
   this project treats the binary as a *behavioural specification* and ships
   none of that product's code, strings or assets.
3. **Static analysis is not runtime evidence.** Unless a claim was observed
   running, it is labelled as inference.
4. **One theme, one source of truth.** Colour lives in `src/index.css` and
   `src/styles/themes/`, and every appearance is checked by
   `pnpm check:palette`.
5. **A status line names its proof.** "implemented" without a file path is a
   claim, not a fact. When a plan is refreshed, write the date it was measured
   (see *Current state* above) instead of leaving undated status tables to
   rot.
6. **Every word in this repository is English.** Plans, ADRs, code comments,
   release notes and commit messages included. Roman Urdu and other languages
   belong in the conversation, never in a file — a plan that cannot be read by
   the next agent or contributor is not a record. `pnpm check:links` keeps the
   links honest; nothing gates the language, so it is on the writer.
