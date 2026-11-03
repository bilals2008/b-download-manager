import type { SettingsGlyphKind } from "@/components/icons/ui-icons";

/** One page of the options window. */
export interface SettingsPageEntry {
  id: SettingsPageId;
  label: string;
  glyph: SettingsGlyphKind;
  /** The one line under the page title: what this page is about. */
  description: string;
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
  | "advanced"
  | "about";

/**
 * The pages, grouped the way the window reads: what the application is, what a
 * download does, how it reaches the network, and the maintenance behind it.
 * The order is also the order the sidebar draws.
 */
export const SETTINGS_GROUPS: readonly {
  label: string;
  pages: readonly SettingsPageEntry[];
}[] = [
  {
    label: "Basics",
    pages: [
      {
        id: "general",
        label: "General",
        glyph: "general",
        description: "Startup, clipboard and the tray.",
      },
      {
        id: "appearance",
        label: "Appearance",
        glyph: "appearance",
        description: "The theme, the typeface and how heavy the text is.",
      },
      {
        id: "toolbar",
        label: "Toolbar",
        glyph: "toolbar",
        description: "The bar under the menu.",
      },
      {
        id: "shortcuts",
        label: "Shortcuts",
        glyph: "shortcuts",
        description: "Every command's key, and how to change it.",
      },
      {
        id: "tips",
        label: "Tips",
        glyph: "tips",
        description: "The hints shown when the application starts.",
      },
    ],
  },
  {
    label: "Downloads",
    pages: [
      {
        id: "downloads",
        label: "Downloads",
        glyph: "downloads",
        description: "How a download starts, and what happens when it ends.",
      },
      {
        id: "save-locations",
        label: "Save locations",
        glyph: "save-to",
        description: "Where files are written, and where they wait unfinished.",
      },
      {
        id: "browser",
        label: "Browser integration",
        glyph: "browser",
        description: "Hand links over from your browser.",
      },
      {
        id: "video-download-button",
        label: "Video download button",
        glyph: "downloads",
        description: "The floating button the extension draws on a video.",
      },
    ],
  },
  {
    label: "Network",
    pages: [
      {
        id: "connections",
        label: "Connections",
        glyph: "connection",
        description: "Parallel connections and the speed cap.",
      },
      {
        id: "proxy",
        label: "Proxy",
        glyph: "proxy",
        description: "The server every download passes through.",
      },
    ],
  },
  {
    label: "Application",
    pages: [
      {
        id: "advanced",
        label: "Advanced",
        glyph: "advanced",
        description: "Application data, and a full reset.",
      },
      {
        id: "about",
        label: "About",
        glyph: "about",
        description: "This build, and how it keeps itself up to date.",
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

/** The name of a page, for the region the reader is in. */
export function settingsPageLabel(id: SettingsPageId): string {
  return pageEntry(id)?.label ?? "Settings";
}

/** The line under the page title, so a page explains itself on arrival. */
export function settingsPageDescription(id: SettingsPageId): string {
  return pageEntry(id)?.description ?? "";
}
