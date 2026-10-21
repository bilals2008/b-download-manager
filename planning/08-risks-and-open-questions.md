# Risks and open questions

## Open questions

These are things we do not know. Each one is a decision waiting on an
answer, not a task waiting to be done.

### About IDM

| Question | Why it matters | Where to look |
|---|---|---|
| How does IDM choose the number of connections and the boundaries? | It drives throughput more than anything else we could copy | `FUN_0058fdf0`, Priority 1 in the RE plan |
| What exactly triggers "add an optional connection"? | Our scheduler currently uses a different rule | `FUN_00564650`, `HTTPSession::GetSlowestConnection` |
| How does IDM verify a chunk against the file head? | We do not verify at all yet | `FUN_005634de` |
| What is the 16-byte `ChList` registry record? | Only matters if we ever want to read an existing IDM download | `FUN_005634de` |
| Where does the chunk size come from? | `FUN_005b1330` is only `ntohl`; the producer was not found | trace the callers of `FUN_005b2ae0` |
| How does IDM handle `multipart/x-byteranges`? | A server may answer a range with a multipart body | `0x6cf0ac` |

### About the product

| Question | Why it matters |
|---|---|
| Is a 16 MiB segment target right? | It was a guess. A sweep against a real server would settle it. |
| Is growing on "most bytes remaining" the right rule? | It is not the same as IDM's rule, and it is probably wrong. Growth should trigger on a *lagging* connection, not a large one. |
| What should happen when a server does not support ranges but the file is huge? | Today: one connection, very slow. Maybe: a different strategy. |
| Should the manifest be a file or a database? | A file is portable and diffable; a database is queryable. A file, until there are hundreds of downloads. |
| Is the per-download connection limit the right knob to expose? | IDM exposes per-site limits too. |
| Should the UI show a live speed, or just progress? | A live speed is useful but noisy. Undecided. |

## Risks

### R1 — The licence

IDM's terms of use prohibit reverse engineering. This project reads the
binary to learn behaviour and reimplements it; it ships no IDM code, no IDM
strings, no IDM assets.

**Mitigation:** keep the line bright. `ANALYSIS.md` is a behavioural
specification. Nothing from the binary is copied into the source. If the
project ever needed to interoperate with IDM's on-disk format, that is a
different and much more sensitive conversation.

**Severity:** high if ignored, none if the line is held.

### R2 — Servers that do not behave

Real servers lie. They answer `200` for a range, they send a different
`Content-Range`, they serve different bytes for the same offset, they close
early. Every one of those produces a corrupt file if we trust them.

**Mitigation:** M3 in the milestone plan — the integrity checks, before the
product grows.

**Severity:** high. This is the difference between a downloader and a file
corruptor.

### R3 — The engine is not a crate yet

The engine lives in the Tauri app. That means it can only be tested through
a window, which means the failure tests in M3 are painful to write.

**Mitigation:** M2, before M3.

**Severity:** medium. It slows everything rather than breaking it.

### R4 — Splitting a file into part files doubles the disk cost

A 10 GiB download needs 10 GiB of part files plus 10 GiB of output, at peak.

**Mitigation:** the parts are deleted as soon as the output is renamed into
place. But the peak requirement is real and should be checked before
starting, not after. That is a M3 item.

**Severity:** medium.

### R5 — Reqwest does not give us the control IDM has

IDM writes its own HTTP over raw sockets. `reqwest` does TLS, HTTP/2,
redirects and header parsing for us — and takes away the ability to do
things IDM does, such as sending a `Connection` header, or controlling
exactly what goes on the wire.

**Mitigation:** the port is deliberately thin. If a specific behaviour turns
out to be unreachable through `reqwest`, the fix is a `transport` module
with a second implementation, not a fight with the client.

**Severity:** low, and it is why `Transport` is a trait in the target
architecture.

### R6 — The visual direction was never seen on a screen

The palette passes its contrast gate and the DOM renders with the right
tokens, but no one has looked at it. The direction could still read as
"generic light theme with a brown button".

**Mitigation:** the checklist in the verification plan. It is a five minute
job and it is the difference between the design being an intention and being
a result.

**Severity:** medium for the product, zero for correctness.

### R7 — The rebalancing rule is wrong

`maybe_grow()` currently picks the segment with the most bytes *remaining*.
That is not "the segment that is behind" — a large segment with an early
start has plenty remaining and is not lagging at all. Under the current
rule, growth may never trigger, so the download stays at its initial
connection count.

**Mitigation:** M4, and a test that asserts the connection count actually
grows on a slow connection.

**Severity:** medium. The download still works, it is just not fast.

### R8 — Unbounded resource use

Each connection holds a socket and a file handle. A user who sets 32
connections on ten downloads gets 320 sockets. IDM has a global cap
(*"Too many open network connections. Please wait for some downloads to
finish."*).

**Mitigation:** the global limit in the target architecture, implemented in
M4.

**Severity:** medium.

## Deliberately not tracked

- Performance of IDM itself. We do not have a legitimate way to benchmark
  it, and a number we cannot produce should not be quoted.
- IDM's UI. The direction here is ours.
- The Grabber, the scheduler, the browser integration, until M5 says so.
