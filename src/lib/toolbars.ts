/**
 * What the toolbar can show, in which order, and where that choice is kept.
 *
 * The catalogue is plain data so the View menu, the toolbar and the Customize
 * window all read one list. The layout is stored here rather than in
 * `Settings`: the options window does not own it, and a toolbar pack brings a
 * layout of its own that the first customization replaces.
 */

const CATALOG = [
  { command: "add-download", label: "Add URL", shortcut: "Ctrl+N" },
  { command: "resume", label: "Resume" },
  { command: "stop", label: "Stop" },
  { command: "stop-all", label: "Stop All" },
  { command: "delete", label: "Delete" },
  { command: "delete-completed", label: "Delete Completed" },
  { command: "options", label: "Options" },
  {
    command: "scheduler",
    label: "Scheduler",
    disabled: true,
    note: "Queues and schedules are coming soon",
  },
  { command: "start-queue", label: "Start Queue" },
  { command: "stop-queue", label: "Stop Queue" },
  { command: "tell-friend", label: "Tell a Friend" },
] as const;

export type ToolbarCommand = (typeof CATALOG)[number]["command"];

export interface ToolbarItemMeta {
  command: ToolbarCommand;
  label: string;
  /** An accelerator the tooltip can print on a key cap. */
  shortcut?: string;
  /** Buttons whose screen is not built yet stay visible but inert. */
  disabled?: boolean;
  /** Why a button is inert, in the tooltip instead of a silent dead control. */
  note?: string;
}

/** Every button a toolbar may carry, in the order a fresh install shows them. */
export const TOOLBAR_CATALOG: readonly ToolbarItemMeta[] = CATALOG;

const COMMANDS = new Set<string>(CATALOG.map((item) => item.command));

/** The row that groups toolbar buttons; it has no command of its own. */
export const TOOLBAR_SEPARATOR = "separator";

export type ToolbarToken = ToolbarCommand | typeof TOOLBAR_SEPARATOR;

/** A toolbar longer than this is a mistake, not a customization. */
export const MAX_TOOLBAR_TOKENS = 64;

export const DEFAULT_TOOLBAR_ID = "default";

/** The button bar the application ships with. */
export const DEFAULT_TOOLBAR_LAYOUT: readonly ToolbarToken[] = TOOLBAR_CATALOG.map(
  (item) => item.command,
);

export function isToolbarCommand(value: unknown): value is ToolbarCommand {
  return typeof value === "string" && COMMANDS.has(value);
}

/**
 * The stored layout of a pack. Commands are unique, separators never sit at
 * either end or beside each other, and a layout with no button at all is not a
 * toolbar: it falls back to the default one.
 */
export function normalizeToolbarLayout(
  value: unknown,
  fallback: readonly ToolbarToken[] = DEFAULT_TOOLBAR_LAYOUT,
): ToolbarToken[] {
  if (!Array.isArray(value)) {
    return [...fallback];
  }

  const out: ToolbarToken[] = [];
  const seen = new Set<ToolbarCommand>();

  for (const entry of value) {
    if (out.length >= MAX_TOOLBAR_TOKENS) {
      break;
    }
    if (entry === TOOLBAR_SEPARATOR) {
      if (out.length === 0 || out[out.length - 1] === TOOLBAR_SEPARATOR) {
        continue;
      }
      out.push(entry);
      continue;
    }
    if (!isToolbarCommand(entry) || seen.has(entry)) {
      continue;
    }
    seen.add(entry);
    out.push(entry);
  }

  while (out[out.length - 1] === TOOLBAR_SEPARATOR) {
    out.pop();
  }

  return seen.size > 0 ? out : [...fallback];
}

/* --------------------------------------------------------------- storage */

export const TOOLBAR_PREFERENCE_KEY = "idm-clone.toolbar.v1";

const PACK_ID = /^[a-z0-9][a-z0-9._-]{0,47}$/;

/** The most layouts one machine keeps; a pack id beyond this is junk. */
const MAX_STORED_LAYOUTS = 32;

export function isToolbarPackId(value: unknown): value is string {
  return typeof value === "string" && PACK_ID.test(value);
}

export interface ToolbarPreference {
  /** The id of the pack in use. */
  active: string;
  /** Customized layouts, by pack id. A missing pack uses its own default. */
  layouts: Record<string, ToolbarToken[]>;
  /** Whether each button prints its name under the icon. */
  labels: boolean;
}

export function defaultToolbarPreference(): ToolbarPreference {
  return { active: DEFAULT_TOOLBAR_ID, layouts: {}, labels: true };
}

/** Read the saved toolbar choice, falling back field by field on bad data. */
export function loadToolbarPreference(): ToolbarPreference {
  try {
    const raw = window.localStorage.getItem(TOOLBAR_PREFERENCE_KEY);
    if (!raw) {
      return defaultToolbarPreference();
    }
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") {
      return defaultToolbarPreference();
    }

    const value = parsed as { active?: unknown; labels?: unknown; layouts?: unknown };
    const layouts: Record<string, ToolbarToken[]> = {};
    if (value.layouts && typeof value.layouts === "object" && !Array.isArray(value.layouts)) {
      let count = 0;
      for (const [id, layout] of Object.entries(value.layouts)) {
        if (count >= MAX_STORED_LAYOUTS) {
          break;
        }
        if (!isToolbarPackId(id)) {
          continue;
        }
        // An empty list means the entry is unusable; dropping it lets the
        // pack's own layout apply instead of pinning the default over it.
        const normalized = normalizeToolbarLayout(layout, []);
        if (normalized.length > 0) {
          layouts[id] = normalized;
          count += 1;
        }
      }
    }

    return {
      active: isToolbarPackId(value.active) ? value.active : DEFAULT_TOOLBAR_ID,
      layouts,
      labels: typeof value.labels === "boolean" ? value.labels : true,
    };
  } catch {
    return defaultToolbarPreference();
  }
}

export function saveToolbarPreference(preference: ToolbarPreference): void {
  try {
    window.localStorage.setItem(TOOLBAR_PREFERENCE_KEY, JSON.stringify(preference));
  } catch {
    // Storage can be blocked; the toolbar still changes for this session.
  }
}

/** The same preference with another pack in use. */
export function withActiveToolbar(
  preference: ToolbarPreference,
  id: string,
): ToolbarPreference {
  if (!isToolbarPackId(id) || id === preference.active) {
    return preference;
  }
  return { ...preference, active: id };
}

/** The same preference with the layout of one pack replaced. */
export function withToolbarLayout(
  preference: ToolbarPreference,
  id: string,
  layout: readonly ToolbarToken[],
): ToolbarPreference {
  if (!isToolbarPackId(id)) {
    return preference;
  }
  return {
    ...preference,
    layouts: {
      ...preference.layouts,
      [id]: normalizeToolbarLayout(layout),
    },
  };
}

/** The same preference with the labels turned on or off. */
export function withToolbarLabels(
  preference: ToolbarPreference,
  labels: boolean,
): ToolbarPreference {
  if (typeof labels !== "boolean" || labels === preference.labels) {
    return preference;
  }
  return { ...preference, labels };
}

/**
 * The layout of a pack: the reader's customization when there is one, the
 * pack's own suggestion otherwise, and the built-in bar when neither says.
 */
export function resolveToolbarLayout(
  preference: ToolbarPreference,
  id: string,
  packLayout: readonly ToolbarToken[] | null | undefined,
): ToolbarToken[] {
  const saved = preference.layouts[id];
  if (saved) {
    return saved;
  }
  return normalizeToolbarLayout(packLayout, DEFAULT_TOOLBAR_LAYOUT);
}
