/**
 * The tips the application offers, and where the reader got to.
 *
 * A tip only describes something the window can already do. When a feature is
 * added, add a tip for it here; when one is removed, delete its tip, so the
 * carousel never teaches a screen that is not there.
 *
 * The position is stored beside the preferences, not inside `Settings`: the
 * options window does not own it, and every open moves it forward.
 */

export interface Tip {
  /** Stable name, so the collection can be checked for duplicates. */
  id: string;
  /** The part of the application the tip is about. */
  category: string;
  title: string;
  body: string;
  /** An accelerator the tip is about, printed on a key cap. */
  shortcut?: string;
}

export const TIPS: readonly Tip[] = [
  {
    id: "add-anywhere",
    category: "Shortcuts",
    title: "Add a download from anywhere",
    body: "Press Ctrl+N in the main window to open Add URL and paste an address. Nothing starts until you press Download.",
    shortcut: "Ctrl+N",
  },
  {
    id: "find-names-and-addresses",
    category: "Downloads",
    title: "Search names and addresses",
    body: "Ctrl+F searches the file name and the source address together. F3 and Shift+F3 step through the matches.",
    shortcut: "Ctrl+F",
  },
  {
    id: "shortcut-reference",
    category: "Shortcuts",
    title: "Every key in one place",
    body: "Settings > Shortcuts lists the keys the main window listens for, grouped by where they apply, and marks the ones still to come.",
  },
  {
    id: "act-on-selection",
    category: "Downloads",
    title: "Act on several downloads at once",
    body: "Select rows with Ctrl or Shift, then right-click. Resume, stop, queue and category changes apply to the whole selection.",
  },
  {
    id: "row-menu",
    category: "Downloads",
    title: "The row menu holds every action",
    body: "Right-click a download to copy its address or path, open its folder, show its progress window, or read its properties.",
  },
  {
    id: "queue-later",
    category: "Queues",
    title: "Line downloads up in the queue",
    body: "Right-click a download and choose Add to queue to park it. Start queue and Stop queue sit in the Downloads menu and on the toolbar.",
  },
  {
    id: "shared-speed-limit",
    category: "Speed",
    title: "Cap the bandwidth for everything",
    body: "Settings > Connections turns on one shared speed limit in KB/s. Every running and new download stays under it.",
  },
  {
    id: "more-connections",
    category: "Speed",
    title: "Give a download more connections",
    body: "Settings > Downloads sets how many parts a new download may fetch at once. Settings > Connections caps the total open across the app.",
  },
  {
    id: "category-rules",
    category: "Categories",
    title: "Let categories file downloads for you",
    body: "Right-click the categories pane and choose Add category. Give it file types and a folder, and matching downloads are filed there automatically.",
  },
  {
    id: "category-folder",
    category: "Categories",
    title: "Send a category to its own folder",
    body: "Every category, built-in ones included, can point at its own folder in its properties. An empty folder follows the default save folder.",
  },
  {
    id: "existing-files",
    category: "Downloads",
    title: "Decide what happens to an existing file",
    body: "Settings > Downloads can ask, save under a free name, replace the old file, or skip the download when the name is already taken.",
  },
  {
    id: "browser-handover",
    category: "Browser",
    title: "Hand downloads over from your browser",
    body: "With the extension installed, links come straight to the app. Settings > Browser integration decides which file types are taken over.",
  },
  {
    id: "browser-ask",
    category: "Browser",
    title: "Review a link before it starts",
    body: "Turn on Ask before saving in Browser integration, and a hand-over opens Add URL instead of starting on its own.",
  },
  {
    id: "browser-skip-sites",
    category: "Browser",
    title: "Keep some sites in the browser",
    body: "Add a site to Leave these sites to the browser on the Browser integration page. The rule covers the site and everything under it.",
  },
  {
    id: "tray-running",
    category: "Tray",
    title: "Close the window, keep the downloads",
    body: "By default the close button leaves the app running in the tray. The General page can also make the minimize button hide the window.",
  },
  {
    id: "status-bar",
    category: "Downloads",
    title: "Read the totals at a glance",
    body: "The status bar shows how many downloads are in view, how many are selected, their combined size, and the total transfer rate.",
  },
  {
    id: "sort-list",
    category: "Downloads",
    title: "Sort the list your way",
    body: "View > Arrange files sorts by order added, name, size, status, time left, speed, last try or save path.",
  },
  {
    id: "column-widths",
    category: "Downloads",
    title: "Make a column fit",
    body: "Drag a column divider to resize it, use the arrow keys when it has focus, or double-click a divider to reset all columns.",
  },
  {
    id: "progress-window",
    category: "Downloads",
    title: "Open a window for one download",
    body: "Double-click a download for its progress window. Settings > Downloads can also open it whenever a download starts.",
  },
  {
    id: "retry-failed",
    category: "Downloads",
    title: "Retry a failed download",
    body: "A failed row offers Show error and retry in its menu. The window explains what went wrong before anything is restarted.",
  },
  {
    id: "theme",
    category: "Appearance",
    title: "Make the window yours",
    body: "Settings > Appearance switches between Auto, Light and Dark, and changes the interface font and weight.",
  },
  {
    id: "site-grabber",
    category: "Tools",
    title: "Collect files from a whole site",
    body: "Tasks > Run site grabber opens its own window, so a crawl keeps going while the download list stays usable.",
  },
  {
    id: "clipboard-links",
    category: "Browser",
    title: "Catch copied links automatically",
    body: "Turn on clipboard watching on the General page, and a copied download link opens Add URL with the address already filled in.",
  },
  {
    id: "toolbar-size",
    category: "Appearance",
    title: "Choose a smaller toolbar",
    body: "View > Toolbar switches between large and small buttons, and the choice is remembered.",
  },
  {
    id: "toolbar-customize",
    category: "Appearance",
    title: "Put your buttons on the bar",
    body: "View > Toolbar > Customize moves buttons on and off the bar, changes their order and adds separators. An installed toolbar keeps its own layout.",
  },
];

const CURSOR_KEY = "idm-clone.tips.v1";

/** The index of the tip that was shown last, or -1 before the first one. */
export function loadTipCursor(): number {
  try {
    const raw = window.localStorage.getItem(CURSOR_KEY);
    if (!raw) {
      return -1;
    }
    const value = JSON.parse(raw) as { index?: unknown };
    const index = value?.index;
    return typeof index === "number" &&
      Number.isInteger(index) &&
      index >= 0 &&
      index < TIPS.length
      ? index
      : -1;
  } catch {
    return -1;
  }
}

export function saveTipCursor(index: number): void {
  try {
    window.localStorage.setItem(CURSOR_KEY, JSON.stringify({ index }));
  } catch {
    // Storage can be blocked; the next open falls back to the first tip.
  }
}

/** Forget the position, so the next open is the first tip again. */
export function resetTipCursor(): void {
  try {
    window.localStorage.removeItem(CURSOR_KEY);
  } catch {
    // Storage can be blocked; there is nothing to clear in that case.
  }
}

/** The next tip in the run, remembered before it is shown. */
export function nextTipIndex(): number {
  const next = (loadTipCursor() + 1) % TIPS.length;
  saveTipCursor(next);
  return next;
}

/** Move through the collection, wrapping at either end. */
export function stepTipIndex(index: number, step: number): number {
  const current =
    Number.isInteger(index) && index >= 0 && index < TIPS.length ? index : 0;
  return (current + step + TIPS.length) % TIPS.length;
}
