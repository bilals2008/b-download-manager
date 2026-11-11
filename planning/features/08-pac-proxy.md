# PAC proxy execution

**Wave 3 · Status:** 🔧 `net/pac.rs` (+ `net/pac/js.rs`, 1499 lines) exists, nothing calls it

## Goal

Honour a PAC file the way a browser does: the proxy for a download is decided
per URL, from the script, and applied to that request.

## Where it stands

The PAC engine and its JavaScript interpreter are written and tested, with no
caller. Settings → Proxy accepts a static proxy address only. Downloads never
consult a PAC file, and nothing fetches the system's auto-proxy setting.

## Steps

1. Decide where PAC fits beside the static proxy: a mode setting (none, static,
   system, a URL), and what "system" means (WinHTTP's configuration).
2. Fetch the PAC file over the same client policy as everything else (with the
   encrypted DNS and the adapter binding if they are on), cache it, and refetch
   on a clock and on demand.
3. Evaluate per URL before the request, not once per run: `DIRECT`, `PROXY`,
   `SOCKS`, and the fallback list order must be kept.
4. Apply the chosen route to the client that makes the request — including the
   redirected hops — and to the range requests of a resumed download.
5. Refuse to fall through to `DIRECT` when the adapter kill switch is on and the
   PAC says direct: that is a conflict the user must resolve, not a silent route.
6. Show the chosen route in the progress window's connection details.
7. Split `net/pac/js.rs` before touching it (wave 1 of the split plan).

## Files

`crates/idm-core/src/net/pac.rs`, `crates/idm-core/src/net/pac/js.rs`,
`crates/idm-core/src/net.rs`, `crates/idm-core/src/transfer.rs`,
`src-tauri/src/settings_store.rs`,
`src/components/settings/proxy-settings.tsx`,
`src/components/windows/progress/connection-details.tsx`.

## Edge cases

- A PAC file that throws on one URL: treat as DIRECT and log once, never crash
  the run.
- A PAC that returns an unreachable proxy: fail the request with its reason,
  do not retry direct unless the PAC's fallback list says so.
- Credentials in a PAC's proxy string are refused, as the static proxy is.

## Tests

Unit: the route parser and the fallback order. Integration: a local PAC that
returns a local proxy for one host and DIRECT for another, two downloads, the
right route each time.

## Proof

`cargo test -p idm-core`, then the real app with a local PAC and a local proxy,
watching each download take the route the script chose.

## Out of scope

Writing a PAC editor; the user brings a file or an address.
