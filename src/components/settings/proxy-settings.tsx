import { Field, TextInput } from "@/components/settings/fields";
import { Section, SettingsPage } from "@/components/settings/settings-page";
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
        <Field
          label="Proxy address"
          htmlFor="proxy-address"
          hint="An http:// or socks5:// address with the port, for example socks5://127.0.0.1:1080. Leave it empty to go straight to the server."
        >
          <TextInput
            id="proxy-address"
            value={settings.proxyUrl}
            placeholder="socks5://127.0.0.1:1080"
            onChange={(value) => onChange("proxyUrl", value)}
          />
        </Field>
      </Section>

      <Section title="What is not proxied">
        <p className="text-xs text-chrome-muted">
          Every download goes through the proxy above. IDM keeps a list of
          addresses that skip it; that list is not part of this build yet.
        </p>
      </Section>
    </SettingsPage>
  );
}
