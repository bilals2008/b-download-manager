import { useState } from "react";

/**
 * The torrent dialog's state, kept out of the window's shell.
 *
 * A magnet or a `.torrent` pasted into Add URL arrives here: the dialog opens
 * with the link already in it and looks it up, so the reader sees its files
 * without pressing anything twice. Opening it from the menu leaves the field
 * empty instead, because there is nothing to look up yet.
 */
export function useTorrentHandoff() {
  const [open, setOpen] = useState(false);
  // The link handed over, or null when the dialog was opened on its own.
  const [seed, setSeed] = useState<string | null>(null);

  return {
    open,
    seed,
    /** Open the dialog with a link another dialog handed over. */
    openWith(source: string) {
      setSeed(source);
      setOpen(true);
    },
    /** Open the dialog empty, from the menu or the toolbar. */
    openBlank() {
      setSeed(null);
      setOpen(true);
    },
    close() {
      setOpen(false);
      setSeed(null);
    },
  };
}
