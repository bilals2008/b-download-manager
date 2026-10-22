type WindowAction = "minimize" | "toggleMaximize" | "close";

function runningInTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Window chrome is drawn by the app, so the three title-bar buttons drive the
 * Tauri window directly. In a plain browser preview there is no native window
 * to act on, so the call is a no-op.
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

/** Close the window this page is in. The main window uses the menu instead. */
export function closeWindow(): Promise<void> {
  return windowAction("close");
}
