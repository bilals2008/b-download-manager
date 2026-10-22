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
  defaultFolder: "",
  defaultConnections: 8,
  maxConnections: 32,
  speedLimitEnabled: false,
  speedLimitBytes: 0,
  proxyUrl: "",
};

const STORAGE_KEY = "idm-clone.settings.v1";

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

/** The size of the toolbar buttons, the way IDM keeps it in the View menu. */
export type ToolbarSize = "large" | "small";

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

function number(value: unknown, fallback: number, min: number, max: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return fallback;
  }
  return Math.min(max, Math.max(min, Math.round(value)));
}
