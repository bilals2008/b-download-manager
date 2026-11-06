# Verification plan

The rule: **never state a number that was not measured.** Everything below is
either a command to run or a claim that has to be earned first.

## The gates that exist today

| Gate | Command | What it proves |
|---|---|---|
| Rust tests, engine and app together | `cargo test --workspace` | the range logic, resume, integrity, FTP, the media path |
| Engine alone, headless | `cargo test -p idm-core` | the engine without a window or a display |
| Rust lint | `cargo clippy --workspace -- -D warnings` | no warnings, no smells |
| Frontend unit tests | `pnpm test` | the React components, settings, shortcuts, find, selection |
| Type check and production build | `pnpm build` | the UI compiles and bundles |
| Every gate at once | `pnpm verify` | the pre-push contract |
| Palette contrast | `pnpm check:palette` | every rendered colour pair in every appearance |
| The shell self-checks | `pnpm check:all` | dialogs, toolbars, tray, settings, the extension, the grabber |
| The release contract | `pnpm check:release` | versions, updater signature, bridge, release notes |
| Emoji ban | `python3 <kit>/scripts/check_no_emoji.py src` | no emoji in the product (external UX kit) |

Measured state, run twice on 6 November 2026 at v1.0.0 on the development
machine (Windows 10 Pro, build 19045), the second time after the work in this
folder:

- `cargo test --workspace`: **448 passed, 0 failed, 5 ignored**. The two
  ignored in `idm-core` (178 passed) are throughput benchmarks; the three in
  the app (270 passed) reach YouTube.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo fmt --all --check`: clean. It was **not** clean at `d918b72`, which
  had never been through CI.
- `pnpm test`: **132 passed in 19 files**.
- `pnpm check:palette`: **all 70 pairs pass in all 9 appearances**.
- One failure appeared in the first full-suite run of the day —
  `pausing_an_unknown_size_download_does_not_finish_it_short` — and it did not
  reappear when that test ran alone twice, nor in either later full-suite run.
  Treat it as a timing assumption until it is fixed.

The older figure in this file's history, "cargo test 7 passed", belonged to the
first engine commit and is kept here only to show how far the suite has grown.

## The test server

This section asked for a fixture binary to be built. It was built, just not at
the path the plan guessed: the fixtures live inside the crate, as `spawn_*`
helpers in `crates/idm-core/src/downloader/tests.rs`, an FTP control server in
`crates/idm-core/src/ftp.rs` (`mod server`), and a media server with per-path
failure, header requirements and byte drops in `crates/idm-core/src/media/
testserver.rs`.

| Mode | Where it lives | What it proves |
|---|---|---|
| honours `Range`, answers `206` | `spawn_range_server` | the happy path |
| ignores `Range`, answers `200` | `spawn_ignoring_server`, `spawn_slow_ignoring_server` | the downgrade path |
| answers `206` with the wrong `Content-Range` | `spawn_scripted_server`, `an_unreadable_content_range_is_refused` | the reply validation |
| drops the connection after N bytes | `spawn_flaky_server(data, failures)` | resume and reassignment |
| serves different bytes per range | `spawn_lying_server`, `a_server_that_sends_the_wrong_bytes_goes_on_with_one_connection` | the integrity check |
| trickles bytes | `spawn_slow_server`, `spawn_paced_server(data, chunk, delay)` | the scheduler and the speed limit |
| closes early, claims a longer length | `a_short_answer_is_asked_for_again` | the final-size check |
| no `Content-Length` | `spawn_unknown_size_server`, `spawn_slow_unknown_size_server` | unknown total size |
| refuses a header it requires | `testserver::require` | protected media and referer handling |
| answers a status on one path | `testserver::fail`, `fail_next`, `drop_next` | retry and error surfaces |

Determinism is held by construction, not by a seed: a failure count, a fixed
delay or a scripted reply is passed in, so the same test asks the same server
for the same behaviour every run.

**Still open:** the flaky-style helpers depend on wall-clock timing in a few
places, which is the suspected cause of the intermittent pause test above. A
seeded generator would make that class of test repeatable.

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

Engine status, 6 November 2026: every row above has a named test except
"the manifest is written atomically". The write *is* atomic — a temporary file
renamed into place (`session.rs:120`) — but no test kills a write mid-flight to
prove it.

### CLI

| Test | Asserts |
|---|---|
| download a file to a path | the output matches the source |
| download a URL that 404s | a non-zero exit and a clear message |
| resume an interrupted download | only the missing bytes are requested |
| `--connections 1` | exactly one request is made |

CLI status, 6 November 2026: **none of these run in CI.** `crates/idm-cli/src/
main.rs` holds no test functions and `cargo test --workspace` reports
`0 passed` for that binary. The four rows above are still the plan, not the
evidence.

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

UI status, 6 November 2026: nine of the suite's nineteen files are component
tests (`desktop-icons`, `settings-search`, `shortcuts-settings`,
`use-setting-reveal`, `use-shell-shortcuts`, `release-notes`, `tooltip`,
`update-meter`, `download-progress`); the other ten are pure logic, and the
logic set now includes `downloads.test.ts`, which covers the folder each
category saves into. Between them they cover shortcuts, settings search,
tooltip behaviour and the category routing, but none of the table-rendering or
axe rows above. There is no axe audit and no viewport test in the suite; the
keyboard row is partly covered by `use-shell-shortcuts.test.tsx`.

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

Status, 6 November 2026: **still unearned.** The harness exists —
`bench_local_segmented` and `bench_connections` in `crates/idm-core/src/
downloader/tests.rs`, the second reading `BDM_BENCH_URL` — and both are
`#[ignore]`d, so they do not run in CI and no figure has ever been written
down. Run them with `cargo test --release bench_ -- --ignored --nocapture`
against an unthrottled server, then put the table in the README.

## Visual verification

The UI is checked two ways now. Headless: the DOM renders, the tokens apply,
the labels are present, and `pnpm check:palette` prints 70 pairs passing in all
9 appearances. On a screen: the product has shipped nine versions, and real
captures of the running app are the marketing images
(`site/public/images/screens/app-home.webp` and its neighbours), so the window
has been looked at.

What has *not* been re-checked since then is the list below. Each line is a
gate nobody runs, so treat it as open rather than passed:

- [ ] the palette reads well in each of the nine appearances, not only the one
      the screenshots were taken in
- [ ] the primary action is the only filled button, on every screen
- [ ] the table's column widths look right with a long file name
- [ ] the empty state is centred in its card
- [ ] the progress bar is visible against the track in every appearance
- [ ] focus rings are visible on every control
- [ ] the layout holds at 320 px, 768 px and 1440 px — the window's own minimum
      width is 640 px (`src-tauri/tauri.conf.json`), so the 320 px row applies
      to the web and extension surfaces, not the main window

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
