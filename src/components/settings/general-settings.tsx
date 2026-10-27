import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { APP_NAME } from "@/lib/brand";
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
      description="Startup, clipboard and tray."
    >
      <Section title="Startup">
        <SettingRow
          label={`Launch ${APP_NAME} on startup`}
          hint="Adds an entry to Windows startup."
        >
          <Switch
            label={`Launch ${APP_NAME} on startup`}
            checked={settings.launchOnStartup}
            onChange={(checked) => onChange("launchOnStartup", checked)}
          />
        </SettingRow>
      </Section>

      <Section title="Clipboard">
        <SettingRow
          label="Watch the clipboard for download links"
          hint="Opens the Add URL dialog first."
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
          label="Minimize the window to the tray"
          hint="The tray icon brings the window back."
        >
          <Switch
            label="Minimize the window to the tray"
            checked={settings.minimizeToTray}
            onChange={(checked) => onChange("minimizeToTray", checked)}
          />
        </SettingRow>
        <SettingRow
          label="Keep running in the tray when the window is closed"
          hint="Downloads carry on after the window closes."
        >
          <Switch
            label="Keep running in the tray when the window is closed"
            checked={settings.closeToTray}
            onChange={(checked) => onChange("closeToTray", checked)}
          />
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
