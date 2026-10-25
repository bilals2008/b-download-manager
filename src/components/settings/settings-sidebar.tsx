import { SettingsGlyph } from "@/components/icons/ui-icons";
import {
  SETTINGS_GROUPS,
  type SettingsPageId,
} from "@/components/settings/settings-nav";
import { cn } from "@/lib/utils";

interface SettingsSidebarProps {
  page: SettingsPageId;
  onSelect: (page: SettingsPageId) => void;
}

/**
 * The pages as a compact grouped list. A button rather than a tab: the pages
 * are destinations of one window, and the selected one is announced with
 * `aria-current` as well as shown by the filled row.
 */
export function SettingsSidebar({ page, onSelect }: SettingsSidebarProps) {
  return (
    <nav
      aria-label="Settings pages"
      className="w-44 shrink-0 overflow-y-auto border-r border-chrome-line bg-chrome-canvas px-2 py-3"
    >
      {SETTINGS_GROUPS.map((group) => (
        <div key={group.label} className="pb-3 last:pb-0">
          <p className="px-2 pb-1 text-chrome-micro font-medium text-chrome-muted">
            {group.label}
          </p>
          <ul className="flex flex-col gap-0.5">
            {group.pages.map(({ id, label, glyph }) => {
              const active = page === id;
              return (
                <li key={id}>
                  <button
                    type="button"
                    aria-current={active ? "page" : undefined}
                    onClick={() => onSelect(id)}
                    className={cn(
                      "flex w-full items-center gap-2 rounded-chrome px-2 py-1.5 text-left text-xs transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none",
                      active
                        ? "bg-chrome-menu-active text-chrome-title-text"
                        : "text-chrome-menu-text hover:bg-chrome-menu-hover",
                    )}
                  >
                    <SettingsGlyph
                      kind={glyph}
                      className={cn(
                        "size-4 shrink-0",
                        active ? "text-chrome-focus" : "text-chrome-muted",
                      )}
                    />
                    <span className="truncate">{label}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </nav>
  );
}
