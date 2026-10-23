type WindowAction = "minimize" | "toggleMaximize" | "hide" | "close";

function runningInTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Window chrome is drawn by the app, so the title-bar buttons drive the Tauri
 * window directly. In a plain browser preview there is no native window to act
 * on, so the call is a no-op.
 */
export async function windowAction(action: WindowAction): Promise<void> {
  if (!runningInTauri()) {
    return;
  }
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const current = getCurrentWindow();
  await current[action]();
}

export function toggleMaximize(): Promise<void> {
  return windowAction("toggleMaximize");
}

/**
 * Hide the window without closing it.
 *
 * The progress window uses this for its tray button: the download keeps
 * running, the window keeps following it, and the tray menu can bring it back.
 */
export function hideWindow(): Promise<void> {
  return windowAction("hide");
}

/** Close the window this page is in. The main window uses the menu instead. */
export function closeWindow(): Promise<void> {
  return windowAction("close");
}

/**
 * Show a window the backend built hidden, once its page has painted.
 *
 * The backend keeps the two small windows hidden until this call, because a
 * webview that is visible while it loads shows an empty white frame first.
 * Two animation frames: one for React to commit, one for the paint itself.
 */
export async function showWindowWhenPainted(): Promise<void> {
  if (!runningInTauri()) {
    return;
  }
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => resolve());
  });
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => resolve());
  });
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke("show_ready");
  } catch {
    // The window stays hidden rather than opening on a white frame. The
    // backend still has its row, so the next open can try again.
  }
}

/**
 * Give the native window the same fill as the page.
 *
 * Whatever the webview has not painted yet shows the window's own background,
 * which is white by default. Keeping it on the theme's `--background` makes
 * the gap during a load, a restore from the tray, or a theme switch invisible
 * instead of a white flash.
 */
export async function syncWindowBackground(): Promise<void> {
  if (!runningInTauri()) {
    return;
  }
  const color = getComputedStyle(document.documentElement)
    .getPropertyValue("--background")
    .trim();
  if (!color) {
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke("set_window_background", { color });
  } catch {
    // Cosmetic only: the page still draws the theme it has.
  }
}
