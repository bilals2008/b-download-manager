# ADR 0003 — `reqwest` for now, a `Transport` trait to keep the door open

- **Status:** accepted
- **Date:** 2026-10-21
- **Relates to:** [../03-target-architecture.md](../03-target-architecture.md),
  [../08-risks-and-open-questions.md](../08-risks-and-open-questions.md) R5

## Context

The reference product implements HTTP itself over raw WinSock sockets, and
terminates TLS with OpenSSL. The import table has `socket`, `recv`, `send`,
`ioctlsocket` and `closesocket` from `WS2_32.DLL`, and there is no WinHTTP
and no WinInet anywhere. Its install directory ships `libssl.dll` and
`libcrypto.dll`, and the read path branches on the protocol:

```c
if (*(int *)(*(int *)(in_ECX + 200) + 0x24) == 5) {   // 5 == TLS
    iVar3 = FUN_005b32a0(buf, len);                   // SSL_read
} else {
    iVar3 = recv(*(SOCKET *)(in_ECX + 0x31f8), buf, len, 0);
}
```

So the reference download manager owns the whole stack. We could do the same.

## Options considered

**A. `reqwest` over `tokio`, behind a `Transport` trait.** *(chosen for now)*

**B. Raw sockets plus `rustls` or `openssl`, written by us.**

**C. `reqwest` with no abstraction layer.**

## Decision

**Option A**, with the `Transport` trait in the target architecture, and a
rule that the trait is what the engine is written against.

```rust
pub trait Transport: Send + Sync {
    async fn fetch_range(&self, from: u64, to: u64) -> Result<ByteStream, TransportError>;
    async fn probe(&self) -> Result<Probe, TransportError>;
}
```

The current implementation satisfies the trait with `reqwest`. A second
implementation, if one is ever needed, drops in beside it.

## Rationale

**Why `reqwest` now.** It does TLS, HTTP/1.1 and HTTP/2, redirects, header
parsing and connection pooling, correctly, and it is maintained. Writing that
is weeks of work that is not the point of the exercise. The parts of the
engine that *are* the point — segmentation, the chunk list, resume,
rebalancing, assembly — are entirely transport-agnostic.

**Why the trait at all.** The trait is what keeps the decision reversible.
Three specific things `reqwest` may not be able to do:

1. **Send a `Connection` header.** The reference product writes
   `Connection: Keep-Alive\r\n` on the wire. HTTP/1.1 makes keep-alive the
   default, so this is cosmetic, but hyper may refuse to let it through. The
   current implementation omits the header for exactly this reason and says so
   in a comment.
2. **Control the wire format exactly.** The reference product's header order
   and spacing are fixed and hand-written. Some servers are picky.
3. **Speak a protocol `reqwest` does not know.** RTMP, SOCKS, FTP. These are
   all in M6.

**Why not option C, no abstraction.** The engine would be written directly
against `reqwest`, and swapping the transport later would mean rewriting the
engine. The trait costs about forty lines and removes that risk.

**Why not option B now.** Writing our own HTTP client is exactly the kind of
work that is invisible in the finished product and large in the effort. It
also means owning TLS configuration, proxy support, certificate validation
and every HTTP/1.1 parsing edge case, none of which differentiates us from
the product being imitated. If a specific behaviour turns out to be unreachable through `reqwest`,
that is the moment to write a second implementation — behind the trait, as
planned.

## Consequences

**Good.**

- TLS, redirects and pooling are correct on day one.
- The engine can be tested against a fake `Transport` with no sockets at all,
  which is what the failure tests in M3 need.
- The decision is reversible at a known cost.

**Bad.**

- `reqwest` decides some things for us. The `Accept-Encoding` header is
  not added when the compression features are disabled, which is exactly what
  we want — the ranges have to line up with the file offsets — but it means
  the header set has to be checked whenever the features change.
- HTTP/2 multiplexing is available, which we do not use. Ranges and HTTP/2
  interact in ways that need checking before HTTP/2 is relied on.

**Neutral.**

- TLS is `rustls`, not OpenSSL, and is a static dependency. The reference
  product ships OpenSSL as separate DLLs; we do not.
