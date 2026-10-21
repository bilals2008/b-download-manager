import type { ShellCommand } from "./commands";

export interface MenuItem {
  command: ShellCommand;
  label: string;
  shortcut?: string;
  /** Commands whose screen is not built yet stay visible but inert. */
  disabled?: boolean;
  /** A nested menu, the way IDM groups "Export", "Arrange files" and friends. */
  submenu?: MenuRow[];
}

/** `"separator"` draws the hairline rule between groups of commands. */
export type MenuRow = MenuItem | "separator";

export interface MenuDefinition {
  id: string;
  label: string;
  accessKey: string;
  rows: MenuRow[];
}

/**
 * The menus of IDM 6.43, read out of the string table of `IDMan.exe`: the
 * top level is Tasks, File, Downloads, View, Help and Registration, and the
 * groups below them ("Export", "Arrange files", "Toolbar", "Speed Limiter")
 * are nested menus in the original as well.
 */
export const MENUS: MenuDefinition[] = [
  {
    id: "tasks",
    label: "Tasks",
    accessKey: "t",
    rows: [
      { command: "add-download", label: "Add new download...", shortcut: "Ctrl+N" },
      { command: "add-batch", label: "Add batch download...", disabled: true },
      {
        command: "add-batch-clipboard",
        label: "Add batch download from clipboard",
        disabled: true,
      },
      { command: "run-grabber", label: "Run site grabber", disabled: true },
      { command: "drop-target", label: "Show drop target", disabled: true },
      "separator",
      {
        command: "export-list",
        label: "Export",
        submenu: [
          { command: "export-list", label: "To IDM export file", disabled: true },
          { command: "export-text", label: "To text file", disabled: true },
        ],
      },
      {
        command: "import-list",
        label: "Import",
        submenu: [
          { command: "import-list", label: "From IDM export file", disabled: true },
          { command: "import-text", label: "From text file", disabled: true },
        ],
      },
      "separator",
      { command: "exit", label: "Exit", shortcut: "Alt+F4" },
    ],
  },
  {
    id: "file",
    label: "File",
    accessKey: "f",
    rows: [
      { command: "stop", label: "Stop Download" },
      { command: "delete", label: "Remove" },
      { command: "resume", label: "Download Now" },
      { command: "redownload", label: "Redownload" },
    ],
  },
  {
    id: "downloads",
    label: "Downloads",
    accessKey: "d",
    rows: [
      { command: "pause-all", label: "Pause All" },
      { command: "stop-all", label: "Stop All" },
      { command: "delete-completed", label: "Delete All Completed" },
      "separator",
      { command: "find", label: "Find", shortcut: "Ctrl-F", disabled: true },
      { command: "find-next", label: "Find Next", shortcut: "F3", disabled: true },
      { command: "scheduler", label: "Scheduler", disabled: true },
      "separator",
      { command: "options", label: "Options..." },
      "separator",
      {
        command: "start-queue",
        label: "Start queue",
        submenu: [{ command: "start-queue", label: "Start" }],
      },
      {
        command: "stop-queue",
        label: "Stop queue",
        submenu: [{ command: "stop-queue", label: "Stop" }],
      },
      {
        command: "speed-limiter-on",
        label: "Speed Limiter",
        submenu: [
          { command: "speed-limiter-on", label: "Turn on", disabled: true },
          { command: "speed-limiter-off", label: "Turn off", disabled: true },
          { command: "speed-limiter-settings", label: "Settings...", disabled: true },
          { command: "speed-limiter-options", label: "Options", disabled: true },
        ],
      },
    ],
  },
  {
    id: "view",
    label: "View",
    accessKey: "v",
    rows: [
      { command: "categories", label: "Hide categories" },
      "separator",
      {
        command: "arrange-order",
        label: "Arrange files",
        submenu: [
          { command: "arrange-order", label: "By Order Of Addition" },
          { command: "arrange-name", label: "By File Name" },
          { command: "arrange-size", label: "By Size" },
          { command: "arrange-status", label: "By Status" },
          { command: "arrange-eta", label: "By Time Left" },
          { command: "arrange-speed", label: "By Transfer Rate" },
          { command: "arrange-last-try", label: "By Last Try Date" },
          { command: "arrange-path", label: "By Save Path" },
        ],
      },
      {
        command: "toolbar-large",
        label: "Toolbar",
        submenu: [
          { command: "toolbar-customize", label: "Customize...", disabled: true },
          { command: "toolbar-large", label: "Large Buttons" },
          { command: "toolbar-small", label: "Small Buttons" },
          { command: "toolbar-new", label: "Look for new toolbars", disabled: true },
          { command: "toolbar-neon", label: "Neon", disabled: true },
        ],
      },
      {
        command: "tray-3d",
        label: "IDM tray icon",
        submenu: [
          { command: "tray-3d", label: "3D style", disabled: true },
          { command: "tray-classic", label: "Classic style", disabled: true },
          { command: "tray-hidden", label: "Don't show", disabled: true },
          { command: "tray-customize-list", label: "Customize URL List...", disabled: true },
          { command: "tray-dark-mode", label: "Dark Mode support", disabled: true },
        ],
      },
      {
        command: "font-choose",
        label: "Font",
        submenu: [
          { command: "font-choose", label: "Choose Font...", disabled: true },
          { command: "font-reset", label: "Reset to default Font", disabled: true },
        ],
      },
      {
        command: "language-english",
        label: "Language",
        submenu: [{ command: "language-english", label: "English", disabled: true }],
      },
    ],
  },
  {
    id: "help",
    label: "Help",
    accessKey: "h",
    rows: [
      { command: "help-contents", label: "Help contents", shortcut: "F1", disabled: true },
      { command: "tutorials", label: "Tutorials", disabled: true },
      { command: "help-scheduler-queues", label: "Scheduler and queues", disabled: true },
      { command: "grabber-help", label: "Grabber Help", disabled: true },
      { command: "tip-of-the-day", label: "Tip of the Day...", disabled: true },
      "separator",
      { command: "website", label: "IDM Home Page", disabled: true },
      { command: "support", label: "Contact IDM Support", disabled: true },
      { command: "check-updates", label: "Check for updates...", disabled: true },
      "separator",
      { command: "about", label: "About IDM..." },
      { command: "tell-friend", label: "Tell a friend", disabled: true },
    ],
  },
  {
    id: "registration",
    label: "Registration",
    accessKey: "r",
    rows: [
      { command: "order-online", label: "Order online", disabled: true },
      { command: "registration", label: "Registration...", disabled: true },
    ],
  },
];
