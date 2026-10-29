import { useEffect, useState } from "react";

import { TextInput } from "@/components/settings/fields";
import { SettingsButton } from "@/components/settings/settings-button";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Switch } from "@/components/ui/switch";
import { categoryFolderProblem } from "@/lib/categories";
import { formatSize } from "@/lib/downloads";
import { describeError, isDesktop } from "@/lib/errors";
import { chooseFolder, folderStatus, type FolderStatus } from "@/lib/ipc";
import { clearLastFolder, type Settings } from "@/lib/settings";

interface SaveLocationsSettingsProps {
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

/**
 * What the engine says about a folder, once typing settles on it.
 *
 * A folder that does not exist yet is allowed: it is created with the first
 * download that needs it.
 */
function useFolderStatus(
  folder: string,
  checkWhenBlank = true,
): {
  status: FolderStatus | null;
  checking: boolean;
} {
  const [status, setStatus] = useState<FolderStatus | null>(null);
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    // A blank field can mean anything. The default folder falls back to the
    // Downloads folder of the system, which is worth showing; the unfinished
    // one falls back to a folder of the application's own, which the note
    // below already describes.
    if (!isDesktop() || (!checkWhenBlank && !folder.trim())) {
      setStatus(null);
      setChecking(false);
      return;
    }
    let active = true;
    setChecking(true);
    const timer = setTimeout(() => {
      folderStatus(folder)
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
  }, [folder, checkWhenBlank]);

  return { status, checking };
}

/** The line under a folder field: what the engine found, or nothing yet. */
function statusLine(
  folder: string,
  status: FolderStatus | null,
  checking: boolean,
  blankNote: string,
): string {
  if (checking) {
    return "Checking the folder…";
  }
  if (status?.error) {
    return status.error;
  }
  if (status) {
    return `${status.path}${status.freeBytes === null ? "" : ` · ${formatSize(status.freeBytes)} free`}`;
  }
  return folder.trim() ? "The folder could not be checked." : blankNote;
}

/** One folder field with a Browse button and the engine's answer below it. */
function FolderField({
  id,
  value,
  placeholder,
  status,
  checking,
  blankNote,
  invalid,
  onChange,
}: {
  id: string;
  value: string;
  placeholder: string;
  status: FolderStatus | null;
  checking: boolean;
  blankNote: string;
  invalid: boolean;
  onChange: (value: string) => void;
}) {
  const [browseError, setBrowseError] = useState<string | null>(null);

  async function browse() {
    if (!isDesktop()) {
      setBrowseError("Folders can only be chosen in the desktop app.");
      return;
    }
    try {
      const picked = await chooseFolder(value);
      if (picked) {
        setBrowseError(null);
        onChange(picked);
      }
    } catch (failure) {
      setBrowseError(describeError(failure));
    }
  }

  const refusing = Boolean(status?.exists && !status.writable);

  return (
    <div className="flex items-start gap-2">
      <div className="min-w-0 flex-1">
        <TextInput
          id={id}
          mono
          value={value}
          placeholder={placeholder}
          invalid={invalid || refusing}
          onChange={onChange}
        />
        <p
          aria-live="polite"
          className={
            refusing || invalid
              ? "mt-1.5 break-all text-chrome-micro text-chrome-error"
              : "mt-1.5 break-all text-chrome-micro text-chrome-muted"
          }
        >
          {statusLine(value, status, checking, blankNote)}
        </p>
        {browseError ? (
          <p role="alert" className="mt-1 text-chrome-micro text-chrome-error">
            {browseError}
          </p>
        ) : null}
      </div>
      <SettingsButton onClick={() => void browse()}>Browse…</SettingsButton>
    </div>
  );
}

/**
 * The Save locations page: where downloads land, where the unfinished ones
 * wait while they are still being written, whether the last folder is
 * remembered, and what the engine says about each chosen folder before a file
 * is written to it.
 */
export function SaveLocationsSettings({
  settings,
  onChange,
}: SaveLocationsSettingsProps) {
  const save = useFolderStatus(settings.defaultFolder);
  const unfinished = useFolderStatus(settings.unfinishedFolder, false);
  const [forgotten, setForgotten] = useState(false);

  function forget() {
    clearLastFolder();
    setForgotten(true);
  }

  const unfinishedProblem = categoryFolderProblem(settings.unfinishedFolder);

  return (
    <SettingsPage
      title="Save locations"
      description="Where files are saved."
    >
      <Section title="Default folder">
        <SettingRow
          stacked
          label="Save new downloads to"
          htmlFor="save-to-folder"
          hint="Blank uses the Windows Downloads folder."
        >
          <FolderField
            id="save-to-folder"
            value={settings.defaultFolder}
            placeholder="Windows Downloads folder"
            status={save.status}
            checking={save.checking}
            blankNote="The folder could not be checked."
            invalid={false}
            onChange={(value) => {
              setForgotten(false);
              onChange("defaultFolder", value);
            }}
          />
        </SettingRow>
      </Section>

      <Section title="Unfinished downloads">
        <SettingRow
          stacked
          label="Keep unfinished downloads in"
          htmlFor="unfinished-folder"
          hint="Incomplete files and their resume state wait here, out of your finished folders, and are moved into place when a download completes."
        >
          <FolderField
            id="unfinished-folder"
            value={settings.unfinishedFolder}
            placeholder="A folder inside this application's data folder"
            status={unfinished.status}
            checking={unfinished.checking}
            blankNote="Blank uses a folder inside this application's data folder."
            invalid={Boolean(unfinishedProblem)}
            onChange={(value) => onChange("unfinishedFolder", value)}
          />
          {unfinishedProblem ? (
            <p role="alert" className="mt-1 text-chrome-micro text-chrome-error">
              {unfinishedProblem}
            </p>
          ) : null}
        </SettingRow>
      </Section>

      <Section title="Remembering">
        <SettingRow
          label="Start a new download in the last folder used"
          hint="The last folder you really downloaded to."
        >
          <Switch
            label="Start a new download in the last folder used"
            checked={settings.rememberLastFolder}
            onChange={(checked) => onChange("rememberLastFolder", checked)}
          />
        </SettingRow>
        <SettingRow
          label="Forget the remembered folder"
          hint="Starts using the default again."
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
