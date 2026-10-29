import { useEffect, useState } from "react";

import { SettingsButton } from "@/components/settings/settings-button";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Tooltip } from "@/components/ui/tooltip";
import { describeError, isDesktop } from "@/lib/errors";
import { appPaths, openPath, type AppPaths } from "@/lib/ipc";

interface AdvancedSettingsProps {
  /** Restore every preference, after the second click. */
  onReset: () => void;
}

/**
 * Advanced: where the application keeps its data, a guarded reset, and the
 * features that are planned. The last list is deliberately written out: a
 * greyed-out switch would suggest the feature already exists.
 */
export function AdvancedSettings({ onReset }: AdvancedSettingsProps) {
  const [paths, setPaths] = useState<AppPaths | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [armed, setArmed] = useState(false);

  useEffect(() => {
    let active = true;
    if (!isDesktop()) {
      setProblem("Folders can only be opened in the desktop app.");
      return;
    }
    appPaths()
      .then((answer) => {
        if (active) {
          setPaths(answer);
        }
      })
      .catch((failure: unknown) => {
        if (active) {
          setProblem(describeError(failure));
        }
      });
    return () => {
      active = false;
    };
  }, []);

  function reveal(path: string | undefined) {
    if (!path) {
      return;
    }
    void openPath(path).catch((failure: unknown) =>
      setProblem(describeError(failure)),
    );
  }

  return (
    <SettingsPage
      title="Advanced"
      description="Data, reset and plans."
    >
      <Section title="Application data">
        <SettingRow
          stacked
          label="Data folder"
          hint="The download list and unfinished files."
        >
          <div className="flex items-center gap-2">
            <Tooltip
              content={paths?.dataDir ?? "Reading the folder…"}
              side="top"
              className="block min-w-0 flex-1"
            >
              <p className="truncate font-mono text-chrome-micro text-chrome-menu-text">
                {paths?.dataDir ?? "Reading the folder…"}
              </p>
            </Tooltip>
            <SettingsButton
              onClick={() => reveal(paths?.dataDir)}
              disabled={!paths}
            >
              Open
            </SettingsButton>
          </div>
        </SettingRow>
        <SettingRow
          stacked
          label="Log"
          hint="What happened, including failures."
        >
          <div className="flex items-center gap-2">
            <Tooltip
              content={paths?.logPath ?? "Reading the log…"}
              side="top"
              className="block min-w-0 flex-1"
            >
              <p className="truncate font-mono text-chrome-micro text-chrome-menu-text">
                {paths?.logPath ?? "Reading the log…"}
              </p>
            </Tooltip>
            <SettingsButton
              onClick={() => reveal(paths?.logPath)}
              disabled={!paths}
            >
              Open
            </SettingsButton>
          </div>
        </SettingRow>
        {problem ? (
          <p role="alert" className="text-chrome-micro text-chrome-error">
            {problem}
          </p>
        ) : null}
      </Section>

      <Section title="Reset">
        <p className="pb-2 text-xs leading-5 text-chrome-menu-text">
          Restores every setting on these pages, including the last folder used.
          Downloads and saved files are kept.
        </p>
        {armed ? (
          <div className="flex flex-wrap items-center gap-2">
            <SettingsButton tone="danger" onClick={onReset}>
              Reset everything
            </SettingsButton>
            <SettingsButton onClick={() => setArmed(false)}>Cancel</SettingsButton>
          </div>
        ) : (
          <div>
            <SettingsButton tone="danger" onClick={() => setArmed(true)}>
              Reset settings…
            </SettingsButton>
          </div>
        )}
      </Section>

      <Section title="Planned">
        <p className="pb-1.5 text-xs leading-5 text-chrome-menu-text">
          Not built yet.
        </p>
        <ul className="list-disc pl-4 text-chrome-micro leading-5 text-chrome-muted marker:text-chrome-muted">
          <li>Sounds for a start, a finish and a failure.</li>
          <li>A virus scan of a finished file.</li>
          <li>A schedule and named queues.</li>
          <li>Saved logins for sites that ask before they hand over a file.</li>
          <li>A password for the proxy.</li>
          <li>Joining separate video and audio streams.</li>
        </ul>
      </Section>
    </SettingsPage>
  );
}
