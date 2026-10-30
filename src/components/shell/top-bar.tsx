import type { ToolbarSize } from "@/lib/settings";
import type { ToolbarPack } from "@/lib/toolbar-packs";
import type { ToolbarToken } from "@/lib/toolbars";

import type { ShellCommand } from "./commands";
import { MenuBar } from "./menu-bar";
import type { ToolbarMenuEntry } from "./menus";
import { TitleBar } from "./title-bar";
import { Toolbar } from "./toolbar";
import type { ToolbarDisabled } from "./toolbar-state";

interface TopBarProps {
  onCommand: (command: ShellCommand) => void;
  /** Menu items whose text depends on the state of the app. */
  menuLabels?: Partial<Record<ShellCommand, string>>;
  /** Which radio rows are picked, like the toolbar size and pack. */
  menuChecks?: Partial<Record<ShellCommand, boolean>>;
  /** Large buttons are the default; small ones make room for the list. */
  toolbarSize: ToolbarSize;
  /** The buttons of the current toolbar, in the chosen order. */
  toolbarLayout: readonly ToolbarToken[];
  /** The pack that paints the bar and may bring its own icons. */
  toolbarPack: ToolbarPack;
  /** Whether the buttons print their names. */
  toolbarShowLabels: boolean;
  /** The packs the View menu can switch to. */
  toolbars?: readonly ToolbarMenuEntry[];
  /** Toolbar buttons the current selection and list cannot use. */
  toolbarDisabled?: ToolbarDisabled;
}

export function TopBar({
  onCommand,
  menuLabels,
  menuChecks,
  toolbarSize,
  toolbarLayout,
  toolbarPack,
  toolbarShowLabels,
  toolbars,
  toolbarDisabled,
}: TopBarProps) {
  return (
    <header className="shrink-0 select-none">
      <TitleBar />
      <MenuBar
        onCommand={onCommand}
        menuLabels={menuLabels}
        menuChecks={menuChecks}
        toolbars={toolbars}
      />
      <Toolbar
        onCommand={onCommand}
        size={toolbarSize}
        layout={toolbarLayout}
        pack={toolbarPack}
        showLabels={toolbarShowLabels}
        disabled={toolbarDisabled}
      />
    </header>
  );
}
