import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { KeyCap } from "@/components/ui/key-cap";
import { SHORTCUT_GROUPS } from "@/lib/shortcuts";

/**
 * Keyboard shortcuts: what the main window listens for, grouped by where it
 * applies. There is no editor here, because every key is fixed in the code.
 */
export function ShortcutsSettings() {
  return (
    <SettingsPage
      title="Shortcuts"
      description="Keys that work in the main window."
    >
      {SHORTCUT_GROUPS.map((group) => (
        <Section key={group.title} title={group.title}>
          {group.rows.map((row) => (
            <SettingRow
              key={row.id}
              settingId={row.id}
              label={row.label}
              hint={row.hint}
            >
              {row.soon ? (
                <span className="text-chrome-micro text-chrome-muted">
                  Coming soon
                </span>
              ) : row.keys ? (
                <KeyCap>{row.keys}</KeyCap>
              ) : null}
            </SettingRow>
          ))}
        </Section>
      ))}
    </SettingsPage>
  );
}
