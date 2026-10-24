import { NumberInput, TextInput } from "@/components/settings/fields";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { MAX_CONNECTIONS, type Settings } from "@/lib/settings";

interface DownloadsSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Downloads page: where a file lands, how many connections it gets, and
 * what happens when it finishes.
 */
export function DownloadsSettings({ settings, onChange }: DownloadsSettingsProps) {
  return (
    <SettingsPage
      title="Downloads"
      description="Where files are saved and how a download runs."
    >
      <Section title="Save to">
        <SettingRow
          stacked
          label="Default folder"
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

      <Section title="Connections">
        <SettingRow
          label="Default number of connections"
          htmlFor="download-connections"
          hint="One connection per 10 KB of the file, up to this number."
        >
          <NumberInput
            id="download-connections"
            value={settings.defaultConnections}
            min={1}
            max={MAX_CONNECTIONS}
            suffix="connections"
            onChange={(value) => onChange("defaultConnections", value)}
          />
        </SettingRow>
      </Section>

      <Section title="When a download finishes">
        <SettingRow
          label="Show download complete dialog"
          hint="A small window when a download finishes."
        >
          <Switch
            label="Show download complete dialog"
            checked={settings.showCompleteDialog}
            onChange={(checked) => onChange("showCompleteDialog", checked)}
          />
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
