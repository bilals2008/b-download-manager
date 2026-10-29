import type { SettingsGlyphKind } from "@/components/icons/ui-icons";

/** One page of the options window. */
export interface SettingsPageEntry {
  id: SettingsPageId;
  label: string;
  glyph: SettingsGlyphKind;
}

export type SettingsPageId =
  | "general"
  | "appearance"
  | "shortcuts"
  | "tips"
  | "downloads"
  | "save-locations"
  | "browser"
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
      { id: "general", label: "General", glyph: "general" },
      { id: "appearance", label: "Appearance", glyph: "appearance" },
      { id: "shortcuts", label: "Shortcuts", glyph: "shortcuts" },
      { id: "tips", label: "Tips", glyph: "tips" },
    ],
  },
  {
    label: "Downloads",
    pages: [
      { id: "downloads", label: "Downloads", glyph: "downloads" },
      { id: "save-locations", label: "Save locations", glyph: "save-to" },
      { id: "browser", label: "Browser integration", glyph: "browser" },
    ],
  },
  {
    label: "Network",
    pages: [
      { id: "connections", label: "Connections", glyph: "connection" },
      { id: "proxy", label: "Proxy", glyph: "proxy" },
    ],
  },
  {
    label: "Application",
    pages: [
      { id: "advanced", label: "Advanced", glyph: "advanced" },
      { id: "about", label: "About", glyph: "about" },
    ],
  },
];

/** The flat list, for the title of the page that is open. */
export const SETTINGS_PAGES: readonly SettingsPageEntry[] = SETTINGS_GROUPS.flatMap(
  (group) => group.pages,
);

/** The name of a page, for the region the reader is in. */
export function settingsPageLabel(id: SettingsPageId): string {
  return SETTINGS_PAGES.find((page) => page.id === id)?.label ?? "Settings";
}
