/**
 * The connections a host allowed the last time it was asked.
 *
 * The engine keeps a per-host cap so a 2-connection server is not asked for
 * more ranges every download. The cap outlives the run; the reader can
 * forget it when the network changed.
 */

import { invoke } from "@/lib/ipc";

/** Forget what this host allowed last time; the next download starts from
 * the reader's own connection count instead of the learned cap. */
export function resetLearnedHost(host: string): Promise<void> {
  return invoke<void>("reset_learned_host", { host });
}
