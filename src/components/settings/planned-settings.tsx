import { SettingsGlyph } from "@/components/icons/ui-icons";
import { SettingsPage } from "@/components/settings/settings-page";
import { PLANNED_PAGES } from "@/components/settings/settings-nav";

/**
 * Every page this build does not have yet, gathered in one place. Nothing here
 * can be changed: the list tells a reader what is coming and keeps the pages
 * that do work free of greyed-out controls.
 */
export function PlannedSettings() {
  return (
    <SettingsPage
      title="Planned"
      description="Pages that are not part of this build yet."
    >
      <ul className="flex flex-col">
        {PLANNED_PAGES.map(({ id, label, glyph, note }) => (
          <li key={id} className="flex items-start gap-3 py-2.5">
            <SettingsGlyph
              kind={glyph}
              className="mt-0.5 size-4 shrink-0 text-chrome-muted"
            />
            <div className="min-w-0">
              <p className="text-xs leading-5 text-chrome-menu-text">{label}</p>
              <p className="text-chrome-micro leading-4 text-chrome-muted">
                {note}
              </p>
            </div>
          </li>
        ))}
      </ul>
    </SettingsPage>
  );
}
