import type { ShellCommand } from "./commands";
import { MenuBar } from "./menu-bar";
import { TitleBar } from "./title-bar";
import { Toolbar } from "./toolbar";

interface TopBarProps {
  onCommand: (command: ShellCommand) => void;
  /** Menu items whose text depends on the state of the app. */
  menuLabels?: Partial<Record<ShellCommand, string>>;
}

export function TopBar({ onCommand, menuLabels }: TopBarProps) {
  return (
    <header className="shrink-0 select-none">
      <TitleBar />
      <MenuBar onCommand={onCommand} menuLabels={menuLabels} />
      <Toolbar onCommand={onCommand} />
    </header>
  );
}
