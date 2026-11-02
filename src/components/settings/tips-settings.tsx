
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { SettingsButton } from "@/components/settings/settings-button";
import { Switch } from "@/components/ui/switch";
import type { Settings } from "@/lib/settings";
import { resetTipCursor } from "@/lib/tips";

interface TipsSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * Tips: the two choices this window owns. The tips themselves are read in the
 * popup, so the page does not repeat them.
 */
export function TipsSettings({ settings, onChange }: TipsSettingsProps) {
  function forgetPosition() {
    resetTipCursor();
  }

  return (
    <SettingsPage title="Tips">
      <Section title="Startup">
        <SettingRow
          settingId="setting-tips-startup"
          label="Show a tip on startup"
        >
          <Switch
            label="Show a tip on startup"
            checked={settings.tipsOnStartup}
            onChange={(checked) => onChange("tipsOnStartup", checked)}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-tip-order"
          label="Start from the first tip"
        >
          <SettingsButton onClick={forgetPosition}>
            Start from the first tip
          </SettingsButton>
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}

