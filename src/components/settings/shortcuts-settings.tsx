import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { KeyCap } from "@/components/ui/key-cap";

interface Shortcut {
  label: string;
  hint?: string;
  keys?: string;
  /** Not wired up yet, so the row shows this instead of a key. */
  soon?: boolean;
}

interface ShortcutGroup {
  title: string;
  rows: Shortcut[];
}

/**
 * Every key listed here is handled by the code that owns the action: the shell
 * accelerators, the download list and its menu bar. Anything that is not built
 * says so, rather than printing a key that does nothing.
 */
const GROUPS: readonly ShortcutGroup[] = [
  {
    title: "Downloads",
    rows: [
      {
        label: "Add a download",
        hint: "Opens the Add URL dialog from anywhere in the main window.",
        keys: "Ctrl+N",
      },
      {
        label: "Search downloads",
        hint: "Searches the file name and the address.",
        keys: "Ctrl+F",
      },
      { label: "Find next", keys: "F3" },
      { label: "Find previous", keys: "Shift+F3" },
      {
        label: "Remove selected",
        hint: "Works while the download list has the keyboard.",
        keys: "Delete",
      },
      {
        label: "Select all",
        hint: "Works while the download list has the keyboard.",
        keys: "Ctrl+A",
      },
    ],
  },
  {
    title: "The download list",
    rows: [
      {
        label: "Move through downloads",
        hint: "Hold Shift to extend the selection.",
        keys: "↑ / ↓",
      },
      {
        label: "Jump to the first or last download",
        hint: "Hold Shift to extend the selection.",
        keys: "Home / End",
      },
      { label: "Clear the selection", keys: "Esc" },
      {
        label: "Open the context menu",
        hint: "The Menu key on the keyboard does the same.",
        keys: "Shift+F10",
      },
    ],
  },
  {
    title: "Menu bar",
    rows: [
      {
        label: "Open a menu",
        hint: "Alt+F, Alt+D, Alt+V and Alt+H open the other menus.",
        keys: "Alt+T",
      },
    ],
  },
  {
    title: "Application",
    rows: [
      {
        label: "Open Downloads",
        hint: "Jumps to the download list from anywhere.",
        soon: true,
      },
      {
        label: "Open Settings",
        hint: "Opens this window from the keyboard.",
        soon: true,
      },
      { label: "Help contents", hint: "Opens the help topic for the window.", soon: true },
      {
        label: "Exit",
        hint: "Active downloads ask before the window closes.",
        keys: "Alt+F4",
      },
    ],
  },
];

/**
 * Keyboard shortcuts: what the main window listens for, grouped by where it
 * applies. There is no editor here, because every key is fixed in the code.
 */
export function ShortcutsSettings() {
  return (
    <SettingsPage
      title="Shortcuts"
      description="Keys that work in the main window."
    >
      {GROUPS.map((group) => (
        <Section key={group.title} title={group.title}>
          {group.rows.map((row) => (
            <SettingRow key={row.label} label={row.label} hint={row.hint}>
              {row.soon ? (
                <span className="text-chrome-micro text-chrome-muted">
                  Coming soon
                </span>
              ) : row.keys ? (
                <KeyCap>{row.keys}</KeyCap>
              ) : null}
            </SettingRow>
          ))}
        </Section>
      ))}
    </SettingsPage>
  );
}
