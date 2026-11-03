/**
 * The keyboard shortcut registry.
 *
 * Every key the application listens for lives here as data: a command id, a
 * default combination, and the scope that decides where the key is heard. The
 * reader's changes are stored as overrides in `localStorage`, beside the other
 * saved preferences, so a rebound key survives a restart and every window
 * reads one copy.
 *
 * The Settings page, the menus, the toolbar tooltips and the dispatcher all
 * ask this module what the key is; nothing hardcodes a combination elsewhere.
 */

import type { ShellCommand } from "@/components/shell/commands";

/** Where a key is heard: anywhere in the shell, or only in the download list. */
export type ShortcutScope = "global" | "list";

export type ShortcutId =
  // Downloads
  | "add-download"
  | "find"
  | "find-next"
  | "find-previous"
  | "open-downloads-folder"
  | "pause-all"
  | "resume-all"
  | "stop-all"
  // The selection
  | "pause-selected"
  | "resume-selected"
  | "cancel-selected"
  | "remove-selected"
  | "select-all"
  | "clear-selection"
  // Queues
  | "start-queue"
  | "stop-queue"
  // The window
  | "open-settings"
  | "cycle-sections"
  | "cycle-sections-back";

export interface ShortcutDefinition {
  id: ShortcutId;
  label: string;
  /** The row's tooltip and search description. */
  hint: string;
  group: string;
  scope: ShortcutScope;
  /** The shipped combination, in canonical form, or null when it starts clear. */
  defaultBinding: string | null;
}

/**
 * The shipped keys. Windows conventions where one exists (Ctrl+N, Ctrl+F, F3,
 * Ctrl+A, Delete, Ctrl+,), Ctrl+Shift for actions on the selection, and
 * Ctrl+Alt for actions on every download at once, so the two families never
 * collide with each other or with the menu bar's Alt+letter access keys.
 */
export const SHORTCUT_DEFINITIONS: readonly ShortcutDefinition[] = [
  {
    id: "add-download",
    label: "Add a download",
    hint: "Opens the Add URL dialog from anywhere in the main window.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+N",
  },
  {
    id: "find",
    label: "Search downloads",
    hint: "Searches the file name and the address.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+F",
  },
  {
    id: "find-next",
    label: "Find next",
    hint: "Steps to the next match. Works while typing in the Find box.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "F3",
  },
  {
    id: "find-previous",
    label: "Find previous",
    hint: "Steps back to the previous match.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Shift+F3",
  },
  {
    id: "open-downloads-folder",
    label: "Open Downloads folder",
    hint: "Opens the folder new downloads are saved to.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+Shift+O",
  },
  {
    id: "pause-all",
    label: "Pause all downloads",
    hint: "Pauses every download that is running.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+Alt+P",
  },
  {
    id: "resume-all",
    label: "Resume all downloads",
    hint: "Starts every download that is paused or stopped.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+Alt+R",
  },
  {
    id: "stop-all",
    label: "Stop all downloads",
    hint: "Cancels every download that is running.",
    group: "Downloads",
    scope: "global",
    defaultBinding: "Ctrl+Alt+S",
  },
  {
    id: "pause-selected",
    label: "Pause selected",
    hint: "Pauses the downloads the list has selected.",
    group: "Selected downloads",
    scope: "global",
    defaultBinding: "Ctrl+Shift+P",
  },
  {
    id: "resume-selected",
    label: "Resume selected",
    hint: "Starts the selected downloads again from where they stopped.",
    group: "Selected downloads",
    scope: "global",
    defaultBinding: "Ctrl+Shift+Enter",
  },
  {
    id: "cancel-selected",
    label: "Cancel selected",
    hint: "Stops the selected downloads without removing them.",
    group: "Selected downloads",
    scope: "global",
    defaultBinding: "Ctrl+Shift+S",
  },
  {
    id: "remove-selected",
    label: "Remove selected",
    hint: "Works while the download list has the keyboard.",
    group: "Selected downloads",
    scope: "list",
    defaultBinding: "Delete",
  },
  {
    id: "select-all",
    label: "Select all",
    hint: "Selects every download the list is showing.",
    group: "Selection",
    scope: "global",
    defaultBinding: "Ctrl+A",
  },
  {
    id: "clear-selection",
    label: "Deselect all",
    hint: "Esc also clears the selection while the list has the keyboard.",
    group: "Selection",
    scope: "global",
    defaultBinding: "Ctrl+Shift+A",
  },
  {
    id: "start-queue",
    label: "Start the queue",
    hint: "Runs the next download in the queue.",
    group: "Queues",
    scope: "global",
    defaultBinding: "Ctrl+Alt+Q",
  },
  {
    id: "stop-queue",
    label: "Stop the queue",
    hint: "Holds the queue until it is started again.",
    group: "Queues",
    scope: "global",
    defaultBinding: null,
  },
  {
    id: "open-settings",
    label: "Open Settings",
    hint: "Opens the settings window.",
    group: "The window",
    scope: "global",
    defaultBinding: "Ctrl+,",
  },
  {
    id: "cycle-sections",
    label: "Move to the next section",
    hint: "Steps the keyboard through the menu bar, the categories and the list.",
    group: "The window",
    scope: "global",
    defaultBinding: "Ctrl+Tab",
  },
  {
    id: "cycle-sections-back",
    label: "Move to the previous section",
    hint: "Steps the keyboard backwards through the same three places.",
    group: "The window",
    scope: "global",
    defaultBinding: "Ctrl+Shift+Tab",
  },
];

/**
 * The keys that belong to a control's own behavior — the list navigation a
 * desktop user expects, the menu bar's access keys, the window close. They are
 * printed on the Settings page but cannot be rebound, because the control they
 * drive would stop working quietly. The `id` is the anchor a search result
 * scrolls to, in the same shape a command row uses.
 */
export const FIXED_SHORTCUT_ROWS: readonly {
  id: string;
  label: string;
  keys: string;
  hint?: string;
}[] = [
  {
    id: "shortcut-fixed-navigation",
    label: "Move through downloads",
    keys: "↑ / ↓",
    hint: "Hold Shift to extend the selection.",
  },
  {
    id: "shortcut-fixed-jump",
    label: "Jump to the first or last download",
    keys: "Home / End",
    hint: "Hold Shift to extend the selection.",
  },
  { id: "shortcut-fixed-clear-esc", label: "Clear the selection", keys: "Esc" },
  {
    id: "shortcut-fixed-context-menu",
    label: "Open the context menu",
    keys: "Shift+F10",
    hint: "The Menu key on the keyboard does the same.",
  },
  {
    id: "shortcut-fixed-menu-bar",
    label: "Open a menu",
    keys: "Alt+T",
    hint: "Alt+F, Alt+D, Alt+V and Alt+H open the other menus.",
  },
  {
    id: "shortcut-fixed-exit",
    label: "Exit",
    keys: "Alt+F4",
    hint: "Active downloads ask before the window closes.",
  },
];

const DEFINITION_BY_ID = new Map(
  SHORTCUT_DEFINITIONS.map((definition) => [definition.id, definition]),
);

export function shortcutDefinition(id: ShortcutId): ShortcutDefinition {
  const found = DEFINITION_BY_ID.get(id);
  if (!found) {
    throw new Error(`Unknown shortcut id: ${id}`);
  }
  return found;
}

/**
 * The shortcut each menu and toolbar command prints on its row. A command
 * not listed here, or whose key the reader cleared, shows nothing.
 */
const COMMAND_SHORTCUT: Partial<Record<ShellCommand, ShortcutId>> = {
  "add-download": "add-download",
  find: "find",
  "find-next": "find-next",
  stop: "cancel-selected",
  resume: "resume-selected",
  delete: "remove-selected",
  "pause-all": "pause-all",
  "stop-all": "stop-all",
  options: "open-settings",
  "start-queue": "start-queue",
  "stop-queue": "stop-queue",
};

/** The key cap a menu row or tooltip prints for a command; null for no key. */
export function commandShortcutLabel(command: ShellCommand): string | null {
  const id = COMMAND_SHORTCUT[command];
  if (!id) {
    return null;
  }
  const binding = getEffectiveBinding(id);
  return binding ? formatBinding(binding) : null;
}

/* --------------------------------------------------------------------------
 * Canonical combinations
 * ----------------------------------------------------------------------- */

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
  "_": "-",
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

/* --------------------------------------------------------------------------
 * Validation and conflicts
 * ----------------------------------------------------------------------- */

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

/* --------------------------------------------------------------------------
 * Persistence
 * ----------------------------------------------------------------------- */

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

/* --------------------------------------------------------------------------
 * Matching a keydown
 * ----------------------------------------------------------------------- */

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

/* --------------------------------------------------------------------------
 * The rows the settings search indexes
 * ----------------------------------------------------------------------- */

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
