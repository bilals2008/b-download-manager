import type { SettingsGlyphKind } from "@/components/icons/ui-icons";

interface SettingsPageEntry {
  id: string;
  label: string;
  glyph: SettingsGlyphKind;
}

/** The pages this build opens, in the order IDM lists them. */
export const OPEN_PAGES = [
  { id: "general", label: "General", glyph: "general" },
  { id: "downloads", label: "Downloads", glyph: "downloads" },
  { id: "network", label: "Network", glyph: "connection" },
] as const satisfies readonly SettingsPageEntry[];

/**
 * The pages this build does not have yet. They share one tab and one page, so
 * the working pages are not crowded by pages that cannot be opened.
 */
export const PLANNED_PAGES = [
  {
    id: "file-types",
    label: "File types",
    glyph: "file-types",
    note: "Which downloads start by themselves and which ask first.",
  },
  {
    id: "browser",
    label: "Browser capture",
    glyph: "browser",
    note: "Downloads started from a browser, its menu and the keyboard.",
  },
  {
    id: "sites",
    label: "Sites Logins",
    glyph: "sites",
    note: "Saved logins for sites that ask before they hand over a file.",
  },
  {
    id: "dial-up",
    label: "Dial Up / VPN",
    glyph: "dial-up",
    note: "The connection opened before a download starts.",
  },
  {
    id: "sounds",
    label: "Sounds",
    glyph: "sounds",
    note: "What plays when a download starts, finishes or fails.",
  },
] as const satisfies readonly (SettingsPageEntry & { note: string })[];

type SettingsPageId =
  | (typeof OPEN_PAGES)[number]["id"]
  | (typeof PLANNED_PAGES)[number]["id"];

/** The tab that holds every page above, none of which open on their own. */
export const PLANNED_TAB = "planned";

export type SettingsTabId = SettingsPageId | typeof PLANNED_TAB;
