import type { Download } from "@/lib/downloads";

import type { ShellCommand } from "./commands";

/** Buttons the toolbar has to dim, by command. Absent means "leave it alone". */
export type ToolbarDisabled = Partial<Record<ShellCommand, boolean>>;

/** A row the engine can start again: anything that is not done or running. */
function resumable(item: Download): boolean {
  return item.status !== "completed" && item.status !== "downloading";
}

/** A row that is on its way: queued or moving bytes. */
function live(item: Download): boolean {
  return item.status === "downloading" || item.status === "queued";
}

/** A row that can be stopped without losing what it already fetched. */
function stoppable(item: Download): boolean {
  return live(item) || item.status === "paused";
}

/**
 * Which buttons of the toolbar can do their job right now.
 *
 * The bar itself is fixed, the way IDM's is, but pressing a button that cannot
 * act is worse than not offering it: the row commands follow the selection, and
 * the three list commands follow what the list holds. Everything else is always
 * live, and the two screens that are not built yet stay dim for good.
 */
export function toolbarState(
  downloads: Download[],
  selected: number[],
): ToolbarDisabled {
  const picked = downloads.filter((item) => selected.includes(item.id));
  const anyPicked = (test: (item: Download) => boolean) => picked.some(test);
  const anyRow = (test: (item: Download) => boolean) => downloads.some(test);

  return {
    resume: !anyPicked(resumable),
    stop: !anyPicked(stoppable),
    delete: picked.length === 0,
    "stop-all": !anyRow(live),
    "delete-completed": !anyRow((item) => item.status === "completed"),
    "start-queue": !anyRow(
      (item) =>
        item.status === "paused" ||
        item.status === "queued" ||
        item.status === "stopped",
    ),
    "stop-queue": !anyRow((item) => item.status === "downloading"),
  };
}
