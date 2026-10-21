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
  /** Toolbar buttons whose screen is not built yet stay visible but inert. */
  disabled?: boolean;
}

export const TOOLBAR_ITEMS: ToolbarItem[] = [
  { command: "add-download", label: "Add URL", icon: AddIcon },
  { command: "resume", label: "Resume", icon: ResumeIcon },
  { command: "stop", label: "Stop", icon: StopIcon },
  { command: "stop-all", label: "Stop All", icon: StopAllIcon },
  { command: "delete", label: "Delete", icon: DeleteIcon },
  { command: "delete-completed", label: "Delete Completed", icon: DeleteCompletedIcon },
  { command: "options", label: "Options", icon: OptionsIcon },
  { command: "scheduler", label: "Scheduler", icon: SchedulerIcon, disabled: true },
  { command: "start-queue", label: "Start Queue", icon: StartQueueIcon },
  { command: "stop-queue", label: "Stop Queue", icon: StopQueueIcon },
  { command: "tell-friend", label: "Tell a Friend", icon: TellFriendIcon, disabled: true },
];
