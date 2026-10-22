import { Field, TextInput } from "@/components/settings/fields";
import { Section, SettingsPage } from "@/components/settings/settings-page";
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
    <SettingsPage
      title="Save to"
      description="Where the files are saved."
    >
      <Section title="Default folder">
        <Field
          label="Save the file to"
          htmlFor="save-to-folder"
          hint="Leave it empty to use the Downloads folder of the system. The add dialog starts from this folder and remembers the last one you typed."
        >
          <TextInput
            id="save-to-folder"
            value={settings.defaultFolder}
            placeholder="C:\\Users\\you\\Downloads"
            onChange={(value) => onChange("defaultFolder", value)}
          />
        </Field>
      </Section>

      <Section title="Per category" planned>
        <p className="text-xs text-chrome-muted">
          IDM keeps a folder for every category. The tree in the main window
          shows them as soon as they can be edited.
        </p>
      </Section>
    </SettingsPage>
  );
}
