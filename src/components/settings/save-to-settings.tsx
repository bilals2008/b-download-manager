import { TextInput } from "@/components/settings/fields";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import type { Settings } from "@/lib/settings";

interface SaveToSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Save to page: where a file lands when the user does not pick a folder
 * in the add dialog.
 */
export function SaveToSettings({ settings, onChange }: SaveToSettingsProps) {
  return (
    <SettingsPage title="Save to" description="Where the files are saved.">
      <Section title="Default folder">
        <SettingRow
          stacked
          label="Save the file to"
          htmlFor="save-to-folder"
          hint="Empty uses the Downloads folder of Windows."
        >
          <TextInput
            id="save-to-folder"
            mono
            value={settings.defaultFolder}
            placeholder="C:\Users\you\Downloads"
            onChange={(value) => onChange("defaultFolder", value)}
          />
        </SettingRow>
      </Section>

      <Section title="Per category" planned>
        <p className="py-2.5 text-xs leading-5 text-chrome-muted">
          IDM keeps a folder for every category. The tree in the main window
          shows them as soon as they can be edited.
        </p>
      </Section>
    </SettingsPage>
  );
}
