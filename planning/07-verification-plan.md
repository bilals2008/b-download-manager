# Verification plan

The rule: **never state a number that was not measured.** Everything below is
either a command to run or a claim that has to be earned first.

## The gates that exist today

| Gate | Command | What it proves |
|---|---|---|
| Rust unit and integration tests | `cd src-tauri && cargo test` | the range logic, the segment logic, the range-ignoring server |
| Rust type and lint check | `cd src-tauri && cargo clippy -- -D warnings` | no warnings, no smells |
| Frontend type check and build | `npm run build` | the UI compiles and bundles |
| Palette contrast | `npm run check:palette` | every rendered colour pair meets WCAG 2.2 |
| Emoji ban | `python3 <kit>/scripts/check_no_emoji.py src` | no emoji in the product |

Current state: `cargo test` 7 passed, 0 failed. `npm run build` succeeds.
`npm run check:palette` 11 of 11 pairs pass. The other two have not been
run in this environment.

## The test server

The engine tests need a server that behaves like a real one, and a server
that misbehaves on demand. Build a fixture binary, `crates/idm-core/tests/
support/server.rs`, with these modes:

| Mode | Behaviour | What it proves |
|---|---|---|
| `ranges` | honours `Range`, answers `206` | the happy path |
| `no-ranges` | ignores `Range`, answers `200` | the downgrade path |
| `wrong-range` | answers `206` with a `Content-Range` for a different offset | the reply validation |
| `flaky` | drops the connection after N bytes, at random | resume and reassignment |
| `lying` | serves different bytes for different ranges | the integrity check |
| `slow` | trickles bytes | the scheduler and the speed limit |
| `truncated` | closes early, claims a longer length | the "final size" check |
| `no-content-length` | chunks without a length | unknown total size |

Each mode must be deterministic under a seed, so a failure is reproducible.

## Test matrix

### Engine

| Test | Type | Asserts |
|---|---|---|
| probe detects size | unit | `total` matches the fixture |
| probe detects range support | unit | `no-ranges` → one connection |
| plan covers the file | unit | ranges are contiguous and sum to the total |
| plan respects the limit | unit | never more than `max_connections` |
| split keeps received bytes | unit | the parent stays valid after a split |
| range request lands at the right offset | integration | the part file holds exactly the requested bytes |
| server ignoring the range is rejected | integration | the error names the cause |
| wrong `Content-Range` is rejected | integration | the error names the offset |
| the assembled file is byte identical | integration | sha256 of the output equals the source |
| resume after a simulated crash | integration | the second run requests only the missing bytes |
| a truncated part file is detected | integration | the download fails rather than producing a short file |
| the manifest is written atomically | integration | a kill mid-write leaves a valid manifest |
| a connection that dies is retried | integration | the range completes |
| pause actually stops the socket | integration | no bytes arrive while paused |

### CLI

| Test | Asserts |
|---|---|
| download a file to a path | the output matches the source |
| download a URL that 404s | a non-zero exit and a clear message |
| resume an interrupted download | only the missing bytes are requested |
| `--connections 1` | exactly one request is made |

### UI

| Test | Asserts |
|---|---|
| the table renders a download | the row shows the name, size and status |
| an update event updates one row | the other rows do not change |
| the pause button appears only for a running download | — |
| the empty state appears when there are no downloads | — |
| every icon-only button has an accessible name | axe: no unnamed controls |
| the whole page is keyboard reachable | tab order is sensible |
| the page is usable at 320 px wide | no horizontal scroll |

## Performance measurement

The claim to earn is "faster than a single connection". Measure it properly:

1. A local fixture server that serves from a fixed file, so the disk is not
   the bottleneck, and a separate run against a remote server.
2. Sweep connection counts 1, 2, 4, 8, 16.
3. Report bytes per second for each.
4. Repeat each run at least three times and report the median.
5. Write the table into the README, with the machine, the OS, the server and
   the date.

Without that table, the project has no claim to being fast, and should not
make one.

## Visual verification

The UI has been checked in a headless environment: the DOM renders, the
tokens apply, the labels are present, and the contrast gate passes. It has
**not** been looked at on a screen. Before M5 closes, take a screenshot of
the running app and look at it, then check:

- [ ] the palette reads as warm and soft, not as grey or as a gradient
- [ ] the primary action is the only filled button
- [ ] the table's column widths look right with a long file name
- [ ] the empty state is centred in its card
- [ ] the progress bar is visible against the track
- [ ] focus rings are visible on every control
- [ ] the layout holds at 320 px, 768 px and 1440 px

## The kit's gates

The UX kit at `D:/Dev/ux-ui-agent-skills` ships browser-driven gates. Run them
against a running app, and report what they print:

```bash
node <kit>/scripts/verify_states.mjs   <page> [--dark]
node <kit>/scripts/axe_audit.mjs       <page>
node <kit>/scripts/verify_responsive.mjs <src> --scale=1.25
node <kit>/scripts/verify_keyboard.mjs   <src>
node <kit>/scripts/verify_target_size.mjs <src>
node <kit>/scripts/verify_mobile_first.mjs <src>
node <kit>/scripts/lint_intent.mjs       <src>
node <kit>/scripts/slop_tells.mjs --strict <page>
python3 <kit>/scripts/lint_hardcodes.py  <src>
python3 <kit>/scripts/check_no_emoji.py  <src>
```

A render gate with no browser prints `SKIPPED` and exits `0`. That is not a
pass. Set `DS_REQUIRE_BROWSER=1` so a missing browser fails loudly.
