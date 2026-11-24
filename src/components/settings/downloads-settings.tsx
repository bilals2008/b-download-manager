import { NumberInput } from "@/components/settings/fields";
import { ScheduleSettings } from "@/components/settings/schedule-settings";
import { SettingsButton } from "@/components/settings/settings-button";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { COMPLETION_SOUNDS, playCompletionSound } from "@/lib/completion-sound";
import { ListBox } from "@/components/ui/listbox";
import { Switch } from "@/components/ui/switch";
import {
  DUPLICATE_POLICIES,
  MAX_CONNECTIONS,
  type DuplicatePolicy,
  type Settings,
} from "@/lib/settings";

interface DownloadsSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Downloads page: how a new download runs and what happens when it
 * finishes.
 */
export function DownloadsSettings({ settings, onChange }: DownloadsSettingsProps) {
  return (
    <SettingsPage page="downloads">
      <Section title="Connections">
        <SettingRow
          settingId="setting-default-connections"
          label="Default number of connections"
          htmlFor="download-connections"
        >
          <NumberInput
            id="download-connections"
            value={settings.defaultConnections}
            min={1}
            max={MAX_CONNECTIONS}
            suffix="connections"
            onChange={(value) => onChange("defaultConnections", value)}
          />
        </SettingRow>
      </Section>

      <Section title="Existing files">
        <SettingRow
          settingId="setting-duplicate-policy"
          label="When the file already exists"
        >
          <div className="w-52">
            <ListBox
              label="When the file already exists"
              value={settings.duplicatePolicy}
              onChange={(value) =>
                onChange("duplicatePolicy", value as DuplicatePolicy)
              }
              options={DUPLICATE_POLICIES}
            />
          </div>
        </SettingRow>
      </Section>

      <Section title="While a download runs">
        <SettingRow
          settingId="setting-progress-window"
          label="Open the progress window when a download starts"
        >
          <Switch
            label="Open the progress window when a download starts"
            checked={settings.showProgressOnStart}
            onChange={(checked) => onChange("showProgressOnStart", checked)}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-video-window"
          label="Open the quality window when a video download starts"
        >
          <Switch
            label="Open the quality window when a video download starts"
            checked={settings.showVideoWindowOnStart}
            onChange={(checked) => onChange("showVideoWindowOnStart", checked)}
          />
        </SettingRow>
      </Section>

      <Section title="When a download finishes">
        <SettingRow
          settingId="setting-complete-dialog"
          label="Show the download complete dialog"
        >
          <Switch
            label="Show the download complete dialog"
            checked={settings.showCompleteDialog}
            onChange={(checked) => onChange("showCompleteDialog", checked)}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-extract-archive"
          label="Unpack a finished archive beside itself"
        >
          <Switch
            label="Unpack a finished archive beside itself"
            checked={settings.extractAfterDone}
            onChange={(checked) => onChange("extractAfterDone", checked)}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-delete-archive"
          label="Remove the archive after a clean unpack"
        >
          <Switch
            label="Remove the archive after a clean unpack"
            checked={settings.deleteArchiveAfterExtract}
            onChange={(checked) => onChange("deleteArchiveAfterExtract", checked)}
          />
        </SettingRow>
        <SettingRow
          settingId="setting-completion-sound"
          label="Play a sound when a download finishes"
        >
          <Switch
            label="Play a sound when a download finishes"
            checked={settings.completionSound}
            onChange={(checked) => onChange("completionSound", checked)}
          />
        </SettingRow>
        <SettingRow settingId="setting-completion-sound-name" label="The sound">
          <div className="flex items-center gap-2">
            <div className="w-36">
              <ListBox
                label="The sound"
                value={settings.completionSoundName}
                onChange={(value) => onChange("completionSoundName", value)}
                options={COMPLETION_SOUNDS}
                disabled={!settings.completionSound}
              />
            </div>
            <SettingsButton
              disabled={!settings.completionSound}
              onClick={() => {
                playCompletionSound(
                  settings.completionSoundName,
                  settings.completionSoundVolume,
                );
              }}
            >
              Play
            </SettingsButton>
          </div>
        </SettingRow>
        <SettingRow
          settingId="setting-completion-sound-volume"
          label="Sound volume"
          htmlFor="completion-sound-volume"
        >
          <NumberInput
            id="completion-sound-volume"
            value={settings.completionSoundVolume}
            min={0}
            max={100}
            suffix="%"
            disabled={!settings.completionSound}
            onChange={(value) => onChange("completionSoundVolume", value)}
          />
        </SettingRow>
      </Section>

      <ScheduleSettings />
    </SettingsPage>
  );
}
