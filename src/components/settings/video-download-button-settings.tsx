import { Section, SettingRow, SettingsPage } from "@/components/settings/settings-page";
import { ListBox } from "@/components/ui/listbox";
import { Switch } from "@/components/ui/switch";
import { TextInput } from "@/components/settings/fields";
import { type Settings } from "@/lib/settings";

interface VideoDownloadButtonSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

const BUTTON_STYLES: { value: Settings["videoButtonStyle"]; label: string }[] = [
  { value: "icon", label: "Icon only" },
  { value: "text", label: "Text only" },
  { value: "both", label: "Icon + text" },
];

/**
 * The floating download button the browser extension draws on a video.
 *
 * These preferences are stored with the rest of the settings and broadcast to
 * every window; the extension reads them from the loopback bridge so an open
 * tab picks up a change without reloading.
 */
export function VideoDownloadButtonSettings({
  settings,
  onChange,
}: VideoDownloadButtonSettingsProps) {
  return (
    <SettingsPage title="Video download button">
      <Section title="Appearance">
        <SettingRow
          settingId="setting-video-button-style"
          label="Button style"
        >
          <div className="w-64">
            <ListBox
              label="Button style"
              value={settings.videoButtonStyle}
              onChange={(value) =>
                onChange(
                  "videoButtonStyle",
                  value as Settings["videoButtonStyle"],
                )
              }
              options={BUTTON_STYLES}
            />
          </div>
        </SettingRow>
        <SettingRow
          settingId="setting-video-button-label"
          label="Button label"
        >
          <TextInput
            id="video-button-label"
            value={settings.videoButtonLabel}
            placeholder="Download"
            onChange={(value) => onChange("videoButtonLabel", value)}
          />
        </SettingRow>
      </Section>

      <Section title="Behaviour">
        <SettingRow
          settingId="setting-video-button-hover"
          label="Hover to show"
        >
          <Switch
            label="Hover to show"
            checked={settings.videoButtonHover}
            onChange={(checked) => onChange("videoButtonHover", checked)}
          />
        </SettingRow>
      </Section>
    </SettingsPage>
  );
}
