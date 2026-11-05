/**
 * The preferences the options window owns.
 *
 * Every value here is stored on this machine and is read back when the
 * application starts.
 */
export interface Settings {
  /** Register the application in the operating system's startup entry. */
  launchOnStartup: boolean;
  /** Watch the clipboard for links and offer them to the add dialog. */
  clipboardCapture: boolean;
  /** Whether the minimize button hides the window into the tray. */
  minimizeToTray: boolean;
  /** Whether closing the window keeps the application in the tray. */
  closeToTray: boolean;
  /** Whether a finished download gets its own window. */
  showCompleteDialog: boolean;
  /** Whether a new download opens its progress window. */
  showProgressOnStart: boolean;
  /** Whether one tip is offered when the application is revealed at startup. */
  tipsOnStartup: boolean;
  /** Auto follows the system; the other two override it. */
  theme: ThemePreference;
  /** The face the whole window is drawn in, built in or imported. */
  uiFont: UiFont;
  /** How heavy the text is drawn. */
  uiWeight: UiWeight;
  /** Where a new download is saved when the user does not say. */
  defaultFolder: string;
  /** Open on the folder of the last download instead of the default. */
  rememberLastFolder: boolean;
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
  /** What happens when a file of the same name is already there. */
  duplicatePolicy: DuplicatePolicy;
  /** Whether the browser extension hands downloads to the application. */
  browserCapture: boolean;
  /** Whether a hand-over opens the add dialog instead of starting. */
  captureAsk: boolean;
  /** File types the extension takes over. Empty means every type. */
  captureTypes: string[];
  /** Hosts the extension leaves to the browser. */
  skipSites: string[];
  /** Icon only, text only, or icon + text on the floating video button. */
  videoButtonStyle: "icon" | "text" | "both";
  /** The label on the floating video button, when it shows text. */
  videoButtonLabel: string;
  /** Only show the floating video button while the pointer is on the video. */
  videoButtonHover: boolean;
  /**
   * Whether the application asks the release endpoint for a newer version
   * when it starts. Off by default: a check reaches the network, so it is the
   * reader's call, not something done on their first open.
   */
  updateAutoCheck: boolean;
  /**
   * When a version check last finished, as the clock reads it. Null means no
   * check has run on this machine since the field existed.
   */
  lastUpdateCheck: number | null;
  /**
   * The version whose startup notice the reader put off. The same release is
   * not offered again; a newer one is. Null means nothing was dismissed.
   */
  dismissedUpdateVersion: string | null;
  /**
   * Whether the add dialog has been opened at least once. The first of
   * those openings says where files will land when no save folder has been
   * chosen yet; after that the sentence has done its work.
   */
  firstRunDone: boolean;
}

export type DuplicatePolicy = "ask" | "rename" | "replace" | "skip";

export const DUPLICATE_POLICIES: { value: DuplicatePolicy; label: string }[] = [
  { value: "ask", label: "Ask every time" },
  { value: "rename", label: "Save under a free name" },
  { value: "replace", label: "Replace the file" },
  { value: "skip", label: "Skip the download" },
];

export const DEFAULT_SETTINGS: Settings = {
  launchOnStartup: false,
  clipboardCapture: false,
  minimizeToTray: false,
  closeToTray: true,
  showCompleteDialog: true,
  showProgressOnStart: false,
  tipsOnStartup: true,
  theme: "ayu",
  uiFont: "googlesans",
  uiWeight: "medium",
  defaultFolder: "",
  rememberLastFolder: true,
  defaultConnections: 8,
  maxConnections: 32,
  speedLimitEnabled: false,
  speedLimitBytes: 0,
  proxyUrl: "",
  duplicatePolicy: "ask",
  browserCapture: true,
  captureAsk: false,
  captureTypes: [],
  skipSites: [],
  videoButtonStyle: "icon",
  videoButtonLabel: "Download",
  videoButtonHover: false,
  updateAutoCheck: false,
  lastUpdateCheck: null,
  dismissedUpdateVersion: null,
  firstRunDone: false,
};

/**
 * Ownership of the saved values: the settings the engine applies live in its
 * own file (`settings.json` beside the application data); this storage keeps
 * the cache the settings window opens on, and the values only the windows
 * read — appearance, toolbars, tips.
 */
const STORAGE_KEY = "idm-clone.settings.v1";

/**
 * The folder the add dialog used last. It is not part of `Settings`: the
 * options window does not own it, and the next successful add replaces it.
 */
const LAST_FOLDER_KEY = "idm-clone.last-folder.v1";

/** The range the engine accepts for a connection count. */
export const MAX_CONNECTIONS = 32;

/** The longest list of capture rules one page keeps. */
export const MAX_RULE_ENTRIES = 64;

const TYPE_PATTERN = /^[a-z0-9][a-z0-9+_-]{0,15}$/;

/** The same host rule the extension uses, so a rule means the same thing. */
const HOST_PATTERN =
  /^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$/;

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
    const migrated =
      value.closeToTray === undefined && typeof value.minimizeToTray === "boolean";
    return {
      launchOnStartup: boolean(value.launchOnStartup, DEFAULT_SETTINGS.launchOnStartup),
      clipboardCapture: boolean(value.clipboardCapture, DEFAULT_SETTINGS.clipboardCapture),
      // An older build had one switch, `minimizeToTray`, meaning what
      // `closeToTray` means now. Its value moves there once, and the new
      // minimize choice starts off unless the newer key is present.
      minimizeToTray: migrated
        ? false
        : boolean(value.minimizeToTray, DEFAULT_SETTINGS.minimizeToTray),
      closeToTray: boolean(
        value.closeToTray,
        migrated
          ? boolean(value.minimizeToTray, DEFAULT_SETTINGS.closeToTray)
          : DEFAULT_SETTINGS.closeToTray,
      ),
      showCompleteDialog: boolean(
        value.showCompleteDialog,
        DEFAULT_SETTINGS.showCompleteDialog,
      ),
      showProgressOnStart: boolean(
        value.showProgressOnStart,
        DEFAULT_SETTINGS.showProgressOnStart,
      ),
      tipsOnStartup: boolean(
        value.tipsOnStartup,
        DEFAULT_SETTINGS.tipsOnStartup,
      ),
      theme: theme(value.theme, DEFAULT_SETTINGS.theme),
      uiFont: font(value.uiFont, DEFAULT_SETTINGS.uiFont),
      uiWeight: weight(value.uiWeight, DEFAULT_SETTINGS.uiWeight),
      defaultFolder: text(value.defaultFolder, DEFAULT_SETTINGS.defaultFolder),
      rememberLastFolder: boolean(
        value.rememberLastFolder,
        DEFAULT_SETTINGS.rememberLastFolder,
      ),
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
      duplicatePolicy: duplicate(
        value.duplicatePolicy,
        DEFAULT_SETTINGS.duplicatePolicy,
      ),
      browserCapture: boolean(
        value.browserCapture,
        DEFAULT_SETTINGS.browserCapture,
      ),
      captureAsk: boolean(value.captureAsk, DEFAULT_SETTINGS.captureAsk),
      captureTypes: entries(
        value.captureTypes,
        DEFAULT_SETTINGS.captureTypes,
        cleanCaptureType,
      ),
      skipSites: entries(value.skipSites, DEFAULT_SETTINGS.skipSites, cleanSite),
      videoButtonStyle: videoButtonStyle(value.videoButtonStyle),
      videoButtonLabel: text(value.videoButtonLabel, DEFAULT_SETTINGS.videoButtonLabel),
      videoButtonHover: boolean(value.videoButtonHover, DEFAULT_SETTINGS.videoButtonHover),
      updateAutoCheck: boolean(
        value.updateAutoCheck,
        DEFAULT_SETTINGS.updateAutoCheck,
      ),
      lastUpdateCheck: stamp(
        value.lastUpdateCheck,
        DEFAULT_SETTINGS.lastUpdateCheck,
      ),
      dismissedUpdateVersion: version(
        value.dismissedUpdateVersion,
        DEFAULT_SETTINGS.dismissedUpdateVersion,
      ),
      firstRunDone: boolean(value.firstRunDone, DEFAULT_SETTINGS.firstRunDone),
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

/**
 * Read the engine's settings file into the webview's copy, so the settings
 * window opens on what the engine really applied.
 *
 * The file `settings.json` beside the application data is what the engine
 * reads at every start, so it owns the values it applies. The webview's
 * saved copy is a cache of them, written back through the storage commands
 * when the reader edits.
 *
 * Null means there is no file yet — a first run or an upgrade — and the
 * webview's own copy stays the seed; the first write from it creates the
 * file. A failure means the copy stays as it is, the way the window already
 * reads it.
 */
export async function syncEngineSettings(): Promise<Settings | null> {
  const { savedSettings } = await import("./ipc.ts");
  const saved = await savedSettings();
  if (!saved) {
    return null;
  }
  const settings = loadSettings();
  settings.defaultFolder = text(saved.defaultFolder, settings.defaultFolder);
  // The folder for the bytes of an unfinished download is not a setting:
  // the application owns it, and the file of the engine no longer names it.
  settings.defaultConnections = number(
    saved.defaultConnections,
    settings.defaultConnections,
    1,
    MAX_CONNECTIONS,
  );
  settings.maxConnections = number(
    saved.maxConnections,
    settings.maxConnections,
    1,
    MAX_CONNECTIONS,
  );
  settings.speedLimitEnabled = boolean(
    saved.speedLimitEnabled,
    settings.speedLimitEnabled,
  );
  settings.speedLimitBytes = number(
    saved.speedLimitBytes,
    settings.speedLimitBytes,
    0,
    1024 * 1024 * 1024,
  );
  settings.proxyUrl = text(saved.proxyUrl, settings.proxyUrl);
  settings.browserCapture = boolean(
    saved.browserPolicy.capture,
    settings.browserCapture,
  );
  settings.captureAsk = boolean(saved.browserPolicy.ask, settings.captureAsk);
  settings.captureTypes = entries(
    saved.browserPolicy.types,
    settings.captureTypes,
    cleanCaptureType,
  );
  settings.skipSites = entries(
    saved.browserPolicy.skipSites,
    settings.skipSites,
    cleanSite,
  );
  settings.videoButtonStyle = videoButtonStyle(saved.videoButton.style);
  settings.videoButtonLabel = text(
    saved.videoButton.label,
    settings.videoButtonLabel,
  );
  settings.videoButtonHover = boolean(
    saved.videoButton.hover,
    settings.videoButtonHover,
  );
  settings.showCompleteDialog = boolean(
    saved.showCompleteDialog,
    settings.showCompleteDialog,
  );
  settings.showProgressOnStart = boolean(
    saved.showProgressOnStart,
    settings.showProgressOnStart,
  );
  settings.clipboardCapture = boolean(
    saved.clipboardCapture,
    settings.clipboardCapture,
  );
  saveSettings(settings);
  return settings;
}

/** ".MP4", "*.mp4" and " mp4 " are all the same extension. */
export function cleanCaptureType(value: string): string {
  const cleaned = value.trim().toLowerCase().replace(/^\*?\./, "");
  return TYPE_PATTERN.test(cleaned) ? cleaned : "";
}

/** A host name out of anything the user may have typed. Junk becomes "". */
export function cleanSite(value: string): string {
  const text = value.trim().replace(/^\*?\./, "");
  if (!text) {
    return "";
  }
  try {
    const url = new URL(
      text.includes("://") ? text : `http://${text}`,
    );
    const host = url.hostname.toLowerCase().replace(/\.$/, "");
    if (host === "localhost") {
      return host;
    }
    return HOST_PATTERN.test(host) ? host : "";
  } catch {
    return "";
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

/** Forget the folder of the last download, for a full reset. */
export function clearLastFolder(): void {
  try {
    window.localStorage.removeItem(LAST_FOLDER_KEY);
  } catch {
    // Storage can be blocked; the remembered folder does no harm.
  }
}

/**
 * The folder the add dialog opens on: the configured default, or the folder of
 * the last download when the reader asked to remember it. Empty means the
 * system Downloads folder, which the dialog asks the engine for.
 */
export function preferredFolder(): string {
  const settings = loadSettings();
  if (settings.rememberLastFolder) {
    const last = loadLastFolder();
    if (last) {
      return last;
    }
  }
  return settings.defaultFolder.trim();
}

/** The size of the toolbar buttons, a View menu setting. */
export type ToolbarSize = "large" | "small";

/** How the appearance is chosen. Auto follows the system; the rest are fixed. */
export type ThemePreference =
  | "auto"
  | "light"
  | "dark"
  | "cyberpunk"
  | "volt"
  | "softblack"
  | "ayu"
  | "graphite"
  | "bloom"
  | "hive";

/** The saved values, in one place so the loader and the picker agree. */
const THEME_PREFERENCES: readonly ThemePreference[] = [
  "auto",
  "light",
  "dark",
  "cyberpunk",
  "volt",
  "softblack",
  "ayu",
  "graphite",
  "bloom",
  "hive",
];

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
  return THEME_PREFERENCES.includes(value as ThemePreference)
    ? (value as ThemePreference)
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

/**
 * A moment the clock can read back. Anything else — a junk number, a date from
 * before the epoch — means no check has run, which is what null says.
 */
function stamp(value: unknown, fallback: number | null): number | null {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    return fallback;
  }
  return Math.round(value);
}

/**
 * A release number this machine put off. A version is a short token made of
 * digits, dots and the letters a prerelease can carry; anything else is not
 * one and reads as nothing dismissed.
 */
function version(value: unknown, fallback: string | null): string | null {
  if (typeof value !== "string") {
    return fallback;
  }
  const trimmed = value.trim();
  return /^[0-9A-Za-z.+-]{1,64}$/.test(trimmed) ? trimmed : fallback;
}

function duplicate(
  value: unknown,
  fallback: DuplicatePolicy,
): DuplicatePolicy {
  return value === "ask" ||
    value === "rename" ||
    value === "replace" ||
    value === "skip"
    ? value
    : fallback;
}

function videoButtonStyle(value: unknown): "icon" | "text" | "both" {
  return value === "icon" || value === "text" || value === "both"
    ? value
    : DEFAULT_SETTINGS.videoButtonStyle;
}

function entries(
  value: unknown,
  fallback: readonly string[],
  clean: (value: string) => string,
): string[] {
  if (!Array.isArray(value)) {
    return [...fallback];
  }
  const out: string[] = [];
  for (const entry of value) {
    if (typeof entry !== "string") {
      continue;
    }
    const cleaned = clean(entry);
    if (cleaned && !out.includes(cleaned)) {
      out.push(cleaned);
    }
    if (out.length >= MAX_RULE_ENTRIES) {
      break;
    }
  }
  return out;
}
