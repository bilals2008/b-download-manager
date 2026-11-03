import { afterEach, describe, expect, it } from "vitest";

import {
  SHORTCUT_DEFINITIONS,
  bindingIsDefault,
  canonicalCombo,
  commandShortcutLabel,
  comboFromEvent,
  formatBinding,
  getEffectiveBinding,
  getShortcutBindings,
  invalidateShortcutCache,
  matchShortcut,
  parseCombo,
  resetAllShortcutBindings,
  resetShortcutBinding,
  setShortcutBinding,
  shortcutSearchRows,
  shortcutsAreCustomized,
  validateBinding,
} from "@/lib/shortcuts";

const STORAGE_KEY = "idm-clone.shortcuts.v1";

function press(
  key: string,
  modifiers: { ctrl?: boolean; alt?: boolean; shift?: boolean; meta?: boolean } = {},
) {
  return {
    key,
    ctrlKey: modifiers.ctrl ?? false,
    altKey: modifiers.alt ?? false,
    shiftKey: modifiers.shift ?? false,
    metaKey: modifiers.meta ?? false,
  };
}

/** Feed the matcher the event shape it reads, built from a combo's parts. */
function eventFromCombo(combo: string) {
  const parts = parseCombo(combo);
  if (!parts) {
    throw new Error(`bad combo ${combo}`);
  }
  return {
    key: parts.key,
    ctrlKey: parts.ctrl,
    altKey: parts.alt,
    shiftKey: parts.shift,
    metaKey: false,
  };
}

afterEach(() => {
  localStorage.clear();
  invalidateShortcutCache();
});

describe("combination parsing and printing", () => {
  it("reads a keydown as a canonical combination", () => {
    expect(comboFromEvent(press("n", { ctrl: true }))).toBe("Ctrl+N");
    expect(comboFromEvent(press("P", { ctrl: true, shift: true }))).toBe(
      "Ctrl+Shift+P",
    );
    expect(comboFromEvent(press("F3", { shift: true }))).toBe("Shift+F3");
    expect(comboFromEvent(press("Delete"))).toBe("Delete");
    expect(comboFromEvent(press("Tab", { ctrl: true }))).toBe("Ctrl+Tab");
    expect(comboFromEvent(press(",", { ctrl: true }))).toBe("Ctrl+,");
  });

  it("folds the Cmd key into Ctrl", () => {
    expect(comboFromEvent(press("n", { meta: true }))).toBe("Ctrl+N");
  });

  it("ignores a lone modifier and unrecordable keys", () => {
    expect(comboFromEvent(press("Control"))).toBeNull();
    expect(comboFromEvent(press("Shift"))).toBeNull();
    // Shift+"," arrives as "<"; it records as the unshifted key with Shift.
    expect(comboFromEvent({ key: "<", ctrlKey: true, altKey: false, shiftKey: true, metaKey: false })).toBe(
      "Ctrl+Shift+,",
    );
    expect(comboFromEvent(press("Help"))).toBeNull();
  });

  it("normalizes what a person may write", () => {
    expect(canonicalCombo("shift+ctrl+p")).toBe("Ctrl+Shift+P");
    expect(canonicalCombo("Ctrl+ESC")).toBe("Ctrl+Escape");
    expect(canonicalCombo("Alt+Page Up")).toBeNull();
    expect(canonicalCombo("Ctrl+Meta+P")).toBeNull();
    expect(canonicalCombo("Ctrl+")).toBeNull();
    expect(canonicalCombo("")).toBeNull();
    expect(canonicalCombo("Ctrl+Ctrl+P")).toBeNull();
  });

  it("prints what a key cap shows", () => {
    expect(formatBinding("Ctrl+Shift+P")).toBe("Ctrl+Shift+P");
    expect(formatBinding("Delete")).toBe("Del");
    expect(formatBinding("Escape")).toBe("Esc");
    expect(formatBinding("Shift+F10")).toBe("Shift+F10");
    expect(formatBinding(null)).toBe("Not set");
  });

  it("parses only real combinations", () => {
    expect(parseCombo("Ctrl+Shift+P")).toEqual({
      ctrl: true,
      alt: false,
      shift: true,
      key: "P",
    });
    expect(parseCombo("N")).toEqual({
      ctrl: false,
      alt: false,
      shift: false,
      key: "N",
    });
    expect(parseCombo("Ctrl+Shift+Alt+Meta+N")).toBeNull();
  });
});

describe("the shipped defaults", () => {
  it("bind no two commands to the same keys", () => {
    const taken = new Set<string>();
    for (const definition of SHORTCUT_DEFINITIONS) {
      const binding = definition.defaultBinding;
      if (!binding) {
        continue;
      }
      expect(taken.has(binding), `${definition.id} repeats ${binding}`).toBe(false);
      taken.add(binding);
    }
  });

  it("keep the keys the app has always answered to", () => {
    const bindings = getShortcutBindings();
    expect(bindings["add-download"]).toBe("Ctrl+N");
    expect(bindings["find"]).toBe("Ctrl+F");
    expect(bindings["find-next"]).toBe("F3");
    expect(bindings["find-previous"]).toBe("Shift+F3");
    expect(bindings["select-all"]).toBe("Ctrl+A");
    expect(bindings["remove-selected"]).toBe("Delete");
  });

  it("stay off the system and menu-bar keys", () => {
    for (const [id, binding] of Object.entries(getShortcutBindings())) {
      if (!binding) {
        continue;
      }
      expect(binding, id).not.toBe("Alt+F4");
      expect(binding, id).not.toBe("Ctrl+W");
      // Alt alone belongs to the menu bar.
      const parts = parseCombo(binding);
      if (parts?.alt && !parts.ctrl) {
        expect(
          ["T", "F", "D", "V", "H"].includes(parts.key),
          `${id} → ${binding}`,
        ).toBe(false);
      }
    }
  });

  it("are all accepted by the validator", () => {
    const bindings = getShortcutBindings();
    for (const definition of SHORTCUT_DEFINITIONS) {
      const problem = validateBinding(
        definition.id,
        definition.defaultBinding,
        bindings,
      );
      expect(problem.error, definition.id).toBeNull();
    }
  });
});

describe("validateBinding", () => {
  const defaults = getShortcutBindings();

  it("refuses a key another command already holds", () => {
    const problem = validateBinding("add-download", "Ctrl+F", defaults);
    expect(problem.error).toContain("Search downloads");
    expect(problem.conflictWith).toBe("find");
  });

  it("refuses the keys the system owns", () => {
    expect(validateBinding("add-download", "Alt+F4", defaults).error).toContain(
      "system",
    );
    expect(validateBinding("add-download", "Ctrl+W", defaults).error).toContain(
      "system",
    );
  });

  it("refuses the menu bar's Alt letters", () => {
    expect(validateBinding("open-settings", "Alt+D", defaults).error).toContain(
      "opens a menu",
    );
    // The same letter with Ctrl is fine; nothing holds Ctrl+Alt+D.
    expect(validateBinding("open-settings", "Ctrl+Alt+D", defaults).error).toBeNull();
  });

  it("asks for a modifier except on function and navigation keys", () => {
    expect(validateBinding("add-download", "K", defaults).error).toContain(
      "Ctrl or Alt",
    );
    expect(validateBinding("add-download", "Enter", defaults).error).toContain(
      "focused",
    );
    expect(validateBinding("add-download", "F9", defaults).error).toBeNull();
    expect(validateBinding("remove-selected", "Delete", defaults).error).toBeNull();
    expect(validateBinding("find", "Ctrl+Shift+Enter", defaults).error).toContain(
      "Resume selected",
    );
    // The same *shape* — Enter with modifiers — is fine when free.
    expect(validateBinding("find", "Ctrl+Shift+Home", defaults).error).toBeNull();
  });

  it("allows a command to hold its own key and to be cleared", () => {
    expect(validateBinding("add-download", "Ctrl+N", defaults).error).toBeNull();
    expect(validateBinding("add-download", null, defaults).error).toBeNull();
  });
});

describe("persistence", () => {
  it("stores a valid rebind and reads it back after a restart", () => {
    expect(setShortcutBinding("add-download", "Ctrl+Shift+K").error).toBeNull();
    invalidateShortcutCache();
    expect(getEffectiveBinding("add-download")).toBe("Ctrl+Shift+K");
    expect(shortcutsAreCustomized()).toBe(true);
    expect(bindingIsDefault("add-download")).toBe(false);
    expect(bindingIsDefault("find")).toBe(true);
  });

  it("drops the override when the shipped key is re-entered", () => {
    setShortcutBinding("add-download", "Ctrl+Shift+K");
    setShortcutBinding("add-download", "Ctrl+N");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("{}");
    expect(shortcutsAreCustomized()).toBe(false);
  });

  it("refuses to take a key another command holds, changing nothing", () => {
    const problem = setShortcutBinding("add-download", "Ctrl+F");
    expect(problem.error).toContain("Search downloads");
    invalidateShortcutCache();
    expect(getEffectiveBinding("add-download")).toBe("Ctrl+N");
  });

  it("can clear a row and put it back", () => {
    setShortcutBinding("stop-queue", "Ctrl+Alt+E");
    expect(getEffectiveBinding("stop-queue")).toBe("Ctrl+Alt+E");
    setShortcutBinding("stop-queue", null);
    expect(getEffectiveBinding("stop-queue")).toBeNull();
    resetShortcutBinding("stop-queue");
    // Its shipped state is unbound; Reset lands it back there.
    expect(getEffectiveBinding("stop-queue")).toBeNull();
  });

  it("restores every row from the defaults button", () => {
    setShortcutBinding("add-download", "Ctrl+Shift+K");
    resetAllShortcutBindings();
    invalidateShortcutCache();
    expect(getEffectiveBinding("add-download")).toBe("Ctrl+N");
    expect(getEffectiveBinding("find")).toBe("Ctrl+F");
    expect(shortcutsAreCustomized()).toBe(false);
  });

  it("survives a junk file without ever sharing a key", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        "add-download": "Ctrl+F",
        "not-a-command": "Ctrl+G",
        find: 42,
      }),
    );
    const bindings = getShortcutBindings();
    const values = Object.values(bindings).filter(
      (value): value is string => value !== null,
    );
    expect(new Set(values).size).toBe(values.length);
    // The hand edit stole the untouched row's shipped key, so it is dropped
    // and both rows sit on their defaults.
    expect(bindings["add-download"]).toBe("Ctrl+N");
    expect(bindings["find"]).toBe("Ctrl+F");
  });

  it("lets a real swap of two rows stand", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        "add-download": "Ctrl+F",
        find: "Ctrl+N",
      }),
    );
    const bindings = getShortcutBindings();
    // Neither row is untouched, so neither's default is claimed: the swap fits.
    expect(bindings["add-download"]).toBe("Ctrl+F");
    expect(bindings["find"]).toBe("Ctrl+N");
  });

  it("reads the defaults when the storage itself is broken", () => {
    localStorage.setItem(STORAGE_KEY, "{not json");
    expect(getEffectiveBinding("add-download")).toBe("Ctrl+N");
  });
});

describe("matchShortcut", () => {
  it("finds the command a press belongs to", () => {
    expect(matchShortcut(eventFromCombo("Ctrl+N"), false)).toBe("add-download");
    expect(matchShortcut(eventFromCombo("F3"), false)).toBe("find-next");
    expect(matchShortcut(eventFromCombo("Shift+F3"), false)).toBe("find-previous");
    expect(matchShortcut(eventFromCombo("Ctrl+,"), false)).toBe("open-settings");
  });

  it("keeps a list-only key out of the rest of the window", () => {
    const del = eventFromCombo("Delete");
    expect(matchShortcut(del, false)).toBeNull();
    expect(matchShortcut(del, true)).toBe("remove-selected");
  });

  it("honours a rebind and an unbound row", () => {
    setShortcutBinding("add-download", "Ctrl+Alt+K");
    expect(matchShortcut(eventFromCombo("Ctrl+Alt+K"), false)).toBe("add-download");
    expect(matchShortcut(eventFromCombo("Ctrl+N"), false)).toBeNull();
    setShortcutBinding("add-download", null);
    expect(matchShortcut(eventFromCombo("Ctrl+Alt+K"), false)).toBeNull();
  });

  it("answers nothing for keys the registry does not own", () => {
    expect(matchShortcut(eventFromCombo("Ctrl+J"), false)).toBeNull();
    expect(matchShortcut(eventFromCombo("Alt+3"), false)).toBeNull();
  });
});

describe("menu and search readers", () => {
  it("prints the current key on a menu command", () => {
    expect(commandShortcutLabel("add-download")).toBe("Ctrl+N");
    setShortcutBinding("add-download", "F7");
    expect(commandShortcutLabel("add-download")).toBe("F7");
  });

  it("prints nothing for a command with no key or a cleared one", () => {
    expect(commandShortcutLabel("pause-all")).toBe("Ctrl+Alt+P");
    setShortcutBinding("pause-all", null);
    expect(commandShortcutLabel("pause-all")).toBeNull();
    expect(commandShortcutLabel("about")).toBeNull();
  });

  it("gives the search one row per command plus the fixed keys", () => {
    const rows = shortcutSearchRows();
    const ids = rows.map((row) => row.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toContain("shortcut-add-download");
    expect(ids).toContain("shortcut-fixed-menu-bar");
    const queue = rows.find((row) => row.id === "shortcut-stop-queue");
    expect(queue?.keys).toBe("");
    expect(queue?.hint).toContain("Starts with no key");
  });
});
