/**
 * The proxy auto-config script: the address it is fetched from, and the
 * script itself.
 */

import { invoke } from "@/lib/ipc";

/** What the reader knows about the script. */
export interface ScriptState {
  /** The address the script is fetched from; empty when there is none. */
  url: string;
  /** Whether the script is in force right now. */
  loaded: boolean;
  /** What the script is doing, in one line. */
  note: string;
}

/**
 * Fetch the script at `url` and put it in force.
 *
 * The file is fetched before it is stored: an address that does not answer
 * is refused here rather than leaving every download without a route. An
 * empty address means no script, and the proxy address is used for every
 * download.
 */
export function setProxyScript(url: string): Promise<ScriptState> {
  return invoke<ScriptState>("set_proxy_script", { url });
}

/** Fetch the script again, now. */
export function reloadProxyScript(): Promise<ScriptState> {
  return invoke<ScriptState>("reload_proxy_script");
}

/** What the reader knows about the script. */
export function proxyScriptState(): Promise<ScriptState> {
  return invoke<ScriptState>("proxy_script_state");
}
