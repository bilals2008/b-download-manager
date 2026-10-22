import { useEffect, useRef } from "react";

interface ShellShortcuts {
  onAddDownload: () => void;
  onFind: () => void;
  onFindNext: (step: 1 | -1) => void;
}

/**
 * The accelerators the menus print: Ctrl+N, Ctrl+F and F3. They work from
 * anywhere in the shell except a text field, an open menu or a window of its
 * own, the way a desktop application treats a shortcut. Delete is not here:
 * it belongs to the download list, which is the only place it means anything.
 */
export function useShellShortcuts(shortcuts: ShellShortcuts) {
  const latest = useRef(shortcuts);
  useEffect(() => {
    latest.current = shortcuts;
  });

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const element = event.target instanceof Element ? event.target : null;
      const actions = latest.current;

      if (element?.closest("dialog, [role='menu']")) {
        return;
      }

      // F3 types nothing, so it works from the Find box as well.
      if (event.key === "F3") {
        event.preventDefault();
        actions.onFindNext(event.shiftKey ? -1 : 1);
        return;
      }

      const typing = element?.closest(
        "textarea, select, [contenteditable='true'], input:not([type='checkbox']):not([type='radio']):not([type='button']):not([type='submit'])",
      );
      const command = event.ctrlKey || event.metaKey;
      if (typing || !command || event.altKey) {
        return;
      }
      if (event.key.toLowerCase() === "n") {
        event.preventDefault();
        actions.onAddDownload();
      } else if (event.key.toLowerCase() === "f") {
        event.preventDefault();
        actions.onFind();
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
