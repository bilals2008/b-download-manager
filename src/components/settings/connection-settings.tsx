import { NumberInput, Field } from "@/components/settings/fields";
import { Section, SettingsPage } from "@/components/settings/settings-page";
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
        <Field
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
        </Field>
      </Section>

      <Section title="Speed limit">
        <div className="flex flex-col gap-3">
          <label className="flex items-center gap-2 text-xs text-chrome-title-text">
            <input
              type="checkbox"
              className="size-3.5 accent-chrome-focus"
              checked={settings.speedLimitEnabled}
              onChange={(event) => onChange("speedLimitEnabled", event.target.checked)}
            />
            Limit the speed of all downloads
          </label>
          <Field
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
              onChange={(value) => onChange("speedLimitBytes", value * 1024)}
            />
          </Field>
        </div>
      </Section>
    </SettingsPage>
  );
}
