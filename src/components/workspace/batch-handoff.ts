import { useState } from "react";

import {
  batchPatternFromText,
  emptyBatchPattern,
  type BatchPattern,
} from "@/lib/batch";
import { readClipboardText } from "@/lib/ipc";

/**
 * The batch dialog's state, kept out of the window's shell.
 *
 * The menu opens it empty. "Add batch download from clipboard" opens it at
 * once with the address the reader copied already broken into a pattern, so
 * the box is never empty while the clipboard is being read.
 */
export function useBatchHandoff() {
  const [open, setOpen] = useState(false);
  /** The pattern to open with, or null for an empty form. */
  const [seed, setSeed] = useState<BatchPattern | null>(null);

  return {
    open,
    seed,
    /** Open the dialog empty, from the menu or the toolbar. */
    openBlank() {
      setSeed(null);
      setOpen(true);
    },
    /** Open the dialog on the address on the clipboard, when it holds one. */
    openFromClipboard() {
      setSeed(null);
      setOpen(true);
      void readClipboardText()
        .then((text) => {
          const pattern = batchPatternFromText(text);
          // A clipboard with no address, or one with no number in it, leaves
          // the form empty rather than guessing at a range.
          if (pattern) {
            setSeed(pattern);
          } else {
            setSeed((current) => current ?? emptyBatchPattern());
          }
        })
        .catch(() => {
          // A clipboard the system will not read is an empty form.
        });
    },
    close() {
      setOpen(false);
      setSeed(null);
    },
  };
}
