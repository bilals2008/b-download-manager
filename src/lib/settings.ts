/**
 * The preferences the options window owns.
 *
 * Every value here is stored on this machine and is read back when the
 * application starts. The ones the engine owns — the proxy, the bandwidth
 * cap, the number of connections — are pushed to the engine as well, so a
 * change takes effect at once.
 *
 * shortcut: `launchOnStartup` and `clipboardCapture` are stored but not
 * enforced by the operating system yet. Wire them to autostart and clipboard
 * monitoring when those subsystems land.
 */
export interface Settings {
  launchOnStartup: boolean;
  clipboardCapture: boolean;
  /** Whether closing the window hides it into the tray. */
  minimizeToTray: boolean;
  /** Whether a finished download gets its own window. */
  showCompleteDialog: boolean;
  /** Auto follows the system; the other two override it. */
  theme: ThemePreference;
  /** The face the whole window is drawn in, built in or imported. */
  uiFont: UiFont;
  /** How heavy the text is drawn. */
  uiWeight: UiWeight;
  /** Where a new download is saved when the user does not say. */
  defaultFolder: string;
  /** Connections one download gets by default. */
  defaultConnections: number;
  /** Connections the whole application may have open at once. */
  maxConnections: number;
  /** Whether the bandwidth cap is on. */
  speedLimitEnabled: boolean;
  /** The bandwidth cap, in bytes per second. */
  speedLimitBytes: number;
  /** The proxy new downloads go through, empty for none. */
  proxyUrl: string;
}

export const DEFAULT_SETTINGS: Settings = {
  launchOnStartup: false,
  clipboardCapture: false,
  minimizeToTray: true,
  showCompleteDialog: true,
  theme: "auto",
  uiFont: "googlesans",
  uiWeight: "medium",
  defaultFolder: "",
  defaultConnections: 8,
  maxConnections: 32,
  speedLimitEnabled: false,
  speedLimitBytes: 0,
  proxyUrl: "",
};

const STORAGE_KEY = "idm-clone.settings.v1";

/**
 * The folder the add dialog used last. It is not part of `Settings`: the
 * options window does not own it, and the next successful add replaces it.
 */
const LAST_FOLDER_KEY = "idm-clone.last-folder.v1";

/** The range the engine accepts for a connection count. */
export const MAX_CONNECTIONS = 32;

/** Read the saved preferences, falling back field by field on bad data. */
export function loadSettings(): Settings {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return DEFAULT_SETTINGS;
    }
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") {
      return DEFAULT_SETTINGS;
    }
    const value = parsed as Partial<Record<keyof Settings, unknown>>;
    return {
      launchOnStartup: boolean(value.launchOnStartup, DEFAULT_SETTINGS.launchOnStartup),
      clipboardCapture: boolean(value.clipboardCapture, DEFAULT_SETTINGS.clipboardCapture),
      minimizeToTray: boolean(value.minimizeToTray, DEFAULT_SETTINGS.minimizeToTray),
      showCompleteDialog: boolean(
        value.showCompleteDialog,
        DEFAULT_SETTINGS.showCompleteDialog,
      ),
      theme: theme(value.theme, DEFAULT_SETTINGS.theme),
      uiFont: font(value.uiFont, DEFAULT_SETTINGS.uiFont),
      uiWeight: weight(value.uiWeight, DEFAULT_SETTINGS.uiWeight),
      defaultFolder: text(value.defaultFolder, DEFAULT_SETTINGS.defaultFolder),
      defaultConnections: number(
        value.defaultConnections,
        DEFAULT_SETTINGS.defaultConnections,
        1,
        MAX_CONNECTIONS,
      ),
      maxConnections: number(
        value.maxConnections,
        DEFAULT_SETTINGS.maxConnections,
        1,
        MAX_CONNECTIONS,
      ),
      speedLimitEnabled: boolean(
        value.speedLimitEnabled,
        DEFAULT_SETTINGS.speedLimitEnabled,
      ),
      speedLimitBytes: number(
        value.speedLimitBytes,
        DEFAULT_SETTINGS.speedLimitBytes,
        0,
        1024 * 1024 * 1024,
      ),
      proxyUrl: text(value.proxyUrl, DEFAULT_SETTINGS.proxyUrl),
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function saveSettings(settings: Settings): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Storage can be blocked; the dialog still closes with the chosen values.
  }
}

/** The folder of the last download started from the add dialog. */
export function loadLastFolder(): string {
  try {
    return window.localStorage.getItem(LAST_FOLDER_KEY)?.trim() ?? "";
  } catch {
    return "";
  }
}

/** Remember the folder of a download, once it has really started. */
export function saveLastFolder(folder: string): void {
  const value = folder.trim();
  if (!value) {
    return;
  }
  try {
    window.localStorage.setItem(LAST_FOLDER_KEY, value);
  } catch {
    // Storage can be blocked; the next open falls back to the default.
  }
}

/**
 * The folder the add dialog opens on: the one the last download used, and the
 * configured default until there is one. Empty means the system Downloads
 * folder, which the dialog asks the engine for.
 */
export function preferredFolder(): string {
  return loadLastFolder() || loadSettings().defaultFolder.trim();
}

/** The size of the toolbar buttons, the way IDM keeps it in the View menu. */
export type ToolbarSize = "large" | "small";

/** How the appearance is chosen. */
export type ThemePreference = "auto" | "light" | "dark";

/**
 * The faces the appearance page offers. System is the Windows face, so it
 * costs no download; the rest ship with the application.
 */
export type UiFont = string;

const UI_FONTS: readonly string[] = [
  "inter",
  "geist",
  "googlesans",
  "dmsans",
  "jakarta",
  "instrument",
  "bricolage",
  "plex",
  "jetbrains",
  "system",
];

/**
 * How heavy the window is drawn. The whole scale moves together, so a heading
 * stays one step above the body it sits in.
 */
export type UiWeight = "regular" | "medium" | "semibold";

const UI_WEIGHTS: readonly UiWeight[] = ["regular", "medium", "semibold"];

const TOOLBAR_KEY = "idm-clone.toolbar-size.v1";

/** Read the saved toolbar size, falling back to the large one. */
export function loadToolbarSize(): ToolbarSize {
  try {
    return window.localStorage.getItem(TOOLBAR_KEY) === "small"
      ? "small"
      : "large";
  } catch {
    return "large";
  }
}

export function saveToolbarSize(size: ToolbarSize): void {
  try {
    window.localStorage.setItem(TOOLBAR_KEY, size);
  } catch {
    // Storage can be blocked; the toolbar still changes for this session.
  }
}

function boolean(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function text(value: unknown, fallback: string): string {
  return typeof value === "string" ? value : fallback;
}

function theme(value: unknown, fallback: ThemePreference): ThemePreference {
  return value === "auto" || value === "light" || value === "dark"
    ? value
    : fallback;
}

function font(value: unknown, fallback: UiFont): UiFont {
  if (typeof value !== "string") {
    return fallback;
  }
  return UI_FONTS.includes(value) ? value : fallback;
}

function weight(value: unknown, fallback: UiWeight): UiWeight {
  return UI_WEIGHTS.includes(value as UiWeight)
    ? (value as UiWeight)
    : fallback;
}

function number(value: unknown, fallback: number, min: number, max: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return fallback;
  }
  return Math.min(max, Math.max(min, Math.round(value)));
}
