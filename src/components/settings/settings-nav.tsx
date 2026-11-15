import type { SettingsGlyphKind } from "@/components/icons/ui-icons";
import type { StringKey } from "@/i18n/catalogue";

/** One page of the options window. */
export interface SettingsPageEntry {
  id: SettingsPageId;
  labelKey: StringKey;
  glyph: SettingsGlyphKind;
  /** The catalogue key of the one line under the page title. */
  descriptionKey: StringKey;
}

export type SettingsPageId =
  | "general"
  | "appearance"
  | "toolbar"
  | "shortcuts"
  | "tips"
  | "downloads"
  | "save-locations"
  | "browser"
  | "video-download-button"
  | "connections"
  | "proxy"
  | "debrid"
  | "remote"
  | "torrent"
  | "advanced"
  | "about";

/**
 * The pages, grouped the way the window reads. The order is also the order
 * the sidebar draws. Every word here is a catalogue key, so the window
 * reads in the language the reader chose.
 */
export const SETTINGS_GROUPS: readonly {
  labelKey: StringKey;
  pages: readonly SettingsPageEntry[];
}[] = [
  {
    labelKey: "settings.sidebar.basics",
    pages: [
      {
        id: "general",
        labelKey: "settings.general",
        glyph: "general",
        descriptionKey: "settings.general.description",
      },
      {
        id: "appearance",
        labelKey: "settings.appearance",
        glyph: "appearance",
        descriptionKey: "settings.appearance.description",
      },
      {
        id: "toolbar",
        labelKey: "settings.toolbar",
        glyph: "toolbar",
        descriptionKey: "settings.toolbar.description",
      },
      {
        id: "shortcuts",
        labelKey: "settings.shortcuts",
        glyph: "shortcuts",
        descriptionKey: "settings.shortcuts.description",
      },
      {
        id: "tips",
        labelKey: "settings.tips",
        glyph: "tips",
        descriptionKey: "settings.tips.description",
      },
    ],
  },
  {
    labelKey: "settings.sidebar.downloads",
    pages: [
      {
        id: "downloads",
        labelKey: "settings.downloads",
        glyph: "downloads",
        descriptionKey: "settings.downloads.description",
      },
      {
        id: "save-locations",
        labelKey: "settings.save-locations",
        glyph: "save-to",
        descriptionKey: "settings.save-locations.description",
      },
      {
        id: "browser",
        labelKey: "settings.browser",
        glyph: "browser",
        descriptionKey: "settings.browser.description",
      },
      {
        id: "video-download-button",
        labelKey: "settings.video-download-button",
        glyph: "downloads",
        descriptionKey: "settings.video-download-button.description",
      },
    ],
  },
  {
    labelKey: "settings.sidebar.services",
    pages: [
      {
        id: "debrid",
        labelKey: "settings.debrid",
        glyph: "sites",
        descriptionKey: "settings.debrid.description",
      },
      {
        id: "remote",
        labelKey: "settings.remote",
        glyph: "dial-up",
        descriptionKey: "settings.remote.description",
      },
      {
        id: "torrent",
        labelKey: "settings.torrent",
        glyph: "downloads",
        descriptionKey: "settings.torrent.description",
      },
    ],
  },
  {
    labelKey: "settings.sidebar.network",
    pages: [
      {
        id: "connections",
        labelKey: "settings.connections",
        glyph: "connection",
        descriptionKey: "settings.connections.description",
      },
      {
        id: "proxy",
        labelKey: "settings.proxy",
        glyph: "proxy",
        descriptionKey: "settings.proxy.description",
      },
    ],
  },
  {
    labelKey: "settings.sidebar.application",
    pages: [
      {
        id: "advanced",
        labelKey: "settings.advanced",
        glyph: "advanced",
        descriptionKey: "settings.advanced.description",
      },
      {
        id: "about",
        labelKey: "settings.about",
        glyph: "about",
        descriptionKey: "settings.about.description",
      },
    ],
  },
];

/** The flat list, for the title of the page that is open. */
export const SETTINGS_PAGES: readonly SettingsPageEntry[] = SETTINGS_GROUPS.flatMap(
  (group) => group.pages,
);

function pageEntry(id: SettingsPageId): SettingsPageEntry | undefined {
  return SETTINGS_PAGES.find((page) => page.id === id);
}

/** The key of the name of a page, for the region the reader is in. */
export function settingsPageLabelKey(id: SettingsPageId): StringKey {
  return pageEntry(id)?.labelKey ?? "settings.title";
}

/** The key of the line under the page title. */
export function settingsPageDescriptionKey(id: SettingsPageId): StringKey {
  return pageEntry(id)?.descriptionKey ?? "settings.title";
}
