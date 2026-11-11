# Encrypted DNS: DoH, DoT, NextDNS

**Wave 3 · Status:** 🔧 `net/dns.rs` exists, nothing calls it

## Goal

The user chooses how names are resolved: the system resolver, DNS over HTTPS,
DNS over TLS, or a NextDNS profile. The choice applies to downloads, the PAC
fetch, and the update check.

## Where it stands

`crates/idm-core/src/net/dns.rs` is a complete resolver path with tests and no
caller. Every HTTP client in `transfer.rs` and `video/` builds a plain
`reqwest` client, which uses the system resolver. Settings → Connection has no
page for DNS.

## Steps

1. Add three settings to the engine's file: mode (system, doh, dot, nextdns),
   the endpoint URL for DoH, the hostname for DoT, and the profile id for
   NextDNS.
2. Build the resolver once in the app state and hand it to every client through
   `ClientBuilder::dns_resolver`; there must be one place that constructs
   clients, or a second one will silently use the system resolver.
3. Validate what the user typed before storing it: HTTPS for DoH, a hostname
   for DoT, a profile id that is a hostname component for NextDNS. Never store
   a URL with credentials.
4. Add the settings page with a mode picker and one field per mode, plus a
   "test this resolver" button that resolves a known name once.
5. Log the mode, never the query contents, when a resolution fails.
6. Fall back to the system resolver only if the user says so; a silent fallback
   is a privacy leak and must be a visible setting.

## Files

`crates/idm-core/src/net/dns.rs`, `crates/idm-core/src/net.rs`,
`crates/idm-core/src/transfer.rs`, `src-tauri/src/settings_store.rs`,
`src-tauri/src/state.rs`, `src/components/settings/connections-settings.tsx`,
`src/components/settings/settings-search-data.ts`.

## Edge cases

- A DoH resolver's own hostname still needs a bootstrap lookup; the current
  resolver path resolves it through the system once. Say so in the settings
  hint rather than pretending it is fully encrypted.
- A captive portal answers everything; the test button is what tells the user.
- Certificate failure must surface as "the resolver could not be reached", not
  as a failed download.

## Tests

Unit: the validation rule and the endpoint builder per mode. Integration: a
resolve against a local DoH test server, and one download through it.

## Proof

`cargo test -p idm-core`, then the real app: switch to DoH, resolve, download,
and read the log to confirm the mode in force.

## Out of scope

The adapter binding (its own plan); DNS is about names, not routes.
