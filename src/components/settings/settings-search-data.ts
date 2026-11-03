import {
  SETTINGS_PAGES,
  type SettingsPageId,
} from "@/components/settings/settings-nav";
import {
  buildSettingsSearchIndex,
  groupSettingsHits,
  searchSettings,
  type SettingsSearchEntry,
  type SettingsSearchGroup,
  type SettingsSearchHit,
} from "@/lib/settings-search";
import { shortcutSearchRows } from "@/lib/shortcuts";

export interface SettingsCatalogEntry extends SettingsSearchEntry {
  page: SettingsPageId;
}

/**
 * Every setting the window owns, in one place. The page still draws its own
 * row; this list only says what that row is called, what it does and which
 * element a search result should open. A row's `settingId` is the anchor here.
 */
const PAGE_ENTRIES: readonly SettingsCatalogEntry[] = [
  {
    id: "setting-launch-startup",
    page: "general",
    section: "Startup",
    title: "Launch on startup",
    description: "Adds an entry to the startup of this computer.",
    keywords: [
      "startup",
      "start up",
      "autostart",
      "auto start",
      "boot",
      "login",
      "launch at login",
      "start with windows",
      "run at startup",
    ],
  },
  {
    id: "setting-clipboard",
    page: "general",
    section: "Clipboard",
    title: "Watch the clipboard for download links",
    description: "Opens the Add URL dialog first.",
    keywords: [
      "clipboard",
      "copy",
      "paste",
      "links",
      "link capture",
      "monitor",
      "add url",
    ],
  },
  {
    id: "setting-minimize-tray",
    page: "general",
    section: "The tray",
    title: "Minimize the window to the tray",
    description: "The tray icon brings the window back.",
    keywords: [
      "tray",
      "minimize",
      "minimise",
      "taskbar",
      "notification area",
      "hide window",
    ],
  },
  {
    id: "setting-close-tray",
    page: "general",
    section: "The tray",
    title: "Keep running in the tray when the window is closed",
    description: "Downloads carry on after the window closes.",
    keywords: [
      "tray",
      "close",
      "background",
      "quit",
      "exit",
      "keep running",
      "close to tray",
    ],
  },
  {
    id: "setting-tray-icon",
    page: "general",
    section: "The tray",
    title: "Show the tray icon",
    description: "Off leaves the window the only way back, so closing it exits.",
    keywords: [
      "tray icon",
      "hide icon",
      "notification area",
      "system tray",
    ],
  },
  {
    id: "setting-tray-dark-mode",
    page: "general",
    section: "The tray",
    title: "Dark mode support",
    description: "The icon takes the inks of the appearance the window is in.",
    keywords: ["tray", "dark mode", "light icon", "appearance", "icon colour"],
  },
  {
    id: "setting-url-list",
    page: "general",
    section: "The tray",
    title: "Customize URL List",
    description: "The protocols a copied address has to start with.",
    keywords: ["url", "links", "protocols", "clipboard", "schemes", "http"],
  },
  {
    id: "setting-theme",
    page: "appearance",
    section: "Theme",
    title: "Theme",
    description: "Auto follows your system.",
    keywords: [
      "theme",
      "appearance",
      "skin",
      "colours",
      "colors",
      "dark",
      "light",
      "cyberpunk",
      "volt",
      "soft black",
      "auto",
    ],
  },
  {
    id: "setting-font",
    page: "appearance",
    section: "Typography",
    title: "Font",
    description: "The face the whole window is drawn in.",
    keywords: [
      "font",
      "typeface",
      "text",
      "inter",
      "google sans",
      "geist",
      "interface font",
    ],
  },
  {
    id: "setting-weight",
    page: "appearance",
    section: "Typography",
    title: "Weight",
    description: "How heavy the text is drawn, in every window at once.",
    keywords: ["weight", "bold", "semibold", "medium", "regular", "text"],
  },
  {
    id: "setting-toolbar-style",
    page: "toolbar",
    section: "Style",
    title: "Toolbar style",
    description: "The picture pack the bar under the menu is drawn with.",
    keywords: [
      "toolbar",
      "style",
      "icons",
      "pack",
      "3d",
      "classic",
      "isometric",
      "theme",
    ],
  },
  {
    id: "setting-toolbar-size",
    page: "toolbar",
    section: "Size and labels",
    title: "Large buttons",
    description: "Small buttons leave more room for the download list.",
    keywords: ["toolbar", "size", "large", "small", "compact", "buttons"],
  },
  {
    id: "setting-toolbar-labels",
    page: "toolbar",
    section: "Size and labels",
    title: "Show text labels",
    description: "The tooltip names the button either way.",
    keywords: ["toolbar", "labels", "text", "captions", "icons"],
  },
  {
    id: "setting-toolbar-buttons",
    page: "toolbar",
    section: "Buttons",
    title: "Customize buttons",
    description: "Add, remove, reorder or group the buttons on the bar.",
    keywords: [
      "toolbar",
      "customize",
      "buttons",
      "add",
      "remove",
      "reorder",
      "separator",
    ],
  },
  {
    id: "setting-toolbar-manage",
    page: "toolbar",
    section: "Buttons",
    title: "More toolbars",
    description: "Install a pack from a file, replace one or remove it.",
    keywords: ["toolbar", "packs", "install", "import", "replace", "remove"],
  },
  {
    id: "setting-tips-startup",
    page: "tips",
    section: "Startup",
    title: "Show a tip on startup",
    description: "One tip per start. Help > Tip of the Day opens them any time.",
    keywords: ["tips", "startup", "hints", "tip of the day", "help"],
  },
  {
    id: "setting-tip-order",
    page: "tips",
    section: "Startup",
    title: "Tip order",
    description: "The next tip continues after the last one you saw.",
    keywords: ["tips", "order", "reset", "first tip", "cursor"],
  },
  {
    id: "setting-default-connections",
    page: "downloads",
    section: "Connections",
    title: "Default number of connections",
    description: "For new downloads. Up to this many parts.",
    keywords: [
      "connections",
      "parallel",
      "parts",
      "segments",
      "threads",
      "default",
      "speed",
    ],
  },
  {
    id: "setting-duplicate-policy",
    page: "downloads",
    section: "Existing files",
    title: "When the file already exists",
    description: "What happens when the file is already there.",
    keywords: [
      "duplicate",
      "existing file",
      "overwrite",
      "rename",
      "resume",
      "ask",
      "same name",
    ],
  },
  {
    id: "setting-progress-window",
    page: "downloads",
    section: "While a download runs",
    title: "Open the progress window when a download starts",
    description: "Opens for every new download.",
    keywords: ["progress", "window", "popup", "start", "download"],
  },
  {
    id: "setting-complete-dialog",
    page: "downloads",
    section: "When a download finishes",
    title: "Show the download complete dialog",
    description: "Names the file and where it went.",
    keywords: [
      "complete",
      "finished",
      "dialog",
      "notification",
      "popup",
      "done",
      "finished download",
    ],
  },
  {
    id: "setting-default-folder",
    page: "save-locations",
    section: "Default folder",
    title: "Save new downloads to",
    description: "Blank uses the Downloads folder of this computer.",
    keywords: [
      "folder",
      "save",
      "destination",
      "directory",
      "path",
      "storage",
      "location",
      "downloads folder",
    ],
  },
  {
    id: "setting-unfinished-folder",
    page: "save-locations",
    section: "Unfinished downloads",
    title: "Keep unfinished downloads in",
    description:
      "Incomplete files and their resume state wait here until a download completes.",
    keywords: [
      "unfinished",
      "temporary",
      "temp",
      "partial",
      "incomplete",
      "resume",
      "folder",
      "storage",
    ],
  },
  {
    id: "setting-remember-folder",
    page: "save-locations",
    section: "Remembering",
    title: "Start a new download in the last folder used",
    description: "The last folder you really downloaded to.",
    keywords: ["remember", "last folder", "recent", "folder", "default"],
  },
  {
    id: "setting-forget-folder",
    page: "save-locations",
    section: "Remembering",
    title: "Forget the remembered folder",
    description: "Starts using the default again.",
    keywords: ["forget", "clear", "remembered folder", "reset", "folder"],
  },
  {
    id: "setting-browser-ready",
    page: "browser",
    section: "Status",
    title: "Ready to receive from the browser",
    description: "The local bridge the extension hands links to.",
    keywords: [
      "browser",
      "status",
      "bridge",
      "connection",
      "port",
      "extension",
      "ready",
    ],
  },
  {
    id: "setting-browser-last-link",
    page: "browser",
    section: "Status",
    title: "Last link received",
    description: "Any request from the extension counts.",
    keywords: ["browser", "last link", "received", "history", "extension"],
  },
  {
    id: "setting-browser-capture",
    page: "browser",
    section: "From the browser",
    title: "Take over downloads from the browser",
    description: "These rules win while the app is running.",
    keywords: [
      "browser",
      "capture",
      "take over",
      "extension",
      "downloads",
      "hand-over",
    ],
  },
  {
    id: "setting-browser-ask",
    page: "browser",
    section: "From the browser",
    title: "Ask before saving",
    description: "The link waits for you to press Download.",
    keywords: ["browser", "ask", "confirm", "save", "add dialog", "capture"],
  },
  {
    id: "setting-capture-types",
    page: "browser",
    section: "File types",
    title: "Take over only these types",
    description: "Blank takes every type.",
    keywords: [
      "file types",
      "extensions",
      "mp4",
      "video",
      "audio",
      "capture",
      "browser",
    ],
  },
  {
    id: "setting-skip-sites",
    page: "browser",
    section: "Sites",
    title: "Leave these sites to the browser",
    description: "Covers the site and anything under it.",
    keywords: [
      "sites",
      "skip",
      "exclude",
      "ignore",
      "exception",
      "domains",
      "browser",
    ],
  },
  {
    id: "setting-video-button-style",
    page: "video-download-button",
    section: "Appearance",
    title: "Button style",
    description: "Icon only, text only, or both.",
    keywords: ["video", "button", "style", "icon", "text", "floating"],
  },
  {
    id: "setting-video-button-label",
    page: "video-download-button",
    section: "Appearance",
    title: "Button label",
    description: "Shown when the style includes text. Defaults to Download.",
    keywords: ["video", "button", "label", "text", "caption"],
  },
  {
    id: "setting-video-button-hover",
    page: "video-download-button",
    section: "Behaviour",
    title: "Hover to show",
    description: "Only show the button while the pointer is on the video.",
    keywords: ["video", "button", "hover", "pointer", "mouse", "show"],
  },
  {
    id: "setting-max-connections",
    page: "connections",
    section: "Connections",
    title: "Maximum number of connections",
    description: "Across all downloads at once.",
    keywords: [
      "connections",
      "maximum",
      "limit",
      "parallel",
      "simultaneous",
      "threads",
    ],
  },
  {
    id: "setting-speed-limit-toggle",
    page: "connections",
    section: "Speed limit",
    title: "Limit the speed of all downloads",
    description: "Shared by all downloads.",
    keywords: [
      "speed",
      "bandwidth",
      "limit",
      "throttle",
      "slow",
      "cap",
      "network",
    ],
  },
  {
    id: "setting-speed-limit",
    page: "connections",
    section: "Speed limit",
    title: "Speed limit",
    description: "Kilobytes per second, all downloads.",
    keywords: ["speed", "kb/s", "bandwidth", "limit", "throttle", "cap"],
  },
  {
    id: "setting-proxy",
    page: "proxy",
    section: "Proxy address",
    title: "Proxy address",
    description: "Blank means direct.",
    keywords: [
      "proxy",
      "socks5",
      "http",
      "network",
      "direct",
      "address",
      "server",
    ],
  },
  {
    id: "setting-data-folder",
    page: "advanced",
    section: "Application data",
    title: "Data folder",
    description: "The download list and unfinished files.",
    keywords: ["data", "folder", "storage", "appdata", "files", "open"],
  },
  {
    id: "setting-log",
    page: "advanced",
    section: "Application data",
    title: "Log",
    description: "What happened, including failures.",
    keywords: ["log", "logs", "errors", "troubleshooting", "open", "file"],
  },
  {
    id: "setting-reset",
    page: "advanced",
    section: "Reset",
    title: "Reset settings",
    description: "Restores every setting, including the last folder used.",
    keywords: [
      "reset",
      "restore",
      "defaults",
      "clear",
      "erase",
      "factory settings",
    ],
  },
  {
    id: "setting-version",
    page: "about",
    section: "Application",
    title: "Version",
    description: "The build that is running.",
    keywords: ["version", "build", "release", "about"],
  },
  {
    id: "setting-last-check",
    page: "about",
    section: "Updates",
    title: "Last check",
    description: "When the application last asked for a newer version.",
    keywords: ["update", "last check", "version", "release"],
  },
  {
    id: "setting-auto-update",
    page: "about",
    section: "Updates",
    title: "Check for updates when the application starts",
    description: "Asks the release server once at startup.",
    keywords: [
      "update",
      "updates",
      "automatic",
      "startup",
      "new version",
      "release",
    ],
  },
];

/**
 * Every shortcut row joins the same catalogue. The page and the search read
 * the one registry, so a key can never exist on one and not the other.
 */
const SHORTCUT_ENTRIES: readonly SettingsCatalogEntry[] = shortcutSearchRows().map(
  (row) => ({
    id: row.id,
    page: "shortcuts" as const,
    section: "Keyboard shortcuts",
    title: row.label,
    description: row.keys
      ? `${row.hint} The key is ${row.keys}.`
      : row.hint,
    keywords: [
      "shortcut",
      "shortcuts",
      "keyboard",
      "hotkey",
      "key",
      "bind",
      ...(row.keys ? [row.keys] : []),
    ],
  }),
);

export const SETTINGS_SEARCH_ENTRIES: readonly SettingsCatalogEntry[] = [
  ...PAGE_ENTRIES,
  ...SHORTCUT_ENTRIES,
];

export const SETTINGS_PAGE_LABELS: ReadonlyMap<string, string> = new Map(
  SETTINGS_PAGES.map((page) => [page.id, page.label]),
);

const SETTINGS_PAGE_ORDER: readonly string[] = SETTINGS_PAGES.map(
  (page) => page.id,
);

/**
 * Prepared once at module load. Searching is a single pass over this array;
 * no component rebuilds it, and nothing is scanned from the DOM.
 */
export const SETTINGS_SEARCH_INDEX = buildSettingsSearchIndex(
  SETTINGS_SEARCH_ENTRIES,
  SETTINGS_PAGE_LABELS,
);

export function searchSettingsCatalog(
  query: string,
): SettingsSearchHit<SettingsCatalogEntry>[] {
  return searchSettings(query, SETTINGS_SEARCH_INDEX);
}

export function groupSettingsCatalogHits(
  hits: readonly SettingsSearchHit<SettingsCatalogEntry>[],
): SettingsSearchGroup<SettingsCatalogEntry>[] {
  return groupSettingsHits(hits, SETTINGS_PAGE_LABELS, SETTINGS_PAGE_ORDER);
}

export type { SettingsSearchGroup, SettingsSearchHit };
