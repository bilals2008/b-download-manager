import { Checkbox } from "@/components/ui/checkbox";
import { NumberInput, Field } from "@/components/settings/fields";
import { Section, SettingsPage } from "@/components/settings/settings-page";
import { MAX_CONNECTIONS, type Settings } from "@/lib/settings";

interface DownloadsSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Downloads page: how many connections a file gets, and how the list is
 * filled.
 */
export function DownloadsSettings({ settings, onChange }: DownloadsSettingsProps) {
  return (
    <SettingsPage
      title="Downloads"
      description="How a new download starts, and how many connections it gets."
    >
      <Section title="Connections">
        <Field
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
        </Field>
      </Section>

      <Section title="The list">
        <p className="text-xs text-chrome-muted">
          The list keeps every download that was started, and a download that
          was stopped can be started again from where it stopped.
        </p>
      </Section>

      <Section title="When a download finishes">
        <Checkbox
          id="download-complete-dialog"
          label="Show download complete dialog"
          hint="Show a small window when a download finishes."
          checked={settings.showCompleteDialog}
          onChange={(event) =>
            onChange("showCompleteDialog", event.target.checked)
          }
        />
      </Section>
    </SettingsPage>
  );
}
