import type { ToolbarSize } from "@/lib/settings";

import type { ShellCommand } from "./commands";
import { MenuBar } from "./menu-bar";
import { TitleBar } from "./title-bar";
import { Toolbar } from "./toolbar";
import type { ToolbarDisabled } from "./toolbar-state";

interface TopBarProps {
  onCommand: (command: ShellCommand) => void;
  /** Menu items whose text depends on the state of the app. */
  menuLabels?: Partial<Record<ShellCommand, string>>;
  /** Which radio rows are picked. */
  menuChecks?: Partial<Record<ShellCommand, boolean>>;
  /** Large buttons are the default; small ones make room for the list. */
  toolbarSize: ToolbarSize;
  /** Toolbar buttons the current selection and list cannot use. */
  toolbarDisabled?: ToolbarDisabled;
}

export function TopBar({
  onCommand,
  menuLabels,
  menuChecks,
  toolbarSize,
  toolbarDisabled,
}: TopBarProps) {
  return (
    <header className="shrink-0 select-none">
      <TitleBar />
      <MenuBar
        onCommand={onCommand}
        menuLabels={menuLabels}
        menuChecks={menuChecks}
      />
      <Toolbar
        onCommand={onCommand}
        size={toolbarSize}
        disabled={toolbarDisabled}
      />
    </header>
  );
}
