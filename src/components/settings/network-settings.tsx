import { NumberInput, TextInput } from "@/components/settings/fields";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { MAX_CONNECTIONS, type Settings } from "@/lib/settings";

interface NetworkSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Network page: how much of the connection the app may use, and the proxy
 * the downloads go through.
 */
export function NetworkSettings({ settings, onChange }: NetworkSettingsProps) {
  const limitInKb = Math.round(settings.speedLimitBytes / 1024);

  return (
    <SettingsPage
      title="Network"
      description="Connections, speed limit and the proxy."
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

      <Section title="Proxy">
        <SettingRow
          stacked
          label="Proxy address"
          htmlFor="proxy-address"
          hint="For example socks5://127.0.0.1:1080. Empty connects directly."
        >
          <TextInput
            id="proxy-address"
            mono
            value={settings.proxyUrl}
            placeholder="socks5://127.0.0.1:1080"
            onChange={(value) => onChange("proxyUrl", value)}
          />
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
