/**
 * The report of the last time the window stopped answering.
 *
 * The monitor writes it beside the log while the application is not looking,
 * so the next start can say what happened instead of the reader guessing.
 */

import { invoke } from "@/lib/ipc";

/** What the monitor left behind. */
export interface FreezeReport {
  /** Where the report and the stacks of the stopped program are. */
  folder: string;
  /** The report as it was written. */
  text: string;
  /** When the report was written, in seconds since the epoch. */
  writtenAt: number;
}

/** The last freeze report, when there was one. */
export function lastFreezeReport(): Promise<FreezeReport | null> {
  return invoke<FreezeReport | null>("last_freeze_report");
}
