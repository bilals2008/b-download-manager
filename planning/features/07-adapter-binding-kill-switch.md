# Network adapter binding with a kill switch

**Wave 3 · Status:** 🔧 `net/adapter.rs` exists, nothing calls it

## Goal

Hold every download to one network adapter — usually a VPN — and, if that
adapter goes away, stop rather than quietly continue over the default route.

## Where it stands

`crates/idm-core/src/net/adapter.rs` can enumerate adapters and bind a socket.
No client uses it, there is no setting, and nothing watches the adapter for
changes. Every download takes the operating system's route.

## Steps

1. Enumerate adapters for the settings page (name, address, up/down), read-only.
2. Store the chosen adapter by a stable identity (GUID), not by its display
   name, which changes.
3. Bind sockets to the adapter's source address *and* set the interface index
   (`IP_UNICAST_IF` for IPv4 in network byte order, `IPV6_UNICAST_IF` for IPv6
   in host byte order); the address alone is not an interface choice.
4. Watch with `NotifyIpInterfaceChange`, and on loss stop every running
   download with one clear reason, refuse new starts, and do not retry through
   the default route.
5. Make the kill switch a setting with a visible state in the status bar: an
   adapter chosen and lost is a state the user must see, not infer.
6. Say plainly in the settings hint that user-space monitoring cannot guarantee
   no packet escapes — an OS firewall rule is the stronger guarantee and is out
   of this app's scope.

## Files

`crates/idm-core/src/net/adapter.rs`, `crates/idm-core/src/net.rs`,
`crates/idm-core/src/transfer.rs`, `src-tauri/src/settings_store.rs`,
`src-tauri/src/state.rs`, `src/components/settings/connections-settings.tsx`,
`src/components/workspace/status-bar.tsx`.

## Edge cases

- A laptop switching Wi-Fi to Ethernet loses and regains an adapter in seconds;
  the switch must re-check rather than stop the world, but never continue on an
  unbound socket.
- A VPN that is "up" with no route must be treated as gone.
- Loopback and IPv6-only hosts.

## Tests

Unit: the byte-order rule per family, and the decision table (bound, lost,
recovered). Integration: bind to loopback only, download from the lab server,
confirm the socket is bound and the unbound target fails closed.

## Proof

`cargo test -p idm-core`, then the real app with a chosen adapter, a download,
and the adapter disabled mid-run — it must stop, not finish.

## Out of scope

Writing firewall rules; that needs elevation and belongs to the user.
