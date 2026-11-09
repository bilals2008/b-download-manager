import type { ThemePreference } from "@/lib/settings";

/**
 * Appearance. The tokens live in `src/index.css` and `src/styles/themes/`; this
 * file only decides which block is active by setting `data-theme` on the root element. It is applied
 * before the first render (see `main.tsx`), so a light window never flashes
 * dark on the way in.
 *
 * Auto resolves to one of the two system appearances; a fixed theme is itself.
 */
export type ResolvedTheme = Exclude<ThemePreference, "auto">;

export function resolveTheme(preference: ThemePreference): ResolvedTheme {
  if (preference !== "auto") {
    return preference;
  }
  return window.matchMedia("(prefers-color-scheme: light)").matches
    ? "light"
    : "dark";
}

/**
 * Whether an appearance is drawn on light surfaces.
 *
 * The tray icon takes its inks from the appearance, and it has no tokens to
 * read, so the light appearances are named here once. A new theme belongs in
 * this list when its canvas is pale.
 */
export function isLightTheme(theme: ResolvedTheme): boolean {
  return theme === "light" || theme === "bloom";
}

export function applyTheme(preference: ThemePreference): void {
  document.documentElement.dataset.theme = resolveTheme(preference);
}

/** The appearance in force, in the tokens the extension's panel paints with. */
export interface BrowserThemeSnapshot {
  theme: ResolvedTheme;
  light: boolean;
  palette: Record<string, string>;
}

/**
 * Read the tokens of the theme that is on screen.
 *
 * The extension's shadow root does not see this window's styles, so the
 * computed values are handed to it through the bridge. A token the theme does
 * not set is left out, and the extension keeps its own value for it.
 */
export function currentBrowserTheme(): BrowserThemeSnapshot | null {
  if (typeof document === "undefined" || !document.documentElement) {
    return null;
  }
  const theme = document.documentElement.dataset.theme as
    | ResolvedTheme
    | undefined;
  if (!theme) {
    return null;
  }
  const style = getComputedStyle(document.documentElement);
  const value = (name: string) => style.getPropertyValue(name).trim();
  const entries: Record<string, string> = {
    canvas: value("--chrome-popup") || value("--chrome-canvas"),
    surface: value("--chrome-panel"),
    text: value("--chrome-menu-text") || value("--foreground"),
    muted: value("--chrome-muted"),
    line: value("--chrome-line") || value("--border"),
    hover: value("--chrome-menu-hover"),
    active: value("--chrome-menu-active"),
    primary: value("--primary"),
    "on-primary": value("--primary-foreground"),
    progress: value("--progress") || value("--chrome-focus"),
    success: value("--success") || value("--icon-green"),
    danger: value("--chrome-error"),
    focus: value("--chrome-focus"),
    radius: value("--radius"),
    shadow: value("--shadow"),
  };
  const palette = Object.fromEntries(
    Object.entries(entries).filter(([, token]) => token.length > 0),
  );
  return { theme, light: isLightTheme(theme), palette };
}
