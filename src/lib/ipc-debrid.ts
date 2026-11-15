/**
 * The debrid accounts, and the host links they unlock.
 *
 * A key lives in the credential store of the operating system and nowhere
 * else: not in the settings, not in the downloads index and not in a log
 * line. The window only ever sees the account's name and how much paid
 * time is left.
 *
 * The feature is experimental: a service that is down is said plainly, and
 * the download then starts on the original link when the reader asks for it.
 */

import { invoke } from "@/lib/ipc";

/** What the settings page shows about one service. */
export interface DebridService {
  /** The short name of the service ("real-debrid"). */
  id: string;
  /** The name the service writes on itself ("Real-Debrid"). */
  name: string;
  /** Whether the reader is signed in. */
  signedIn: boolean;
  /** Whether the account may use the paid features. */
  premium: boolean;
  /** The name the service gave the account. */
  user: string | null;
  /** The paid days left. */
  premiumDays: number | null;
  /** The page where the reader finds the account's key. */
  tokenPage: string;
}

/** The state of one account, without its key. */
export interface AccountState {
  /** The short name of the service ("real-debrid"). */
  kind: string;
  /** The name the service writes on itself. */
  service: string;
  /** The name the service gave the account. */
  user: string;
  /** Whether the account may use the paid features. */
  premium: boolean;
  /** The paid days left. */
  premiumDays: number | null;
}

/** The services, and what the reader is signed in to. */
export function debridServices(): Promise<DebridService[]> {
  return invoke<DebridService[]>("debrid_services");
}

/** Sign in to a service with the key the reader pasted. */
export function debridSignIn(kind: string, token: string): Promise<AccountState> {
  return invoke<AccountState>("debrid_sign_in", { kind, token });
}

/** Sign out of a service, and forget its key. */
export function debridSignOut(kind: string): Promise<void> {
  return invoke<void>("debrid_sign_out", { kind });
}

/** Check that the key of a service still works. */
export function debridTest(kind: string): Promise<void> {
  return invoke<void>("debrid_test", { kind });
}

/** Turn the automatic switch on or off. */
export function debridSetAuto(enabled: boolean): Promise<void> {
  return invoke<void>("debrid_set_auto", { enabled });
}

/** Whether every host link a signed-in service can take is unlocked. */
export function debridAuto(): Promise<boolean> {
  return invoke<boolean>("debrid_auto");
}

/** Remember that the reader wants this link unlocked. */
export function debridAskUnlock(url: string): Promise<void> {
  return invoke<void>("debrid_ask_unlock", { url });
}
