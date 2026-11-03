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
    <SettingsPage page="downloads">
      <Section title="Connections">
        <SettingRow
          settingId="setting-default-connections"
          label="Default number of connections"
          htmlFor="download-connections"
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
          settingId="setting-duplicate-policy"
          label="When the file already exists"
        >
          <div className="w-52">
            <ListBox
              label="When the file already exists"
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
          settingId="setting-progress-window"
          label="Open the progress window when a download starts"
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
          settingId="setting-complete-dialog"
          label="Show the download complete dialog"
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
