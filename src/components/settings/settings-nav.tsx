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

const OPEN_PAGES = SETTINGS_PAGES.filter(({ id }) => BUILT_PAGES.includes(id));
const PLANNED_PAGES = SETTINGS_PAGES.filter(
  ({ id }) => !BUILT_PAGES.includes(id),
);

const ITEM =
  "flex h-8 w-full items-center gap-2.5 rounded-chrome px-2 text-left text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none";

interface SettingsNavProps {
  page: SettingsPageId;
  onSelect: (page: SettingsPageId) => void;
}

/**
 * The list of pages, as a column of the window rather than a card inside it.
 * The pages this build has are at the top; the ones it does not open yet are
 * kept together at the foot under their own caption, so the column is honest
 * without making the working pages look unfinished.
 */
export function SettingsNav({ page, onSelect }: SettingsNavProps) {
  return (
    <nav
      aria-label="Settings pages"
      className="scrollbar-none flex w-44 shrink-0 flex-col overflow-y-auto border-r border-chrome-line bg-chrome-canvas p-2"
    >
      <div className="flex flex-col gap-0.5">
        {OPEN_PAGES.map(({ id, label, glyph }) => {
          const active = id === page;
          return (
            <button
              key={id}
              type="button"
              aria-current={active ? "page" : undefined}
              onClick={() => onSelect(id)}
              className={
                ITEM +
                (active
                  ? " bg-chrome-menu-active text-chrome-title-text"
                  : " text-chrome-menu-text hover:bg-chrome-menu-hover")
              }
            >
              <SettingsGlyph
                kind={glyph}
                className={
                  "size-4 shrink-0 " +
                  (active ? "text-chrome-focus" : "text-chrome-muted")
                }
              />
              <span className="truncate">{label}</span>
            </button>
          );
        })}
      </div>

      <p className="mt-auto px-2 pt-6 pb-1.5 text-chrome-micro text-chrome-muted">
        Not in this build yet
      </p>
      <div className="flex flex-col gap-0.5">
        {PLANNED_PAGES.map(({ id, label, glyph }) => (
          <button
            key={id}
            type="button"
            disabled
            title="Not part of this build yet"
            className={ITEM + " cursor-default text-chrome-muted/50"}
          >
            <SettingsGlyph
              kind={glyph}
              className="size-4 shrink-0 text-chrome-muted/50"
            />
            <span className="truncate">{label}</span>
          </button>
        ))}
      </div>
    </nav>
  );
}
