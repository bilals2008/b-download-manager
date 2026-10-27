import { NumberInput } from "@/components/settings/fields";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { MAX_CONNECTIONS, type Settings } from "@/lib/settings";

interface ConnectionsSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Connections page: the ceiling on open connections and the cap on the
 * bandwidth they may use together.
 */
export function ConnectionsSettings({
  settings,
  onChange,
}: ConnectionsSettingsProps) {
  // Zero means the cap has never been set; the field shows a usable starting
  // point rather than an empty box, because turning the cap on fills it in.
  const limitKb = Math.round(
    (settings.speedLimitBytes || 1024 * 1024) / 1024,
  );

  return (
    <SettingsPage
      title="Connections"
      description="Total connections and speed."
    >
      <Section title="Connections">
        <SettingRow
          label="Maximum number of connections"
          htmlFor="connection-max"
          hint="Across all downloads at once."
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
          hint="Shared by all downloads."
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
          hint="Kilobytes per second, all downloads."
        >
          <NumberInput
            id="connection-speed"
            value={limitKb}
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
