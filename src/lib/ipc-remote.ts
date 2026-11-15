/**
 * The phone and LAN remote.
 *
 * Off until the reader turns it on. A phone on the same home network pairs
 * once with a one-time code, after which it may list, add, pause, resume and
 * remove downloads. Pairing happens over plain HTTP on a trusted network and
 * the page says so.
 */

import { invoke } from "@/lib/ipc";

/** What the window shows about the remote. */
export interface RemoteStatus {
  /** Whether the listener is on at all. */
  enabled: boolean;
  /** The address the phone reaches the remote on, with its port. */
  address: string | null;
  /** The devices that may talk to it. */
  devices: RemoteDevice[];
}

/** A device that paired, as the window shows it. */
export interface RemoteDevice {
  /** The device's id, for revocation. */
  id: string;
  /** The name the phone was paired under. */
  name: string;
  /** The moment it paired, in milliseconds since the epoch. */
  pairedAt: number;
}

/** What the remote is doing, and who may talk to it. */
export function remoteStatus(): Promise<RemoteStatus> {
  return invoke<RemoteStatus>("remote_status");
}

/** Turn the remote on or off. */
export function remoteSetEnabled(enabled: boolean): Promise<RemoteStatus> {
  return invoke<RemoteStatus>("remote_set_enabled", { enabled });
}

/** The code a phone pairs with: the code that is good now, or a fresh one. */
export function remotePairingCode(): Promise<{ code: string; expiresAt: number } | null> {
  return invoke<{ code: string; expiresAt: number } | null>("remote_pairing_code");
}

/** Forget a device; its token stops working at once. */
export function remoteRevoke(deviceId: string): Promise<RemoteDevice[]> {
  return invoke<RemoteDevice[]>("remote_revoke", { deviceId });
}
