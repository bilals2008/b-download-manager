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
