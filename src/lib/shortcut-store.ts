/**
 * The reader's shortcut overrides, and the questions asked of them.
 *
 * Only the rows the reader changed are kept, in `localStorage` beside the other
 * saved preferences. A saved value is always the canonical spelling, so a
 * hand-edited file cannot quietly create a second meaning for one key.
 */

import type { ShellCommand } from "@/components/shell/commands";
import type { ShortcutId } from "./shortcut-definitions";
import {
  COMMAND_SHORTCUT,
  DEFINITION_BY_ID,
  FIXED_SHORTCUT_ROWS,
  SHORTCUT_DEFINITIONS,
  shortcutDefinition,
} from "./shortcut-definitions";
import type { BindingProblem } from "./shortcut-keys";
import { canonicalCombo, comboFromEvent, formatBinding, validateBinding } from "./shortcut-keys";

/** The key cap a menu row or tooltip prints for a command; null for no key. */
export function commandShortcutLabel(command: ShellCommand): string | null {
  const id = COMMAND_SHORTCUT[command];
  if (!id) {
    return null;
  }
  const binding = getEffectiveBinding(id);
  return binding ? formatBinding(binding) : null;
}

/** The reader's overrides: only the rows they changed are kept. */
type ShortcutOverrides = Partial<Record<ShortcutId, string | null>>;

const STORAGE_KEY = "idm-clone.shortcuts.v1";

let cache: Readonly<Record<ShortcutId, string | null>> | null = null;

function readOverrides(): ShortcutOverrides {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return {};
    }
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") {
      return {};
    }
    const out: ShortcutOverrides = {};
    for (const [key, value] of Object.entries(parsed as Record<string, unknown>)) {
      const definition = DEFINITION_BY_ID.get(key as ShortcutId);
      if (!definition) {
        continue;
      }
      if (value === null) {
        out[definition.id] = null;
        continue;
      }
      if (typeof value !== "string") {
        continue;
      }
      const canonical = canonicalCombo(value);
      if (canonical) {
        out[definition.id] = canonical;
      }
    }
    return out;
  } catch {
    return {};
  }
}

function writeOverrides(overrides: ShortcutOverrides): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(overrides));
  } catch {
    // Storage can be blocked; the change still stands for this session.
  }
}

/**
 * Every command's current binding: the shipped default with the reader's
 * overrides applied. An override is the reader's choice and wins, except when
 * it squats on the default of a command that was left alone — only a
 * hand-edited file can do that, since the editor refuses the clash — and then
 * the override is dropped and the command falls back to its shipped key. Two
 * commands never share a key no matter what is on disk.
 */
export function getShortcutBindings(): Readonly<Record<ShortcutId, string | null>> {
  if (cache) {
    return cache;
  }
  const overrides = readOverrides();
  // The defaults of the commands nobody touched are claimed first.
  const claimed = new Set<string>();
  for (const definition of SHORTCUT_DEFINITIONS) {
    if (overrides[definition.id] === undefined && definition.defaultBinding) {
      claimed.add(definition.defaultBinding);
    }
  }
  const bindings = {} as Record<ShortcutId, string | null>;
  const used = new Set<string>();
  for (const definition of SHORTCUT_DEFINITIONS) {
    const stored = overrides[definition.id];
    if (stored === undefined) {
      // Untouched: its default was claimed above, so no override took it.
      bindings[definition.id] = definition.defaultBinding ?? null;
      if (definition.defaultBinding) {
        used.add(definition.defaultBinding);
      }
      continue;
    }
    if (stored === null) {
      bindings[definition.id] = null;
      continue;
    }
    if (!claimed.has(stored) && !used.has(stored)) {
      bindings[definition.id] = stored;
      used.add(stored);
      continue;
    }
    // The key is another row's shipped, untouched default (only a hand-edited
    // file can say that, the editor refuses it), or two rows claim one key:
    // this row falls back to its shipped key, or to nothing if that is taken.
    const fallback = definition.defaultBinding;
    bindings[definition.id] = fallback && !used.has(fallback) ? fallback : null;
    if (bindings[definition.id]) {
      used.add(bindings[definition.id] as string);
    }
  }
  cache = bindings;
  return bindings;
}

export function getEffectiveBinding(id: ShortcutId): string | null {
  return getShortcutBindings()[id];
}

/** Is the command still on its shipped key? The page shows Reset when not. */
export function bindingIsDefault(id: ShortcutId): boolean {
  return readOverrides()[id] === undefined;
}

/** Ask the store to forget its read copy so the next read sees the file. */
export function invalidateShortcutCache(): void {
  cache = null;
}

/**
 * Rebind one command. The new keys must not belong to the system, the menu
 * bar, or another command; pass null to clear the row. The canonical form is
 * what is stored.
 */
export function setShortcutBinding(
  id: ShortcutId,
  combo: string | null,
): BindingProblem {
  const canonical = combo === null ? null : canonicalCombo(combo);
  if (combo !== null && !canonical) {
    return { error: "That is not a key combination this app can record." };
  }
  const problem = validateBinding(id, canonical, getShortcutBindings());
  if (problem.error) {
    return problem;
  }
  const overrides = readOverrides();
  if (canonical === shortcutDefinition(id).defaultBinding) {
    delete overrides[id];
  } else {
    overrides[id] = canonical;
  }
  writeOverrides(overrides);
  cache = null;
  return { error: null };
}

/** Put one command back on its shipped key. */
export function resetShortcutBinding(id: ShortcutId): void {
  const overrides = readOverrides();
  delete overrides[id];
  writeOverrides(overrides);
  cache = null;
}

/** Put every command back on its shipped key. */
export function resetAllShortcutBindings(): void {
  try {
    window.localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Storage can be blocked; the in-memory reset below still stands.
  }
  cache = null;
}

/** True when at least one command is off its shipped key. */
export function shortcutsAreCustomized(): boolean {
  return Object.keys(readOverrides()).length > 0;
}

/**
 * The command one keydown answers to, scoped the way the registry says.
 * A global key fires from anywhere the caller allows; a list key only fires
 * while the download list itself heard it.
 */
export function matchShortcut(
  event: {
    key: string;
    ctrlKey: boolean;
    altKey: boolean;
    shiftKey: boolean;
    metaKey: boolean;
  },
  fromList: boolean,
): ShortcutId | null {
  const combo = comboFromEvent(event);
  if (!combo) {
    return null;
  }
  const bindings = getShortcutBindings();
  for (const definition of SHORTCUT_DEFINITIONS) {
    if (bindings[definition.id] !== combo) {
      continue;
    }
    if (definition.scope === "list" && !fromList) {
      continue;
    }
    return definition.id;
  }
  return null;
}

/**
 * One row per command, plus the fixed keys, in the shape the search catalogue
 * reads: the anchor id, the label, and the keys as the page prints them.
 */
export function shortcutSearchRows(): readonly {
  id: string;
  label: string;
  keys: string;
  hint: string;
}[] {
  return [
    ...SHORTCUT_DEFINITIONS.map((definition) => ({
      id: `shortcut-${definition.id}`,
      label: definition.label,
      keys: definition.defaultBinding
        ? formatBinding(definition.defaultBinding)
        : "",
      hint: definition.defaultBinding
        ? `${definition.hint} The key can be changed on this page.`
        : `${definition.hint} Starts with no key; set one on this page.`,
    })),
    ...FIXED_SHORTCUT_ROWS.map((row) => ({
      id: row.id,
      label: row.label,
      keys: row.keys,
      hint: row.hint ?? "Fixed, the way the control it belongs to works.",
    })),
  ];
}
