import type { ToolbarPack } from "@/lib/toolbar-packs";
import type { ToolbarCommand } from "@/lib/toolbars";

import { TOOLBAR_BY_COMMAND } from "./toolbar-items";

interface ToolbarPackIconProps {
  command: ToolbarCommand;
  pack: ToolbarPack;
  className?: string;
}

/**
 * The drawing of one button: the pack's own image when it brings one, the
 * built-in component otherwise. Shared by the bar and the settings preview so
 * a style looks the same in both.
 */
export function ToolbarPackIcon({ command, pack, className }: ToolbarPackIconProps) {
  const source = pack.icons[command];
  if (source) {
    return (
      <img
        src={source}
        alt=""
        aria-hidden="true"
        draggable={false}
        className={className ? `${className} object-contain` : "object-contain"}
      />
    );
  }

  const item = TOOLBAR_BY_COMMAND.get(command);
  if (!item) {
    return null;
  }
  const Icon = item.icon;
  return <Icon className={className} />;
}
