import { TextInput } from "@/components/settings/fields";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import type { Settings } from "@/lib/settings";

interface ProxySettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Proxy / Socks page. The engine already speaks both kinds of proxy;
 * this page is where the address is written down.
 */
export function ProxySettings({ settings, onChange }: ProxySettingsProps) {
  return (
    <SettingsPage
      title="Proxy / Socks"
      description="The proxy the downloads go through."
    >
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

      <Section title="What is not proxied">
        <p className="py-2.5 text-xs leading-5 text-chrome-muted">
          Every download goes through the proxy above. IDM keeps a list of
          addresses that skip it; that list is not part of this build yet.
        </p>
      </Section>
    </SettingsPage>
  );
}
