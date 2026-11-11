# Phone and LAN remote control

**Wave 6 · Status:** ❌ not written (the reference build has `crates/remote`)

## Goal

A phone on the same home network can add links and see the list, after a one
time pairing, without the API being reachable from the internet.

## Where it stands

Nothing. The only server is the loopback bridge, bound to `127.0.0.1` on
purpose, and it refuses every origin but a browser extension's.

## Steps

1. A second listener, off by default, that survives a network change and is
   bound to a chosen local interface only.
2. Pairing: a one-time code or QR shown in Options → Remote, exchanged for a
   per-device token stored hashed; no password is ever typed on the phone.
3. A small page served by the app: the list, add a link, pause, resume, remove.
   No framework; it is a few files, and it must be readable on a phone.
4. Every route checks the token, the device is revocable, and the whole server
   can be switched off from the tray without a window.
5. Refuse a request from outside the local network, both by source address and
   by the `Host` header, the same reasoning the bridge uses.
6. TLS is not required on a trusted LAN, but the page must say plain HTTP.

## Files

New crate `crates/idm-remote`, `src-tauri/src/lib.rs` (start it),
`src-tauri/src/state.rs` (the device list),
`src/components/settings/advanced-settings.tsx`,
`src/lib/ipc.ts`.

## Edge cases

- A device that was paired and then lost: revoke in the settings page, and the
  served page must refuse it at once.
- A VPN adapter active with the kill switch on: the remote listener follows the
  same rule, it does not open a second route.
- A network change mid-request: the listener rebinds, the page reconnects.

## Tests

Unit: the pairing exchange, the token check and the local-network rule.
Integration: a second "device" on loopback given a token, one add and one read;
an unpaired request refused.

## Proof

`cargo test -p idm-remote`, then a real phone on the LAN adding a download that
appears in the window.

## Out of scope

Any port forwarding, any cloud relay, and any access from outside the LAN.
