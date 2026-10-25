import { NumberInput } from "@/components/settings/fields";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { ListBox } from "@/components/ui/listbox";
import { Switch } from "@/components/ui/switch";
import {
  DUPLICATE_POLICIES,
  MAX_CONNECTIONS,
  type DuplicatePolicy,
  type Settings,
} from "@/lib/settings";

interface DownloadsSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Downloads page: how a new download runs and what happens when it
 * finishes.
 */
export function DownloadsSettings({ settings, onChange }: DownloadsSettingsProps) {
  return (
    <SettingsPage
      title="Downloads"
      description="How a new download runs and what is shown while it does."
    >
      <Section title="Connections">
        <SettingRow
          label="Default number of connections"
          htmlFor="download-connections"
          hint="Used for a new download. One connection per 10 KB of the file, up to this number."
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

      <Section title="Existing files">
        <SettingRow
          label="When a file of the same name is already there"
          hint="A new download follows this answer. Ask opens the File Exists dialog."
        >
          <div className="w-52">
            <ListBox
              label="When a file of the same name is already there"
              value={settings.duplicatePolicy}
              onChange={(value) =>
                onChange("duplicatePolicy", value as DuplicatePolicy)
              }
              options={DUPLICATE_POLICIES}
            />
          </div>
        </SettingRow>
      </Section>

      <Section title="While a download runs">
        <SettingRow
          label="Open the progress window when a download starts"
          hint="Applies to a download started here and to one handed over by the browser."
        >
          <Switch
            label="Open the progress window when a download starts"
            checked={settings.showProgressOnStart}
            onChange={(checked) => onChange("showProgressOnStart", checked)}
          />
        </SettingRow>
      </Section>

      <Section title="When a download finishes">
        <SettingRow
          label="Show the download complete dialog"
          hint="A small window with the file and the folder when a download finishes."
        >
          <Switch
            label="Show the download complete dialog"
            checked={settings.showCompleteDialog}
            onChange={(checked) => onChange("showCompleteDialog", checked)}
          />
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
