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
| [10-video-capture-research.md](./10-video-capture-research.md) | how the reference manager captures video (HLS/DASH/YouTube), with evidence |
| [11-media-download-plan.md](./11-media-download-plan.md) | the implementation plan for video capture: contracts, phases V0-V7, gates, risks |
| [decisions/](./decisions/) | architecture decision records |

## Where the evidence lives

- [`../ANALYSIS.md`](../ANALYSIS.md) — the reverse engineering report for
  the studied 32-bit binary: every string, address and pseudo-code fragment
  the design is based on, plus an explicit list of what could not be
  established.
- [`../src-tauri/src/`](../src-tauri/src/) — the current implementation.

## Current state

| Area | State |
|---|---|
| Range requests, part files, assembly, resume manifest | implemented |
| Pause / resume / cancel over IPC | implemented |
| Connection count, dynamic splitting, server range detection | implemented |
| Chunked transfer encoding | handled by `reqwest` |
| FTP | not implemented |
| Proxy / SOCKS | not implemented |
| Media splitting (HLS / DASH / MPEG-TS) | not implemented, research in [10](./10-video-capture-research.md), plan in [11](./11-media-download-plan.md) |
| Speed limiting, scheduling, queue | not implemented |
| Integrity verification of chunk heads | not implemented |
| Graceful shutdown, disk space checks, retry backoff policy | partial |

## Working rules

1. **Never state a number that was not measured.** A contrast ratio, a
   throughput figure, a test count — run the thing and report what it printed.
2. **No binary is copied.** Everything here is reimplemented from observed
   behaviour. The reference product's publisher forbids reverse engineering;
   this project treats the binary as a *behavioural specification* and ships
   none of that product's code, strings or assets.
3. **Static analysis is not runtime evidence.** Unless a claim was observed
   running, it is labelled as inference.
4. **One theme, one source of truth.** Colour lives in `src/index.css` and is
   checked by `npm run check:palette`.
