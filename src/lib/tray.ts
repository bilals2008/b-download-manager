/**
 * The tray icon choices the View menu owns.
 *
 * They are saved by the backend, beside the close and minimize choices, in
 * `desktop.json`: the icon has to be right before a window has read anything.
 * This file only holds the model — the rules the URL list follows.
 * `src-tauri/src/prefs.rs` applies the same rules to the list before it is
 * saved, so what the dialog shows is what the clipboard watch matches.
 */

/**
 * The protocols a copied address has to start with to be offered as a
 * download, before the reader changes them.
 */
export const DEFAULT_URL_PROTOCOLS: readonly string[] = ["http", "https", "ftp"];

/** The longest list the backend keeps. */
export const MAX_URL_PROTOCOLS = 32;

/** A protocol as it is stored: a bare scheme, or `""` when it is not one. */
export function cleanUrlProtocol(value: string): string {
  const entry = value
    .trim()
    .replace(/:\/\/$/i, "")
    .replace(/:$/i, "")
    .toLowerCase();
  if (!/^[a-z][a-z0-9+-.]{0,15}$/.test(entry)) {
    return "";
  }
  return entry;
}

/**
 * The list as the backend stores it: cleaned, without repeats, and never
 * empty. An empty list would offer nothing, so it falls back to the three the
 * application starts with.
 */
export function normalizeUrlProtocols(
  values: readonly string[],
): string[] {
  const out: string[] = [];
  for (const value of values) {
    const entry = cleanUrlProtocol(value);
    if (entry && !out.includes(entry)) {
      out.push(entry);
    }
    if (out.length >= MAX_URL_PROTOCOLS) {
      break;
    }
  }
  return out.length ? out : [...DEFAULT_URL_PROTOCOLS];
}

/** The tray choices, as the backend saved them. */
export interface TrayPreferences {
  /** "Don't show" in the menu. */
  hidden: boolean;
  /** "Dark Mode support" in the menu. */
  darkMode: boolean;
  /** "Customize URL List..." edits this. */
  urlProtocols: string[];
}

export const DEFAULT_TRAY_PREFERENCES: TrayPreferences = {
  hidden: false,
  darkMode: false,
  urlProtocols: [...DEFAULT_URL_PROTOCOLS],
};

/**
 * The tray choices out of what the backend saved.
 *
 * The shape is structural, so this file stays free of the IPC module that
 * reads the rest of `desktop.json`.
 */
export function trayPreferencesFrom(saved: {
  trayHidden: boolean;
  trayDarkMode: boolean;
  urlProtocols: string[];
}): TrayPreferences {
  return {
    hidden: saved.trayHidden,
    darkMode: saved.trayDarkMode,
    urlProtocols: normalizeUrlProtocols(saved.urlProtocols),
  };
}
