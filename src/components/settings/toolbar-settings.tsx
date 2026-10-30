import { useId } from "react";

import { CheckGlyph } from "@/components/icons/ui-icons";
import { SettingsButton } from "@/components/settings/settings-button";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { ToolbarPackIcon } from "@/components/shell/toolbar-icon";
import { Switch } from "@/components/ui/switch";
import type { ToolbarSize } from "@/lib/settings";
import type { ToolbarPack } from "@/lib/toolbar-packs";
import type { ToolbarCommand, ToolbarToken } from "@/lib/toolbars";
import { cn } from "@/lib/utils";

export interface ToolbarSettingsProps {
  /** The pack in use. */
  pack: ToolbarPack;
  /** Every bar the reader can switch to, built in and installed. */
  packs: readonly ToolbarPack[];
  /** The buttons of the active pack, in order. */
  layout: readonly ToolbarToken[];
  size: ToolbarSize;
  showLabels: boolean;
  onSelectPack: (id: string) => void;
  onSize: (size: ToolbarSize) => void;
  onShowLabels: (show: boolean) => void;
  onCustomize: () => void;
  onManage: () => void;
}

/** The drawings that name a style in the list, one colourful, one alerting. */
const SAMPLE: readonly ToolbarCommand[] = ["add-download", "stop", "options"];

/**
 * The Toolbar page of the options window.
 *
 * The style list, the size switch and the label switch write the same state
 * the bar reads, so every change lands at once. Customize and Manage open the
 * same two dialogs the View menu opens.
 */
export function ToolbarSettings({
  pack,
  packs,
  layout,
  size,
  showLabels,
  onSelectPack,
  onSize,
  onShowLabels,
  onCustomize,
  onManage,
}: ToolbarSettingsProps) {
  const radioName = useId();
  const buttonCount = layout.filter((token) => token !== "separator").length;

  return (
    <SettingsPage
      title="Toolbar"
      description="The bar under the menu: its style, size and buttons."
    >
      <Section title="Style">
        <fieldset className="flex flex-col">
          <legend className="sr-only">Toolbar style</legend>
          {packs.map((choice) => {
            const active = choice.id === pack.id;
            return (
              <label
                key={choice.id}
                className={cn(
                  "flex cursor-pointer items-center gap-3 rounded-chrome px-2 py-2 transition-colors focus-within:ring-2 focus-within:ring-chrome-focus focus-within:ring-inset",
                  active ? "bg-chrome-menu-active" : "hover:bg-chrome-menu-hover",
                )}
              >
                <input
                  type="radio"
                  name={radioName}
                  value={choice.id}
                  aria-label={choice.name}
                  checked={active}
                  onChange={() => onSelectPack(choice.id)}
                  className="sr-only"
                />
                <span
                  aria-hidden="true"
                  className="flex w-16 shrink-0 items-center gap-0.5"
                >
                  {SAMPLE.map((command) => (
                    <ToolbarPackIcon
                      key={command}
                      command={command}
                      pack={choice}
                      className="size-5"
                    />
                  ))}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-xs text-chrome-title-text">
                    {choice.name}
                  </span>
                  <span className="block truncate text-chrome-micro text-chrome-muted">
                    {choice.description}
                  </span>
                </span>
                <CheckGlyph
                  className={cn(
                    "size-3.5 shrink-0 text-chrome-focus",
                    active ? "opacity-100" : "opacity-0",
                  )}
                />
              </label>
            );
          })}
        </fieldset>
      </Section>

      <Section title="Size and labels">
        <SettingRow
          label="Large buttons"
          hint="Small buttons leave more room for the download list."
        >
          <Switch
            label="Large buttons"
            checked={size === "large"}
            onChange={(large) => onSize(large ? "large" : "small")}
          />
        </SettingRow>
        <SettingRow
          label="Show text labels"
          hint="The tooltip names the button either way."
        >
          <Switch
            label="Show text labels"
            checked={showLabels}
            onChange={onShowLabels}
          />
        </SettingRow>
      </Section>

      <Section title="Buttons">
        <SettingRow
          label="Customize buttons"
          hint={`${buttonCount} button${buttonCount === 1 ? "" : "s"} on this bar. Add, remove, reorder or group them.`}
        >
          <SettingsButton aria-haspopup="dialog" onClick={onCustomize}>
            Customize...
          </SettingsButton>
        </SettingRow>
        <SettingRow
          label="More toolbars"
          hint="Install a pack from a file, replace one or remove it."
        >
          <SettingsButton aria-haspopup="dialog" onClick={onManage}>
            Manage toolbars...
          </SettingsButton>
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
