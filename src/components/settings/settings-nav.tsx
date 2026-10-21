import {
  ArrowDownToLine,
  Cable,
  FileType,
  FolderDown,
  KeyRound,
  Phone,
  Shield,
  SlidersHorizontal,
  Volume2,
  type LucideIcon,
} from "lucide-react";

interface SettingsPage {
  id: string;
  label: string;
  icon: LucideIcon;
}

/**
 * The pages IDM puts in its configuration window. Only General is built; the
 * rest are shown disabled so the shell is honest about what it can reach.
 */
const PAGES: SettingsPage[] = [
  { id: "general", label: "General", icon: SlidersHorizontal },
  { id: "file-types", label: "File types", icon: FileType },
  { id: "save-to", label: "Save to", icon: FolderDown },
  { id: "downloads", label: "Downloads", icon: ArrowDownToLine },
  { id: "connection", label: "Connection", icon: Cable },
  { id: "proxy", label: "Proxy / Socks", icon: Shield },
  { id: "sites", label: "Sites Logins", icon: KeyRound },
  { id: "dial-up", label: "Dial Up / VPN", icon: Phone },
  { id: "sounds", label: "Sounds", icon: Volume2 },
];

export function SettingsNav() {
  return (
    <nav
      aria-label="Settings pages"
      className="hidden w-44 shrink-0 flex-col gap-0.5 border-r border-chrome-line bg-chrome-panel p-2 sm:flex"
    >
      <p className="px-2 pt-1 pb-2 text-chrome-micro font-medium tracking-wider text-chrome-muted uppercase">
        Configuration
      </p>
      {PAGES.map(({ id, label, icon: Icon }) => {
        const active = id === "general";
        return (
          <button
            key={id}
            type="button"
            disabled={!active}
            aria-current={active ? "page" : undefined}
            title={active ? undefined : "Not part of this build yet"}
            className={
              "flex h-8 items-center gap-2 rounded-chrome px-2 text-left text-xs transition-colors " +
              (active
                ? "bg-chrome-menu-active text-chrome-title-text"
                : "cursor-default text-chrome-muted/60")
            }
          >
            <Icon
              aria-hidden="true"
              className={
                "size-3.5 shrink-0 " + (active ? "text-chrome-focus" : "")
              }
            />
            <span className="truncate">{label}</span>
          </button>
        );
      })}
    </nav>
  );
}
