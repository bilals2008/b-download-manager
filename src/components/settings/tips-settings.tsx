import { useState } from "react";

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
  const [reset, setReset] = useState(false);

  function forgetPosition() {
    resetTipCursor();
    setReset(true);
  }

  return (
    <SettingsPage
      title="Tips"
      description="What the window offers on startup."
    >
      <Section title="Startup">
        <SettingRow
          label="Show a tip on startup"
          hint="One tip per start. Help > Tip of the Day opens them any time."
        >
          <Switch
            label="Show a tip on startup"
            checked={settings.tipsOnStartup}
            onChange={(checked) => onChange("tipsOnStartup", checked)}
          />
        </SettingRow>
        <SettingRow
          label="Tip order"
          hint={
            reset
              ? "The next tip will be the first one."
              : "The next tip continues after the last one you saw."
          }
        >
          <SettingsButton onClick={forgetPosition}>
            Start from the first tip
          </SettingsButton>
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
