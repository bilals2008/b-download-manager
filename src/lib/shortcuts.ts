/**
 * The keyboard shortcuts the main window listens for, grouped by where they
 * apply. Kept as data so the Settings page and the search index read one copy.
 * There is no editor: every key is fixed in the code.
 */
export interface Shortcut {
  /** The row's anchor, and its key in the search index. */
  id: string;
  label: string;
  hint?: string;
  keys?: string;
  /** Not wired up yet, so the row shows this instead of a key. */
  soon?: boolean;
}

export interface ShortcutGroup {
  title: string;
  rows: readonly Shortcut[];
}

export const SHORTCUT_GROUPS: readonly ShortcutGroup[] = [
  {
    title: "Downloads",
    rows: [
      {
        id: "shortcut-add-download",
        label: "Add a download",
        hint: "Opens the Add URL dialog from anywhere in the main window.",
        keys: "Ctrl+N",
      },
      {
        id: "shortcut-search-downloads",
        label: "Search downloads",
        hint: "Searches the file name and the address.",
        keys: "Ctrl+F",
      },
      { id: "shortcut-find-next", label: "Find next", keys: "F3" },
      {
        id: "shortcut-find-previous",
        label: "Find previous",
        keys: "Shift+F3",
      },
      {
        id: "shortcut-remove-selected",
        label: "Remove selected",
        hint: "Works while the download list has the keyboard.",
        keys: "Delete",
      },
      {
        id: "shortcut-select-all",
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
        id: "shortcut-move",
        label: "Move through downloads",
        hint: "Hold Shift to extend the selection.",
        keys: "↑ / ↓",
      },
      {
        id: "shortcut-jump",
        label: "Jump to the first or last download",
        hint: "Hold Shift to extend the selection.",
        keys: "Home / End",
      },
      {
        id: "shortcut-clear-selection",
        label: "Clear the selection",
        keys: "Esc",
      },
      {
        id: "shortcut-context-menu",
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
        id: "shortcut-open-menu",
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
        id: "shortcut-open-downloads",
        label: "Open Downloads",
        hint: "Jumps to the download list from anywhere.",
        soon: true,
      },
      {
        id: "shortcut-open-settings",
        label: "Open Settings",
        hint: "Opens this window from the keyboard.",
        soon: true,
      },
      {
        id: "shortcut-help",
        label: "Help contents",
        hint: "Opens the help topic for the window.",
        soon: true,
      },
      {
        id: "shortcut-exit",
        label: "Exit",
        hint: "Active downloads ask before the window closes.",
        keys: "Alt+F4",
      },
    ],
  },
];
