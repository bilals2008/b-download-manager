import { onSettingsChanged } from "@/lib/app-events";
import { applyFont, applyWeight } from "@/lib/fonts";
import { loadSettings } from "@/lib/settings";
import { applyTheme, watchTheme } from "@/lib/theme";
import { syncWindowBackground } from "@/lib/window";

/**
 * Read the saved appearance and put it on the root element.
 *
 * The tokens in `src/index.css` do the drawing; this only chooses the block
 * of them that is in force, so every window of the application can call it.
 */
function applySavedAppearance(): void {
  const settings = loadSettings();
  applyTheme(settings.theme);
  applyFont(settings.uiFont);
  applyWeight(settings.uiWeight);
  // The native fill follows the same token, so a repaint before the page is
  // drawn never shows the webview's default white.
  void syncWindowBackground();
}

let started = false;

/**
 * Apply the saved appearance and keep this window in step with the others.
 *
 * The main window, the progress windows and the completion window are separate
 * documents. Each one applies the saved appearance before its first paint and
 * then follows two sources of change: the operating system when the theme is
 * Auto, and the settings window's broadcast when the user picks a theme, a
 * face or a weight. Without this the small windows would keep the appearance
 * they were opened with until they were closed.
 */
export function startAppearanceSync(): void {
  // A dev reload calls this again; the listeners are already in place.
  if (started) {
    applySavedAppearance();
    return;
  }
  started = true;

  applySavedAppearance();

  // Auto follows the operating system for as long as the window is open.
  watchTheme();

  // The settings window says when a saved preference changed.
  onSettingsChanged(applySavedAppearance);

  // A `storage` event reaches the other windows of the same origin in a plain
  // browser and in the desktop runtime alike, so it is a second signal in case
  // the event above is not delivered.
  window.addEventListener("storage", applySavedAppearance);
}
