import { loadSettings, type ThemePreference } from "@/lib/settings";

/**
 * Appearance. The tokens live in `src/index.css`; this file only decides which
 * block is active by setting `data-theme` on the root element. It is applied
 * before the first render (see `main.tsx`), so a light window never flashes
 * dark on the way in.
 */
export function resolveTheme(preference: ThemePreference): "light" | "dark" {
  if (preference !== "auto") {
    return preference;
  }
  return window.matchMedia("(prefers-color-scheme: light)").matches
    ? "light"
    : "dark";
}

export function applyTheme(preference: ThemePreference): void {
  document.documentElement.dataset.theme = resolveTheme(preference);
}

/** Re-resolve Auto when the operating system switches appearance. */
export function watchTheme(): () => void {
  const media = window.matchMedia("(prefers-color-scheme: light)");
  const sync = () => applyTheme(loadSettings().theme);
  media.addEventListener("change", sync);
  return () => media.removeEventListener("change", sync);
}
