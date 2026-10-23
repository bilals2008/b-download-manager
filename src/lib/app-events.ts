import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";

import { isDesktop } from "@/lib/errors";

/**
 * The event a window sends after a saved preference changed.
 *
 * Preferences live in `localStorage`, which every window of the application
 * shares. A window cannot see another window write to it, though, so the
 * appearance and the application-wide limits would stay at the values they
 * were opened with. The settings window broadcasts this event instead, and
 * every window of its own reads the saved values again.
 */
export const SETTINGS_CHANGED = "settings://changed";

/** Tell every window that the saved preferences changed. */
export function broadcastSettings(): void {
  if (!isDesktop()) {
    return;
  }
  // The event carries no payload: the saved values are the source of truth,
  // and a window that missed one event still catches up with the next. `null`
  // rather than `undefined`, which is not a value JSON can carry.
  void emit(SETTINGS_CHANGED, null).catch(() => {
    // A window that cannot be told still reads the values when it opens.
  });
}

/**
 * Follow preference changes in this window.
 *
 * Returns the function that stops following; the caller owns it, so a
 * component can use this directly as the cleanup of an effect.
 */
export function onSettingsChanged(handler: () => void): () => void {
  if (!isDesktop()) {
    return () => {};
  }

  let disposed = false;
  let unlisten: UnlistenFn | undefined;

  void listen(SETTINGS_CHANGED, handler)
    .then((stop) => {
      if (disposed) {
        stop();
      } else {
        unlisten = stop;
      }
    })
    .catch(() => {
      // The desktop runtime is not there; the saved values were applied when
      // the window opened.
    });

  return () => {
    disposed = true;
    unlisten?.();
  };
}
