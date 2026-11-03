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
 * The pages as a grouped list. A button rather than a tab: the pages are
 * destinations of one window, and the selected one is announced with
 * `aria-current` as well as shown by the tint and the accent bar.
 *
 * A long page name wraps instead of being cut off: the sidebar is a fixed
 * width, and an ellipsis in a navigation list reads as a broken label.
 */
export function SettingsSidebar({ page, onSelect }: SettingsSidebarProps) {
  return (
    <nav
      aria-label="Settings pages"
      className="w-52 shrink-0 overflow-y-auto scrollbar-none px-2.5 py-4"
    >
      {SETTINGS_GROUPS.map((group) => (
        <div key={group.label} className="pb-4 last:pb-0">
          <p className="px-2.5 pb-1.5 text-chrome-micro font-medium text-chrome-muted">
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
                      "relative flex w-full items-start gap-2.5 rounded-chrome py-1.5 pr-2 pl-3 text-left text-xs leading-5 transition-colors focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none",
                      active
                        ? "bg-chrome-menu-active text-chrome-title-text"
                        : "text-chrome-menu-text hover:bg-chrome-menu-hover",
                    )}
                  >
                    {active ? (
                      <span
                        aria-hidden="true"
                        className="absolute top-1.5 bottom-1.5 left-0 w-0.5 rounded-full bg-chrome-focus"
                      />
                    ) : null}
                    <SettingsGlyph
                      kind={glyph}
                      className={cn(
                        "mt-0.5 size-4 shrink-0",
                        active ? "text-chrome-focus" : "text-chrome-muted",
                      )}
                    />
                    <span className="min-w-0 flex-1">{label}</span>
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
