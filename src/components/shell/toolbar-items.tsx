import type { ComponentType } from "react";

import {
  TOOLBAR_CATALOG,
  type ToolbarCommand,
  type ToolbarItemMeta,
} from "@/lib/toolbars";

import {
  AddIcon,
  DeleteCompletedIcon,
  DeleteIcon,
  OptionsIcon,
  ResumeIcon,
  SchedulerIcon,
  StartQueueIcon,
  StopAllIcon,
  StopIcon,
  StopQueueIcon,
  TellFriendIcon,
  type ToolbarIconProps,
} from "./toolbar-icons";

export interface ToolbarItem extends ToolbarItemMeta {
  icon: ComponentType<ToolbarIconProps>;
}

/**
 * The artwork of every catalogue button. The record is keyed by the command
 * union, so adding a button to the catalogue without drawing it is a compile
 * error rather than an empty square on the bar.
 */
const ICONS: Record<ToolbarCommand, ComponentType<ToolbarIconProps>> = {
  "add-download": AddIcon,
  resume: ResumeIcon,
  stop: StopIcon,
  "stop-all": StopAllIcon,
  delete: DeleteIcon,
  "delete-completed": DeleteCompletedIcon,
  options: OptionsIcon,
  scheduler: SchedulerIcon,
  "start-queue": StartQueueIcon,
  "stop-queue": StopQueueIcon,
  "tell-friend": TellFriendIcon,
};

export const TOOLBAR_ITEMS: readonly ToolbarItem[] = TOOLBAR_CATALOG.map(
  (item) => ({ ...item, icon: ICONS[item.command] }),
);

/** The button behind a command, for a layout that is stored as names. */
export const TOOLBAR_BY_COMMAND: ReadonlyMap<ToolbarCommand, ToolbarItem> = new Map(
  TOOLBAR_ITEMS.map((item) => [item.command, item]),
);
