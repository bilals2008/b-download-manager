import {
  SettingsGlyph,
  type SettingsGlyphKind,
} from "@/components/icons/ui-icons";

/** The pages of the options window, in the order IDM lists them. */
export const SETTINGS_PAGES = [
  { id: "general", label: "General", glyph: "general" },
  { id: "file-types", label: "File types", glyph: "file-types" },
  { id: "save-to", label: "Save to", glyph: "save-to" },
  { id: "downloads", label: "Downloads", glyph: "downloads" },
  { id: "connection", label: "Connection", glyph: "connection" },
  { id: "proxy", label: "Proxy / Socks", glyph: "proxy" },
  { id: "sites", label: "Sites Logins", glyph: "sites" },
  { id: "dial-up", label: "Dial Up / VPN", glyph: "dial-up" },
  { id: "sounds", label: "Sounds", glyph: "sounds" },
] as const satisfies readonly {
  id: string;
  label: string;
  glyph: SettingsGlyphKind;
}[];

export type SettingsPageId = (typeof SETTINGS_PAGES)[number]["id"];

/** The pages this build can open. The rest are shown, but greyed out. */
export const BUILT_PAGES: SettingsPageId[] = [
  "general",
  "save-to",
  "downloads",
  "connection",
  "proxy",
];

interface SettingsNavProps {
  page: SettingsPageId;
  onSelect: (page: SettingsPageId) => void;
}

/**
 * The pages IDM puts in its configuration window. The pages this build has
 * open; the rest are shown disabled so the shell is honest about what it
 * can reach.
 */
export function SettingsNav({ page, onSelect }: SettingsNavProps) {
  return (
    <nav
      aria-label="Settings pages"
      className="hidden w-44 shrink-0 flex-col gap-0.5 border-r border-chrome-line bg-chrome-panel p-2 sm:flex"
    >
      <p className="px-2 pt-1 pb-2 text-chrome-micro font-medium tracking-wider text-chrome-muted uppercase">
        Configuration
      </p>
      {SETTINGS_PAGES.map(({ id, label, glyph }) => {
        const built = BUILT_PAGES.includes(id);
        const active = id === page;
        return (
          <button
            key={id}
            type="button"
            disabled={!built}
            aria-current={active ? "page" : undefined}
            title={built ? undefined : "Not part of this build yet"}
            onClick={() => onSelect(id)}
            className={
              "flex h-8 items-center gap-2 rounded-chrome px-2 text-left text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none " +
              (active
                ? "bg-chrome-menu-active text-chrome-title-text"
                : built
                  ? "text-chrome-menu-text hover:bg-chrome-menu-hover"
                  : "cursor-default text-chrome-muted/60")
            }
          >
            <SettingsGlyph
              kind={glyph}
              className={
                "size-4 shrink-0 " + (active ? "text-chrome-focus" : "")
              }
            />
            <span className="truncate">{label}</span>
          </button>
        );
      })}
    </nav>
  );
}
