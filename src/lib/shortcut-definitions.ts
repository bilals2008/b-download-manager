/**
 * The keyboard shortcut registry's data.
 *
 * Every key the application listens for lives here as data: a command id, a
 * default combination, and the scope that decides where the key is heard. The
 * reader's changes are stored as overrides in `localStorage`, beside the other
 * saved preferences, so a rebound key survives a restart and every window
 * reads one copy.
 *
 * The Settings page, the menus, the toolbar tooltips and the dispatcher all
 * ask `@/lib/shortcuts` what the key is; nothing hardcodes a combination
 * elsewhere.
 */

import type { ShellCommand } from "@/components/shell/commands";

/** Where a key is heard: anywhere in the shell, or only in the download list. */
export type ShortcutScope = "global" | "list";

export type ShortcutId =
  // Downloads
  | "add-download"
  | "find"
  | "find-next"
  | "find-previous"
  | "open-downloads-folder"
  | "pause-all"
  | "resume-all"
  | "stop-all"
  // The selection
  | "pause-selected"
  | "resume-selected"
  | "cancel-selected"
  | "remove-selected"
  | "select-all"
  | "clear-selection"
  // Queues
  | "start-queue"
  | "stop-queue"
  // The window
  | "open-settings"
  | "cycle-sections"
  | "cycle-sections-back";

export interface ShortcutDefinition {
  id: ShortcutId;
  label: string;
  /** The row's tooltip and search description. */
  hint: string;
  group: string;
  scope: ShortcutScope;
  /** The shipped combination, in canonical form, or null when it starts clear. */
  defaultBinding: string | null;
}

/**
 * The shipped keys. Windows conventions where one exists (Ctrl+N, Ctrl+F, F3,
 * Ctrl+A, Delete, Ctrl+,), Ctrl+Shift for actions on the selection, and
 * Ctrl+Alt for actions on every download at once, so the two families never
 * collide with each other or with the menu bar's Alt+letter access keys.
 */
export const SHORTCUT_DEFINITIONS: readonly ShortcutDefinition[] = [
  {
    id: "add-download",
    label: "Add a download",
    hint: "Opens the Add URL dialog from anywhere in the main window.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+N",
  },
  {
    id: "find",
    label: "Search downloads",
    hint: "Searches the file name and the address.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+F",
  },
  {
    id: "find-next",
    label: "Find next",
    hint: "Steps to the next match. Works while typing in the Find box.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "F3",
  },
  {
    id: "find-previous",
    label: "Find previous",
    hint: "Steps back to the previous match.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Shift+F3",
  },
  {
    id: "open-downloads-folder",
    label: "Open Downloads folder",
    hint: "Opens the folder new downloads are saved to.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+Shift+O",
  },
  {
    id: "pause-all",
    label: "Pause all downloads",
    hint: "Pauses every download that is running.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+Alt+P",
  },
  {
    id: "resume-all",
    label: "Resume all downloads",
    hint: "Starts every download that is paused or stopped.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+Alt+R",
  },
  {
    id: "stop-all",
    label: "Stop all downloads",
    hint: "Cancels every download that is running.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+Alt+S",
  },
  {
    id: "pause-selected",
    label: "Pause selected",
    hint: "Pauses the downloads the list has selected.",
    group: "Selected downloads",
    scope: "global",
    defaultBinding: "Ctrl+Shift+P",
  },
  {
    id: "resume-selected",
    label: "Resume selected",
    hint: "Starts the selected downloads again from where they stopped.",
    group: "Selected downloads",
    scope: "global",
    defaultBinding: "Ctrl+Shift+Enter",
  },
  {
    id: "cancel-selected",
    label: "Cancel selected",
    hint: "Stops the selected downloads without removing them.",
    group: "Selected downloads",
    scope: "global",
    defaultBinding: "Ctrl+Shift+S",
  },
  {
    id: "remove-selected",
    label: "Remove selected",
    hint: "Works while the download list has the keyboard.",
    group: "Selected downloads",
    scope: "list",
    defaultBinding: "Delete",
  },
  {
    id: "select-all",
    label: "Select all",
    hint: "Selects every download the list is showing.",
    group: "Selection",
    scope: "global",
    defaultBinding: "Ctrl+A",
  },
  {
    id: "clear-selection",
    label: "Deselect all",
    hint: "Esc also clears the selection while the list has the keyboard.",
    group: "Selection",
    scope: "global",
    defaultBinding: "Ctrl+Shift+A",
  },
  {
    id: "start-queue",
    label: "Start the queue",
    hint: "Runs the next download in the queue.",
    group: "Queues",
    scope: "global",
    defaultBinding: "Ctrl+Alt+Q",
  },
  {
    id: "stop-queue",
    label: "Stop the queue",
    hint: "Holds the queue until it is started again.",
    group: "Queues",
    scope: "global",
    defaultBinding: null,
  },
  {
    id: "open-settings",
    label: "Open Settings",
    hint: "Opens the settings window.",
    group: "The window",
    scope: "global",
    defaultBinding: "Ctrl+,",
  },
  {
    id: "cycle-sections",
    label: "Move to the next section",
    hint: "Steps the keyboard through the menu bar, the categories and the list.",
    group: "The window",
    scope: "global",
    defaultBinding: "Ctrl+Tab",
  },
  {
    id: "cycle-sections-back",
    label: "Move to the previous section",
    hint: "Steps the keyboard backwards through the same three places.",
    group: "The window",
    scope: "global",
    defaultBinding: "Ctrl+Shift+Tab",
  },
];

/**
 * The keys that belong to a control's own behavior — the list navigation a
 * desktop user expects, the menu bar's access keys, the window close. They are
 * printed on the Settings page but cannot be rebound, because the control they
 * drive would stop working quietly. The `id` is the anchor a search result
 * scrolls to, in the same shape a command row uses.
 */
export const FIXED_SHORTCUT_ROWS: readonly {
  id: string;
  label: string;
  keys: string;
  hint?: string;
}[] = [
  {
    id: "shortcut-fixed-navigation",
    label: "Move through downloads",
    keys: "↑ / ↓",
    hint: "Hold Shift to extend the selection.",
  },
  {
    id: "shortcut-fixed-jump",
    label: "Jump to the first or last download",
    keys: "Home / End",
    hint: "Hold Shift to extend the selection.",
  },
  { id: "shortcut-fixed-clear-esc", label: "Clear the selection", keys: "Esc" },
  {
    id: "shortcut-fixed-context-menu",
    label: "Open the context menu",
    keys: "Shift+F10",
    hint: "The Menu key on the keyboard does the same.",
  },
  {
    id: "shortcut-fixed-menu-bar",
    label: "Open a menu",
    keys: "Alt+T",
    hint: "Alt+F, Alt+D, Alt+V and Alt+H open the other menus.",
  },
  {
    id: "shortcut-fixed-exit",
    label: "Exit",
    keys: "Alt+F4",
    hint: "Active downloads ask before the window closes.",
  },
];

export const DEFINITION_BY_ID = new Map(
  SHORTCUT_DEFINITIONS.map((definition) => [definition.id, definition]),
);

export function shortcutDefinition(id: ShortcutId): ShortcutDefinition {
  const found = DEFINITION_BY_ID.get(id);
  if (!found) {
    throw new Error(`Unknown shortcut id: ${id}`);
  }
  return found;
}

/**
 * The shortcut each menu and toolbar command prints on its row. A command
 * not listed here, or whose key the reader cleared, shows nothing.
 */
export const COMMAND_SHORTCUT: Partial<Record<ShellCommand, ShortcutId>> = {
  "add-download": "add-download",
  find: "find",
  "find-next": "find-next",
  stop: "cancel-selected",
  resume: "resume-selected",
  delete: "remove-selected",
  "pause-all": "pause-all",
  "stop-all": "stop-all",
  options: "open-settings",
  "start-queue": "start-queue",
  "stop-queue": "stop-queue",
};
