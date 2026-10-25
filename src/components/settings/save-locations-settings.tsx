import { useEffect, useState } from "react";

import { TextInput } from "@/components/settings/fields";
import { SettingsButton } from "@/components/settings/settings-button";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { formatSize } from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import { chooseFolder, folderStatus, type FolderStatus } from "@/lib/ipc";
import { clearLastFolder, type Settings } from "@/lib/settings";

interface SaveLocationsSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * The Save locations page: where downloads land, whether the last folder is
 * remembered, and what the engine says about the chosen folder before a file
 * is written to it.
 */
export function SaveLocationsSettings({
  settings,
  onChange,
}: SaveLocationsSettingsProps) {
  const [status, setStatus] = useState<FolderStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [browseError, setBrowseError] = useState<string | null>(null);
  const [forgotten, setForgotten] = useState(false);

  // Ask the engine about the folder once typing settles. A folder that does
  // not exist yet is allowed: it is created with the first download.
  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let active = true;
    setChecking(true);
    const timer = setTimeout(() => {
      folderStatus(settings.defaultFolder)
        .then((answer) => {
          if (active) {
            setStatus(answer);
          }
        })
        .catch(() => {
          if (active) {
            setStatus(null);
          }
        })
        .finally(() => {
          if (active) {
            setChecking(false);
          }
        });
    }, 300);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [settings.defaultFolder]);

  async function browse() {
    if (!isDesktop()) {
      setBrowseError("The folder picker is only available in the desktop app.");
      return;
    }
    try {
      const picked = await chooseFolder(settings.defaultFolder);
      if (picked) {
        setBrowseError(null);
        onChange("defaultFolder", picked);
      }
    } catch (failure) {
      setBrowseError(describeError(failure));
    }
  }

  function forget() {
    clearLastFolder();
    setForgotten(true);
  }

  const statusText = checking
    ? "Checking the folder…"
    : status?.error
      ? status.error
      : status
        ? `${status.path}${status.freeBytes === null ? "" : ` · ${formatSize(status.freeBytes)} free`}`
        : "The folder could not be checked in this window.";

  return (
    <SettingsPage
      title="Save locations"
      description="Where a new download is saved, and what is remembered between them."
    >
      <Section title="Default folder">
        <SettingRow
          stacked
          label="Save new downloads to"
          htmlFor="save-to-folder"
          hint="Empty uses the Downloads folder of Windows."
        >
          <div className="flex items-start gap-2">
            <div className="min-w-0 flex-1">
              <TextInput
                id="save-to-folder"
                mono
                value={settings.defaultFolder}
                placeholder="The Downloads folder of Windows"
                invalid={Boolean(status?.exists && !status.writable)}
                onChange={(value) => {
                  setForgotten(false);
                  onChange("defaultFolder", value);
                }}
              />
            </div>
            <SettingsButton onClick={() => void browse()}>Browse…</SettingsButton>
          </div>
          <p
            aria-live="polite"
            className={
              status?.error && status.exists && !status.writable
                ? "mt-1.5 break-all text-chrome-micro text-chrome-error"
                : "mt-1.5 break-all text-chrome-micro text-chrome-muted"
            }
          >
            {statusText}
          </p>
          {browseError ? (
            <p role="alert" className="mt-1 text-chrome-micro text-chrome-error">
              {browseError}
            </p>
          ) : null}
        </SettingRow>
      </Section>

      <Section title="Remembering">
        <SettingRow
          label="Open a new download on the folder of the last one"
          hint="Only a download that really started replaces the remembered folder."
        >
          <Switch
            label="Open a new download on the folder of the last one"
            checked={settings.rememberLastFolder}
            onChange={(checked) => onChange("rememberLastFolder", checked)}
          />
        </SettingRow>
        <SettingRow
          label="Forget the remembered folder"
          hint="The default above is used again. No download or saved file is touched."
        >
          <SettingsButton onClick={forget} disabled={forgotten}>
            {forgotten ? "Forgotten" : "Forget"}
          </SettingsButton>
        </SettingRow>
        {forgotten ? (
          <p role="status" className="text-chrome-micro text-chrome-muted">
            The remembered folder was cleared.
          </p>
        ) : null}
      </Section>
    </SettingsPage>
  );
}
