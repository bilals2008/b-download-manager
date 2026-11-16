import { CATALOGUES } from "@/i18n/catalogue";
import {
  SETTINGS_PAGES,
  type SettingsPageId,
} from "@/components/settings/settings-nav";
import {
  buildSettingsSearchIndex,
  groupSettingsHits,
  searchSettings,
  type SettingsSearchEntry,
  type SettingsSearchGroup,
  type SettingsSearchHit,
} from "@/lib/settings-search";
import { shortcutSearchRows } from "@/lib/shortcuts";
import { PAGE_ENTRIES_APP } from "@/components/settings/settings-search-entries-app";
import { PAGE_ENTRIES_NETWORK } from "@/components/settings/settings-search-entries-network";

export interface SettingsCatalogEntry extends SettingsSearchEntry {
  page: SettingsPageId;
}

/**
 * Every setting the window owns, in one place. The page still draws its own
 * row; this list only says what that row is called, what it does and which
 * element a search result should open. A row's `settingId` is the anchor here.
 */
const PAGE_ENTRIES: readonly SettingsCatalogEntry[] = [
  ...PAGE_ENTRIES_APP,
  ...PAGE_ENTRIES_NETWORK,
];

/**
 * Every shortcut row joins the same catalogue. The page and the search read
 * the one registry, so a key can never exist on one and not the other.
 */
const SHORTCUT_ENTRIES: readonly SettingsCatalogEntry[] = shortcutSearchRows().map(
  (row) => ({
    id: row.id,
    page: "shortcuts" as const,
    section: "Keyboard shortcuts",
    title: row.label,
    description: row.keys
      ? `${row.hint} The key is ${row.keys}.`
      : row.hint,
    keywords: [
      "shortcut",
      "shortcuts",
      "keyboard",
      "hotkey",
      "key",
      "bind",
      ...(row.keys ? [row.keys] : []),
    ],
  }),
);

export const SETTINGS_SEARCH_ENTRIES: readonly SettingsCatalogEntry[] = [
  ...PAGE_ENTRIES,
  ...SHORTCUT_ENTRIES,
];

export const SETTINGS_PAGE_LABELS: ReadonlyMap<string, string> = new Map(
  SETTINGS_PAGES.map((page) => [page.id, CATALOGUES.en[page.labelKey]]),
);

const SETTINGS_PAGE_ORDER: readonly string[] = SETTINGS_PAGES.map(
  (page) => page.id,
);

/**
 * Prepared once at module load. Searching is a single pass over this array;
 * no component rebuilds it, and nothing is scanned from the DOM.
 */
export const SETTINGS_SEARCH_INDEX = buildSettingsSearchIndex(
  SETTINGS_SEARCH_ENTRIES,
  SETTINGS_PAGE_LABELS,
);

export function searchSettingsCatalog(
  query: string,
): SettingsSearchHit<SettingsCatalogEntry>[] {
  return searchSettings(query, SETTINGS_SEARCH_INDEX);
}

export function groupSettingsCatalogHits(
  hits: readonly SettingsSearchHit<SettingsCatalogEntry>[],
): SettingsSearchGroup<SettingsCatalogEntry>[] {
  return groupSettingsHits(hits, SETTINGS_PAGE_LABELS, SETTINGS_PAGE_ORDER);
}

export type { SettingsSearchGroup, SettingsSearchHit };
