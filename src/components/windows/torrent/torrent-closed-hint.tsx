import { useEffect, useState } from "react";

import { listen } from "@tauri-apps/api/event";

import { CloseGlyph } from "@/components/icons/ui-icons";
import { AppMark } from "@/components/shell/app-mark";
import { isDesktop } from "@/lib/errors";

/**
 * The event the backend sends once the torrent window is gone.
 *
 * The close button, Escape and the window's own close all end the same way,
 * so the backend tells the main window instead of the window telling itself.
 */
export const TORRENT_CLOSED = "torrent://closed";

/** Long enough to read twice, short enough not to sit on the list. */
const HINT_MS = 12_000;

/**
 * The hint after the torrent window closes.
 *
 * A closed window leaves nothing behind, so the main window says where the
 * window went and how to bring it back. It is not a dialog: it takes no
 * keyboard, it does not cover the list, and it goes away by itself.
 */
export function TorrentClosedHint() {
  const [shown, setShown] = useState(false);

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let stop: (() => void) | undefined;
    void listen(TORRENT_CLOSED, () => setShown(true))
      .then((unlisten) => {
        stop = unlisten;
      })
      .catch(() => {
        // The desktop runtime is not there, so there is nothing to report.
      });
    return () => stop?.();
  }, []);

  useEffect(() => {
    if (!shown) {
      return;
    }
    let timer: number | undefined;
    // The main window can be in the tray when the torrent window closes, and a
    // hint that expires unseen is no hint: it waits for the window to show.
    function start(): void {
      window.clearTimeout(timer);
      timer = undefined;
      if (document.hidden) {
        return;
      }
      timer = window.setTimeout(() => setShown(false), HINT_MS);
    }
    start();
    document.addEventListener("visibilitychange", start);
    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", start);
    };
  }, [shown]);

  if (!shown) {
    return null;
  }

  return (
    <div
      role="status"
      className="fixed right-4 bottom-9 z-50 w-[21rem] rounded-lg bg-chrome-popup p-3 text-chrome-menu-text shadow-float motion-safe:animate-details-in motion-reduce:animate-none"
    >
      <div className="flex items-start gap-2.5">
        <AppMark className="mt-0.5 size-5" />
        <div className="min-w-0 flex-1">
          <div className="flex items-start justify-between gap-2">
            <p className="text-xs font-medium text-chrome-title-text">
              Torrents window closed
            </p>
            <button
              type="button"
              aria-label="Dismiss"
              onClick={() => setShown(false)}
              className="-mt-1 -mr-1 flex size-6 shrink-0 items-center justify-center rounded-chrome text-chrome-muted transition-colors hover:bg-chrome-menu-hover hover:text-chrome-menu-text focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
            >
              <CloseGlyph className="size-3" />
            </button>
          </div>
          <p className="mt-0.5 text-chrome-micro text-chrome-muted">
            Open it again from{" "}
            <span className="text-chrome-menu-text">Tasks › Torrents</span>, or
            from the tray menu while the tray icon is showing.
          </p>
        </div>
      </div>
    </div>
  );
}
