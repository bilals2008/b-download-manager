/**
 * Preferences the settings screen owns.
 *
 * shortcut: these two preferences are stored but not enforced by the operating
 * system yet. Wire them to autostart and clipboard monitoring when those
 * subsystems land.
 */
export interface Settings {
  launchOnStartup: boolean;
  clipboardCapture: boolean;
}

export const DEFAULT_SETTINGS: Settings = {
  launchOnStartup: false,
  clipboardCapture: false,
};

const STORAGE_KEY = "idm-clone.settings.v1";

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
      launchOnStartup:
        typeof value.launchOnStartup === "boolean"
          ? value.launchOnStartup
          : DEFAULT_SETTINGS.launchOnStartup,
      clipboardCapture:
        typeof value.clipboardCapture === "boolean"
          ? value.clipboardCapture
          : DEFAULT_SETTINGS.clipboardCapture,
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
