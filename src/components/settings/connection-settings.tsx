import { NumberInput } from "@/components/settings/fields";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { MAX_CONNECTIONS, type Settings } from "@/lib/settings";

interface ConnectionSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Connection page: the cap on the connections of the whole application
 * and the bandwidth cap.
 */
export function ConnectionSettings({
  settings,
  onChange,
}: ConnectionSettingsProps) {
  const limitInKb = Math.round(settings.speedLimitBytes / 1024);

  return (
    <SettingsPage
      title="Connection"
      description="How much of the network the app may use."
    >
      <Section title="Connections">
        <SettingRow
          label="Maximum number of connections"
          htmlFor="connection-max"
          hint="Open at once, over all downloads."
        >
          <NumberInput
            id="connection-max"
            value={settings.maxConnections}
            min={1}
            max={MAX_CONNECTIONS}
            suffix="connections"
            onChange={(value) => onChange("maxConnections", value)}
          />
        </SettingRow>
      </Section>

      <Section title="Speed limit">
        <SettingRow
          label="Limit the speed of all downloads"
          hint="The cap is shared by every download in the list."
        >
          <Switch
            label="Limit the speed of all downloads"
            checked={settings.speedLimitEnabled}
            onChange={(checked) => onChange("speedLimitEnabled", checked)}
          />
        </SettingRow>
        <SettingRow
          label="Speed limit"
          htmlFor="connection-speed"
          hint="Total bytes per second, all downloads together."
        >
          <NumberInput
            id="connection-speed"
            value={limitInKb}
            min={1}
            max={1024 * 1024}
            suffix="KB/s"
            disabled={!settings.speedLimitEnabled}
            onChange={(value) => onChange("speedLimitBytes", value * 1024)}
          />
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
