import { useEffect, useState } from "react";

import { SettingsButton } from "@/components/settings/settings-button";
import { Switch } from "@/components/ui/switch";
import {
  Section,
  SettingRow,
  SettingsPage,
} from "@/components/settings/settings-page";
import { Tooltip } from "@/components/ui/tooltip";
import { describeError, isDesktop } from "@/lib/errors";
import { appPaths, openPath, type AppPaths } from "@/lib/ipc";
import { mcpEnabled, setMcpEnabled } from "@/lib/ipc-mcp";

interface AdvancedSettingsProps {
  /** Restore every preference, after the second click. */
  onReset: () => void;
}

/**
 * Advanced: where the application keeps its data, the door an AI assistant
 * comes through, and a guarded reset.
 */
export function AdvancedSettings({ onReset }: AdvancedSettingsProps) {
  const [paths, setPaths] = useState<AppPaths | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [armed, setArmed] = useState(false);
  const [assistants, setAssistants] = useState(false);

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }
    let active = true;
    void mcpEnabled()
      .then((answer) => {
        if (active) {
          setAssistants(answer);
        }
      })
      .catch(() => {
        // The switch stays off when the engine cannot say.
      });
    return () => {
      active = false;
    };
  }, []);

  function toggleAssistants(on: boolean) {
    setAssistants(on);
    void setMcpEnabled(on).catch((failure: unknown) => {
      setProblem(describeError(failure));
      setAssistants(!on);
    });
  }

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
    <SettingsPage page="advanced">
      <Section title="Application data">
        <SettingRow
          settingId="setting-data-folder"
          stacked
          label="Data folder"
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
          settingId="setting-staging-folder"
          stacked
          label="Temporary files"
        >
          <div className="flex items-center gap-2">
            <Tooltip
              content={paths?.stagingDir ?? "Reading the folder…"}
              side="top"
              className="block min-w-0 flex-1"
            >
              <p className="truncate font-mono text-chrome-micro text-chrome-menu-text">
                {paths?.stagingDir ?? "Reading the folder…"}
              </p>
            </Tooltip>
            <SettingsButton
              onClick={() => reveal(paths?.stagingDir)}
              disabled={!paths}
            >
              Open
            </SettingsButton>
          </div>
        </SettingRow>
        <SettingRow
          settingId="setting-log"
          stacked
          label="Log"
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

      <Section title="AI assistants">
        <SettingRow
          settingId="setting-mcp"
          label="Let AI assistants drive this app"
        >
          <Switch
            label="Let AI assistants drive this app"
            checked={assistants}
            onChange={toggleAssistants}
          />
        </SettingRow>
        <p className="pt-1 text-chrome-micro leading-4 text-chrome-muted">
          Off by default. An assistant starts the idm-mcp program beside this app
          and may list, add, pause, resume and remove downloads. Deleting a
          file from disk needs a second, explicit call.
        </p>
      </Section>

      <Section id="setting-reset" title="Reset">
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
    </SettingsPage>
  );
}
