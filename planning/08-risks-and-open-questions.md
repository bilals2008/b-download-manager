# Risks and open questions

## Open questions

These are things we do not know. Each one is a decision waiting on an
answer, not a task waiting to be done.

### About the reference product

| Question | Why it matters | Where to look |
|---|---|---|
| How does the reference product choose the number of connections and the boundaries? | It drives throughput more than anything else we could copy | `FUN_0058fdf0`, Priority 1 in the RE plan |
| What exactly triggers "add an optional connection"? | Our scheduler currently uses a different rule | `FUN_00564650`, `HTTPSession::GetSlowestConnection` |
| How does the reference binary verify a chunk against the file head? | Answered in behaviour, not in the binary: the clone compares the head bytes and rolls back, and a lying server is a test fixture now (`spawn_lying_server`) | `FUN_005634de` |
| What is the 16-byte `ChList` registry record? | Only matters if we ever want to read an existing download of the reference product | `FUN_005634de` |
| Where does the chunk size come from? | `FUN_005b1330` is only `ntohl`; the producer was not found | trace the callers of `FUN_005b2ae0` |
| How does the product handle `multipart/x-byteranges`? | A server may answer a range with a multipart body | `0x6cf0ac` |

### About the product

Each row still matters; the answer column says what the source does today,
measured 6 November 2026.

| Question | Why it matters | Where the code landed |
|---|---|---|
| Is a 16 MiB segment target right? | It was a guess. A sweep against a real server would settle it. | `connections_for()` sizes by total, one connection per 10 KB, capped; the sweep has never been run on an unthrottled link |
| Is growing on "most bytes remaining" the right rule? | Growth should trigger on a *lagging* connection, not a large one. | Resolved: `maybe_grow()` compares progress ratios now (M4 in [06](./06-milestones.md)) |
| What should happen when a server does not support ranges but the file is huge? | Today: one connection, very slow. | Unchanged; the downgrade is correct, just slow |
| Should the manifest be a file or a database? | A file is portable and diffable. | A file, written to a temp path and renamed (`session.rs:120`) |
| Is the per-download connection limit the right knob to expose? | The reference product exposes per-site limits too. | Per-site overrides are live in the engine (`limits.rs` `SiteLimits`, resolved at `downloader.rs:1904`) and exposed as `set_site_connections` / `site_connections` commands, but no screen calls them yet |
| Should the UI show a live speed, or just progress? | A live speed is useful but noisy. | Settled: the list, the progress window and the status bar all show it |

## Risks

### R1 — The licence

The publisher's terms of use prohibit reverse engineering. This project reads
the binary to learn behaviour and reimplements it; it ships none of that
product's code, strings or assets.

**Mitigation:** keep the line bright. The reverse-engineering notes are a
behavioural specification, kept in [02](./02-rea-plan.md) and
[10](./10-video-capture-research.md); the standalone `ANALYSIS.md` report was
removed in `433e99e`. Nothing from the binary is copied into the source. If
the project ever needed to interoperate with the reference product's on-disk
format, that is a different and much more sensitive conversation.

**Severity:** high if ignored, none if the line is held.

### R2 — Servers that do not behave

Real servers lie. They answer `200` for a range, they send a different
`Content-Range`, they serve different bytes for the same offset, they close
early. Every one of those produces a corrupt file if we trust them.

**Mitigation:** M3 shipped, and each case now has a named test — the ignoring
server, the wrong-`Content-Range` server, `spawn_lying_server`,
`a_short_answer_is_asked_for_again`. The case that is still uncovered is a
server that honours the range but serves the *wrong content* at the right
offset in a way the head check cannot see; that is the file-signature check in
[09](./09-remaining-work.md).

**Severity:** low, and it was high before M3.

### R3 — The engine is not a crate yet

**Resolved.** `crates/idm-core` holds the engine with no Tauri dependency and
`crates/idm-cli` is its headless client; `cargo test -p idm-core` runs 178
engine tests with no display (measured 6 November 2026).

**Severity:** none.

### R3b — The headless client has no tests of its own

`cargo test --workspace` reports `0 passed` for `idm-cli`, and the CLI is where
a script or a future integration would use the engine. Its four planned tests
([07](./07-verification-plan.md)) have not been written.

**Mitigation:** add them to `crates/idm-cli`, reusing the same fixture servers.

**Severity:** low for users, medium for confidence in the crate boundary.

### R4 — Splitting a file into part files doubles the disk cost

A 10 GiB download needs 10 GiB of part files plus 10 GiB of output, at peak.

**Mitigation:** the parts are deleted as soon as the output is renamed into
place. But the peak requirement is real and should be checked before
starting, not after. That is a M3 item.

**Severity:** medium.

### R5 — Reqwest does not give us the control the reference product has

The reference binary writes its own HTTP over raw sockets. `reqwest` does
TLS, HTTP/2, redirects and header parsing for us — and takes away the ability
to do things the product does, such as sending a `Connection` header, or
controlling exactly what goes on the wire.

**Mitigation:** the port is deliberately thin. If a specific behaviour turns
out to be unreachable through `reqwest`, the fix is a `transport` module
with a second implementation, not a fight with the client.

**Severity:** low, and it is why `Transport` is a trait in the target
architecture.

### R6 — The visual direction was never seen on a screen

This was written when the app had one warm palette. It now has nine
appearances, the site ships real captures of the running window, and
`pnpm check:palette` proves 70 colour pairs in all of them.

**The residue of the risk:** the gate checks contrast, not character, and a
new appearance can be added without anyone looking at a download running
inside it.

**Mitigation:** the re-check list in
[07](./07-verification-plan.md#visual-verification).

**Severity:** low for correctness, medium for the product.

### R7 — The rebalancing rule is wrong

**Resolved.** `maybe_grow()` compares progress ratios now, not remaining bytes,
and M4 records the change. What is still unknown is the *right* ratio, which is
a measurement, not a reading of the binary.

**Severity:** low.

### R8 — Unbounded resource use

Each connection holds a socket and a file handle. A user who sets 32
connections on ten downloads gets 320 sockets. The reference product has a
global cap (*"Too many open network connections. Please wait for some
downloads to finish."*).

**Mitigation:** shipped — `ConnectionBudget` is a sized semaphore taken per
connection and returned when it ends, `DEFAULT_MAX_CONNECTIONS` is 32
(`limits.rs:136`), and Settings resizes the live budget
(`state.rs:925`). Per-host overrides exist in the engine as well.

**Severity:** low. One gap: the refusal surfaces as the engine's own line,
"the connection budget is closed" (`downloader.rs:1130`), which tells the user
nothing about waiting for a permit.

### R9 — Only Windows is a first-class platform

Three OSes are built and signed, but the platform-specific code is lopsided:
`free_space()` is Windows-only, the staging folder's hidden attribute is
Windows-only, and the share hand-off has a Windows branch. A macOS or Linux
user therefore gets no disk-space guard and can see the unfinished folder in
the file manager.

**Mitigation:** a `statvfs` branch beside the `GetDiskFreeSpaceExW` one, and a
dot-prefixed or `chflags hidden` staging folder on the two unices. The
development machine is Windows 10, so the macOS and Linux paths have been
compiled and bundled by CI but not run here — treat every claim about them in
this folder as unverified until someone opens the app on that OS.

**Severity:** medium. The app works there; the safety net under it does not.

### R10 — A timing-sensitive test in the resume path

`pausing_an_unknown_size_download_does_not_finish_it_short` failed inside the
full-suite run on 6 November 2026 and passed when run alone twice. The
assertion depends on how far the transfer got before the pause landed.

**Mitigation:** pace the fixture server instead of the assertion, as
`spawn_paced_server` already does elsewhere.

**Severity:** medium for trust, none for shipped behaviour — the pause path
itself is covered by two deterministic tests.

## Deliberately not tracked

- Performance of the reference product itself. We do not have a legitimate
  way to benchmark it, and a number we cannot produce should not be quoted.
- The reference product's UI. The direction here is ours.
- Native messaging. The paired loopback bridge does the job with less surface
  (`ADR-0005-bridge-pairing`).
- What the browser does with a download it keeps. The extension hands over or
  leaves alone; it does not manage the browser's own download shelf.

Tracked now, no longer "until M5 says so": the Grabber, the browser capture and
the media path all shipped, so their risks belong in the list above and in
[09](./09-remaining-work.md). The scheduler is the one that still has no plan
of its own.
