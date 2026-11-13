/**
 * The network adapter every connection leaves through, and the kill switch
 * that stops the downloads when it goes away.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { invoke } from "@/lib/ipc";

/** A network adapter of this computer, as the settings page shows it. */
export interface NetworkAdapter {
  /** The system's identity for it: what the settings file keeps. */
  id: string;
  /** Its name as the system shows it ("Ethernet", "ProtonVPN"). */
  name: string;
  /** Its first address, empty when it has none. */
  address: string;
  /** Whether the system says it is up. */
  up: boolean;
}

/** The chosen adapter's state, as the status bar shows it. */
export interface NetworkState {
  /** The adapter's name; empty when no adapter is chosen. */
  label: string;
  /** "none", "connected" or "lost". */
  state: "none" | "connected" | "lost";
}

/** The adapters of this computer. */
export function networkAdapters(): Promise<NetworkAdapter[]> {
  return invoke<NetworkAdapter[]>("network_adapters");
}

/** The chosen adapter's state. */
export function networkState(): Promise<NetworkState> {
  return invoke<NetworkState>("network_state");
}

/**
 * Choose the adapter every connection leaves through.
 *
 * An empty id means the system's own route. A chosen adapter that is not
 * there is not a fallback: the downloads stop instead.
 */
export function setNetworkAdapter(id: string): Promise<NetworkState> {
  return invoke<NetworkState>("set_network_adapter", { id });
}

/** The chosen adapter's state, when it changes. */
export function onNetworkState(
  handler: (state: NetworkState) => void,
): Promise<UnlistenFn> {
  return listen<NetworkState>("network://adapter", (event) =>
    handler(event.payload),
  );
}
