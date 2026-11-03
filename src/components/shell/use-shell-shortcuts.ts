import { useEffect, useRef } from "react";

import { comboFromEvent, matchShortcut, type ShortcutId } from "@/lib/shortcuts";

/**
 * One handler per command id. A command with no handler in the map — and a
 * row the reader cleared — simply does nothing when its key arrives.
 */
export type ShellShortcutHandlers = Partial<Record<ShortcutId, () => void>>;

/** The viewport opts in by carrying this, so list-only keys stay local. */
const LIST_SCOPE_ATTRIBUTE = "data-shortcut-scope";

/**
 * The keyboard shortcut dispatcher for the main window.
 *
 * Every key comes from the registry, so a rebind in Settings takes effect on
 * the next press with no restart. Like a desktop application, the keys work
 * from anywhere in the shell except a text field, an open menu, or a window
 * of its own — with one exception: function keys with no Ctrl or Alt (F3
 * above all) type nothing, so they keep working from the Find box as well.
 */
export function useShellShortcuts(handlers: ShellShortcutHandlers) {
  const latest = useRef(handlers);
  useEffect(() => {
    latest.current = handlers;
  });

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      // A control that already answered the key keeps it.
      if (event.defaultPrevented) {
        return;
      }
      const element = event.target instanceof Element ? event.target : null;
      if (element?.closest("dialog, [role='menu']")) {
        return;
      }
      const combo = comboFromEvent(event);
      if (!combo) {
        return;
      }
      const typing = element?.closest(
        "textarea, select, [contenteditable='true'], input:not([type='checkbox']):not([type='radio']):not([type='button']):not([type='submit'])",
      );
      if (typing) {
        const parts = combo.split("+");
        const modifiers = parts.slice(0, -1);
        const bareFunction =
          modifiers.every((part) => part === "Shift") &&
          /^F([1-9]|1[0-2])$/.test(parts[parts.length - 1]);
        if (!bareFunction) {
          return;
        }
      }
      const id = matchShortcut(
        event,
        Boolean(element?.closest(`[${LIST_SCOPE_ATTRIBUTE}='list']`)),
      );
      if (!id) {
        return;
      }
      const action = latest.current[id];
      if (!action) {
        return;
      }
      event.preventDefault();
      action();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
