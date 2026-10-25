import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import type { Settings } from "@/lib/settings";

interface GeneralSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The General page: how the application starts, what it watches, and what the
 * close button does.
 */
export function GeneralSettings({ settings, onChange }: GeneralSettingsProps) {
  return (
    <SettingsPage
      title="General"
      description="Startup, the clipboard and the tray."
    >
      <Section title="Startup">
        <SettingRow
          label="Launch B Download Manager on startup"
          hint="The entry is made in your Windows account and can be removed here."
        >
          <Switch
            label="Launch B Download Manager on startup"
            checked={settings.launchOnStartup}
            onChange={(checked) => onChange("launchOnStartup", checked)}
          />
        </SettingRow>
      </Section>

      <Section title="Clipboard">
        <SettingRow
          label="Watch the clipboard for download links"
          hint="A copied address opens the Add URL dialog. Nothing downloads until you press Download."
        >
          <Switch
            label="Watch the clipboard for download links"
            checked={settings.clipboardCapture}
            onChange={(checked) => onChange("clipboardCapture", checked)}
          />
        </SettingRow>
      </Section>

      <Section title="The tray">
        <SettingRow
          label="Keep the application in the tray when the window is closed"
          hint="Closing the window hides it; the tray icon brings it back."
        >
          <Switch
            label="Keep the application in the tray when the window is closed"
            checked={settings.minimizeToTray}
            onChange={(checked) => onChange("minimizeToTray", checked)}
          />
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
