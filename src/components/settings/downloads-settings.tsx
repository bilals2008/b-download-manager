import { NumberInput } from "@/components/settings/fields";
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
 * The Downloads page: how many connections a file gets, and what happens when
 * one finishes.
 */
export function DownloadsSettings({ settings, onChange }: DownloadsSettingsProps) {
  return (
    <SettingsPage
      title="Downloads"
      description="How a new download starts, and how many connections it gets."
    >
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

      <Section title="The list">
        <p className="py-2.5 text-xs leading-5 text-chrome-muted">
          The list keeps every download that was started, and a download that
          was stopped can be started again from where it stopped.
        </p>
      </Section>
    </SettingsPage>
  );
}
