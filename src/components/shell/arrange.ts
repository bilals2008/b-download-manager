import type { SortKey } from "@/lib/downloads";

import type { ShellCommand } from "./commands";

/**
 * The order each View > Arrange files row asks for.
 *
 * The menu names an order once and the store takes the key, so the shell only
 * asks here instead of carrying a switch of its own.
 */
const ARRANGE: Partial<Record<ShellCommand, SortKey>> = {
  "arrange-order": "order",
  "arrange-name": "name",
  "arrange-size": "size",
  "arrange-status": "status",
  "arrange-eta": "eta",
  "arrange-speed": "speed",
  "arrange-last-try": "lastTry",
  "arrange-path": "path",
};

/** The sort a command names, or null when it names none. */
export function arrangeKey(command: ShellCommand): SortKey | null {
  return ARRANGE[command] ?? null;
}
