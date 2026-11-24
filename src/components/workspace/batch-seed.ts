import { useEffect, useRef, useState } from "react";

import { emptyBatchPattern, type BatchPattern } from "@/lib/batch";

/**
 * The pattern the batch dialog shows, and where it came from.
 *
 * Two things seed the form. The menu opens it empty. "Add batch download from
 * clipboard" opens it at once and reads the clipboard beside it, so the dialog
 * is never a blank box while the system is being asked. That read finishes a
 * moment later, which is exactly when a reader may already have started
 * typing: a late answer must fill an untouched form, never replace words that
 * are already there. The rule is kept here, next to the state it guards, so
 * the dialog only renders what this returns.
 */
export function useBatchSeed(open: boolean, seed: BatchPattern | null) {
  const [pattern, setPattern] = useState<BatchPattern>(emptyBatchPattern);
  /** Whether the reader has written in the form since it was opened. */
  const edited = useRef(false);
  /** Whether the last render had the dialog open, so a seed can be late. */
  const wasOpen = useRef(false);

  useEffect(() => {
    if (!open) {
      wasOpen.current = false;
      return;
    }

    const fresh = !wasOpen.current;
    wasOpen.current = true;

    if (fresh) {
      edited.current = false;
      setPattern(seed ?? emptyBatchPattern());
      return;
    }

    // The dialog is already open with no seed. Only a clipboard answer that
    // arrives while the form is untouched is applied.
    if (seed && !edited.current) {
      setPattern(seed);
    }
  }, [open, seed]);

  return {
    pattern,
    /** Change a field, and remember that the form is now the reader's. */
    change(update: Partial<BatchPattern>) {
      edited.current = true;
      setPattern((current) => ({ ...current, ...update }));
    },
  };
}
