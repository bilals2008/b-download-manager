import { SettingsButton } from "@/components/settings/settings-button";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { APP_NAME } from "@/lib/brand";
import type { Settings } from "@/lib/settings";
import type { TrayPreferences } from "@/lib/tray";

interface GeneralSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  /** The tray icon choices, which the backend owns rather than this window. */
  tray: TraySettingsProps;
}

/** What the General page needs to drive the tray icon. */
export interface TraySettingsProps {
  preferences: TrayPreferences;
  onHidden: (hidden: boolean) => void;
  onDarkMode: (darkMode: boolean) => void;
  onCustomizeUrlList: () => void;
}

/**
 * The General page: how the application starts, what it watches, and what the
 * close button does.
 */
export function GeneralSettings({ settings, onChange, tray }: GeneralSettingsProps) {
  const { preferences } = tray;
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
        <SettingRow
          label="Show the tray icon"
          hint="Off leaves the window the only way back, so closing it exits."
        >
          <Switch
            label="Show the tray icon"
            checked={!preferences.hidden}
            onChange={(shown) => tray.onHidden(!shown)}
          />
        </SettingRow>
        <SettingRow
          label="Dark mode support"
          hint="The icon takes the inks of the appearance the window is in."
        >
          <Switch
            label="Dark mode support"
            checked={preferences.darkMode}
            onChange={(checked) => tray.onDarkMode(checked)}
          />
        </SettingRow>
        <SettingRow
          label="Customize URL List"
          hint="The protocols a copied address has to start with."
        >
          <SettingsButton
            aria-haspopup="dialog"
            onClick={tray.onCustomizeUrlList}
          >
            Customize URL List...
          </SettingsButton>
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
