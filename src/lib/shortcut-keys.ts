/**
 * Reading, printing and checking one key combination.
 *
 * A combination is stored in one spelling — modifiers in a fixed order, the
 * key last — so two rows that mean the same keys can be compared as text. The
 * editor accepts what a person types ("ctrl+shift+p", "del", "pgdn") and
 * canonicalizes it before it is saved.
 */

import type { ShortcutId } from "./shortcut-definitions";
import { SHORTCUT_DEFINITIONS } from "./shortcut-definitions";

/** The modifiers in print order. Meta folds into Ctrl, as the app has always done. */
const MODIFIER_ORDER = ["ctrl", "alt", "shift"] as const;

/** The presses that carry no meaning of their own. */
const MODIFIER_KEYS = new Set([
  "Control",
  "Alt",
  "Shift",
  "Meta",
  "OS",
  "CapsLock",
]);

/** The named keys a binding may end with; letters, digits and F1–F12 are
 * matched separately. */
const NAMED_KEYS = new Set([
  "Escape",
  "Enter",
  "Tab",
  "Space",
  "Backspace",
  "Insert",
  "Delete",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  ",",
  "-",
  "=",
  "[",
  "]",
  ";",
  "'",
  "`",
  "/",
  ".",
  "\\",
]);

/** The keys allowed with no Ctrl and no Alt: they type nothing into a field. */
const BARE_KEYS = new Set<string>([
  "Delete",
  "Insert",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  ...Array.from({ length: 12 }, (_, index) => `F${index + 1}`),
]);

/** The names a person may write for one key. */
const KEY_ALIASES: Record<string, string> = {
  esc: "Escape",
  return: "Enter",
  option: "Alt",
  control: "Ctrl",
  up: "ArrowUp",
  down: "ArrowDown",
  left: "ArrowLeft",
  right: "ArrowRight",
  pgup: "PageUp",
  pgdn: "PageDown",
  del: "Delete",
  ins: "Insert",
};

/** The keys a shifted hand actually types, read back to their unshifted form. */
const SHIFTED_SYMBOLS: Record<string, string> = {
  "!": "1",
  "@": "2",
  "#": "3",
  $: "4",
  "%": "5",
  "^": "6",
  "&": "7",
  "*": "8",
  "(": "9",
  ")": "0",
  _: "-",
  "+": "=",
  ":": ";",
  "?": "/",
  "<": ",",
  ">": ".",
  "{": "[",
  "}": "]",
  "|": "\\",
  '"': "'",
  "~": "`",
};

export interface ComboParts {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  /** The canonical key name, e.g. "N", "F3", "ArrowUp". */
  key: string;
}

/** Canonicalize one key name; null when a binding may not use it. */
function canonicalKey(token: string): string | null {
  const trimmed = token === " " ? "Space" : token.trim();
  if (!trimmed) {
    return null;
  }
  const aliased =
    KEY_ALIASES[trimmed.toLowerCase()] ?? SHIFTED_SYMBOLS[trimmed] ?? trimmed;
  if (aliased.length === 1) {
    if (/[a-z0-9]/i.test(aliased)) {
      return aliased.toUpperCase();
    }
    return NAMED_KEYS.has(aliased) ? aliased : null;
  }
  if (/^f([1-9]|1[0-2])$/i.test(aliased)) {
    return `F${aliased.slice(1)}`;
  }
  for (const name of NAMED_KEYS) {
    if (name.toLowerCase() === aliased.toLowerCase()) {
      return name;
    }
  }
  return null;
}

/** Parse "Ctrl+Shift+P" into parts; null when the text is not a binding. */
export function parseCombo(combo: string): ComboParts | null {
  const parts: ComboParts = { ctrl: false, alt: false, shift: false, key: "" };
  let keySeen = false;
  for (const raw of combo.split("+")) {
    const token = raw.trim().toLowerCase();
    if (!token) {
      return null;
    }
    if (token === "ctrl" || token === "control") {
      if (parts.ctrl) {
        return null;
      }
      parts.ctrl = true;
      continue;
    }
    if (token === "alt" || token === "option") {
      if (parts.alt) {
        return null;
      }
      parts.alt = true;
      continue;
    }
    if (token === "shift") {
      if (parts.shift) {
        return null;
      }
      parts.shift = true;
      continue;
    }
    if (
      token === "meta" ||
      token === "cmd" ||
      token === "command" ||
      token === "super"
    ) {
      // Meta is not a separate family in this app; Ctrl already covers it.
      return null;
    }
    const key = canonicalKey(raw);
    if (key === null || keySeen) {
      return null;
    }
    parts.key = key;
    keySeen = true;
  }
  return keySeen ? parts : null;
}

function serializeParts(parts: ComboParts): string {
  const names: string[] = [];
  for (const modifier of MODIFIER_ORDER) {
    if (parts[modifier]) {
      names.push(modifier === "ctrl" ? "Ctrl" : modifier === "alt" ? "Alt" : "Shift");
    }
  }
  return [...names, parts.key].join("+");
}

/** The stored spelling of a combination: modifiers in order, key last. */
export function canonicalCombo(combo: string): string | null {
  const parts = parseCombo(combo);
  return parts ? serializeParts(parts) : null;
}

/** The combination a keydown is asking for; null for a bare modifier press. */
export function comboFromEvent(event: {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}): string | null {
  if (MODIFIER_KEYS.has(event.key)) {
    return null;
  }
  // Shift+"," arrives as "<"; the binding records the unshifted key.
  const raw = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  const key = canonicalKey(event.key === " " ? "Space" : raw);
  if (key === null) {
    return null;
  }
  return serializeParts({
    ctrl: event.ctrlKey || event.metaKey,
    alt: event.altKey,
    shift: event.shiftKey,
    key,
  });
}

/** The friendly text for a binding: "Ctrl+Shift+P", "Del", or "Not set". */
export function formatBinding(combo: string | null): string {
  if (!combo) {
    return "Not set";
  }
  const parts = parseCombo(combo);
  if (!parts) {
    return combo;
  }
  const labels: string[] = [];
  if (parts.ctrl) {
    labels.push("Ctrl");
  }
  if (parts.alt) {
    labels.push("Alt");
  }
  if (parts.shift) {
    labels.push("Shift");
  }
  labels.push(KEY_CAPTIONS[parts.key] ?? parts.key);
  return labels.join("+");
}

const KEY_CAPTIONS: Record<string, string> = {
  Escape: "Esc",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  PageUp: "Page Up",
  PageDown: "Page Down",
  Delete: "Del",
  Insert: "Ins",
};

/**
 * Combinations the operating system or the window already owns. A binding that
 * lands on one is refused before it can shadow the key it is borrowing —
 * Alt+letter is refused by the separate access-key rule below.
 */
const RESERVED_COMBOS = new Set([
  "Alt+F4",
  "Alt+Tab",
  "Ctrl+W",
  "Ctrl+Alt+Delete",
  "Ctrl+Shift+Escape",
]);

/** The letters Alt alone opens: the menu bar answers to these already. */
const MENU_ACCESS_KEYS = new Set(["T", "F", "D", "V", "H"]);

/** Keys a control consumes for itself; bare, they are never shortcut material. */
const FOCUS_CONTROL_KEYS = new Set(["Enter", "Space", "Tab", "Backspace", "Escape"]);

export interface BindingProblem {
  /** The message to show the reader; null when the binding is usable. */
  error: string | null;
  /** Set when the binding is refused because another command holds the keys. */
  conflictWith?: ShortcutId;
}

/**
 * Can this command take this combination right now? Checks the shape, the
 * keys the system owns, the menu bar's Alt+letter access keys, and every
 * other command's current binding.
 */
export function validateBinding(
  id: ShortcutId,
  combo: string | null,
  bindings: Readonly<Record<ShortcutId, string | null>>,
): BindingProblem {
  if (combo === null) {
    // Every command may be cleared; the row simply stops answering its key.
    return { error: null };
  }
  const canonical = canonicalCombo(combo);
  const parts = canonical ? parseCombo(canonical) : null;
  if (!canonical || !parts) {
    return { error: "That is not a key combination this app can record." };
  }
  if (RESERVED_COMBOS.has(canonical)) {
    return { error: "The system already uses that combination." };
  }
  if (parts.alt && !parts.ctrl && MENU_ACCESS_KEYS.has(parts.key)) {
    return {
      error: `Alt+${parts.key} opens a menu. Add Ctrl, or pick another key.`,
    };
  }
  if (!parts.ctrl && !parts.alt) {
    if (FOCUS_CONTROL_KEYS.has(parts.key)) {
      return { error: "That key belongs to the focused control." };
    }
    if (!BARE_KEYS.has(parts.key)) {
      return { error: "Add Ctrl or Alt, or use a function key." };
    }
  }
  for (const other of SHORTCUT_DEFINITIONS) {
    if (other.id === id) {
      continue;
    }
    if (bindings[other.id] === canonical) {
      return {
        error: `Already used by ${other.label}.`,
        conflictWith: other.id,
      };
    }
  }
  return { error: null };
}
