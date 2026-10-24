import type { ComponentType } from "react";

import type { ShellCommand } from "./commands";
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

export interface ToolbarItem {
  command: ShellCommand;
  label: string;
  icon: ComponentType<ToolbarIconProps>;
  /** An accelerator the tooltip can print on a key cap. */
  shortcut?: string;
  /** Toolbar buttons whose screen is not built yet stay visible but inert. */
  disabled?: boolean;
  /** Why a button is inert, in the tooltip instead of a silent dead control. */
  note?: string;
}

export const TOOLBAR_ITEMS: ToolbarItem[] = [
  { command: "add-download", label: "Add URL", icon: AddIcon, shortcut: "Ctrl+N" },
  { command: "resume", label: "Resume", icon: ResumeIcon },
  { command: "stop", label: "Stop", icon: StopIcon },
  { command: "stop-all", label: "Stop All", icon: StopAllIcon },
  { command: "delete", label: "Delete", icon: DeleteIcon },
  { command: "delete-completed", label: "Delete Completed", icon: DeleteCompletedIcon },
  { command: "options", label: "Options", icon: OptionsIcon },
  {
    command: "scheduler",
    label: "Scheduler",
    icon: SchedulerIcon,
    disabled: true,
    note: "Queues and schedules are coming soon",
  },
  { command: "start-queue", label: "Start Queue", icon: StartQueueIcon },
  { command: "stop-queue", label: "Stop Queue", icon: StopQueueIcon },
  { command: "tell-friend", label: "Tell a Friend", icon: TellFriendIcon, disabled: true },
];
