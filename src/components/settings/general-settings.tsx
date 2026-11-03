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
    <SettingsPage page="general">
      <Section title="Startup">
        <SettingRow
          settingId="setting-launch-startup"
          label={`Launch ${APP_NAME} on startup`}
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
          settingId="setting-clipboard"
          label="Watch the clipboard for download links"
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
          settingId="setting-minimize-tray"
          label="Minimize the window to the tray"
        >
          <Switch
            label="Minimize the window to the tray"
            checked={settings.minimizeToTray}
            onChange={(checked) => onChange("minimizeToTray", checked)}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-close-tray"
          label="Keep running in the tray when the window is closed"
        >
          <Switch
            label="Keep running in the tray when the window is closed"
            checked={settings.closeToTray}
            onChange={(checked) => onChange("closeToTray", checked)}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-tray-icon"
          label="Show the tray icon"
        >
          <Switch
            label="Show the tray icon"
            checked={!preferences.hidden}
            onChange={(shown) => tray.onHidden(!shown)}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-tray-dark-mode"
          label="Dark mode support"
        >
          <Switch
            label="Dark mode support"
            checked={preferences.darkMode}
            onChange={(checked) => tray.onDarkMode(checked)}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-url-list"
          label="Customize URL List"
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
