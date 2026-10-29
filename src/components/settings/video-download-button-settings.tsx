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
    <SettingsPage
      title="Video download button"
      description="The floating download button the extension shows on videos."
    >
      <Section title="Appearance">
        <SettingRow
          label="Button style"
          hint="Icon only, text only, or both."
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
          label="Button label"
          hint="Shown when the style includes text. Defaults to Download."
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
          label="Hover to show"
          hint="Only show the button while the pointer is on the video."
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
