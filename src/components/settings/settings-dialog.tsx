import { useEffect, useState } from "react";

import { ConnectionSettings } from "@/components/settings/connection-settings";
import { DownloadsSettings } from "@/components/settings/downloads-settings";
import { GeneralSettings } from "@/components/settings/general-settings";
import { ProxySettings } from "@/components/settings/proxy-settings";
import { SaveToSettings } from "@/components/settings/save-to-settings";
import {
  SettingsNav,
  type SettingsPageId,
} from "@/components/settings/settings-nav";
import { Dialog } from "@/components/ui/dialog";
import { describeError } from "@/lib/errors";
import { setMaxConnections, setMinimizeToTray, setProxy, setShowCompleteDialog, setSpeedLimit } from "@/lib/ipc";
import {
  DEFAULT_SETTINGS,
  loadSettings,
  saveSettings,
  type Settings,
  type ThemePreference,
} from "@/lib/settings";
import { applyTheme } from "@/lib/theme";

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

/**
 * "Internet Download Manager Configuration" - the Options window. It opens on
 * the General page, keeps edits in a draft, and writes them only on Save so
 * Cancel genuinely discards.
 *
 * The settings the engine owns (the proxy, the bandwidth cap and the number
 * of connections) are pushed to the engine when the window is saved, so they
 * take effect at once.
 */
export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const [draft, setDraft] = useState<Settings>(DEFAULT_SETTINGS);
  const [page, setPage] = useState<SettingsPageId>("general");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setDraft(loadSettings());
      setError(null);
    }
  }, [open]);

  function cancel() {
    // A previewed theme that was not saved must not outlive the window.
    applyTheme(loadSettings().theme);
    onClose();
  }

  function update<K extends keyof Settings>(key: K, value: Settings[K]) {
    setDraft((current) => ({ ...current, [key]: value }));
    // The appearance previews at once; Cancel puts the saved one back.
    if (key === "theme") {
      applyTheme(value as ThemePreference);
    }
  }

  async function save() {
    saveSettings(draft);
    try {
      // The engine has to know about the three settings it owns.
      await setProxy(draft.proxyUrl);
      await setSpeedLimit(
        draft.speedLimitEnabled ? draft.speedLimitBytes : 0,
      );
      await setMaxConnections(draft.maxConnections);
      await setMinimizeToTray(draft.minimizeToTray);
      await setShowCompleteDialog(draft.showCompleteDialog);
      setError(null);
    } catch (failure) {
      setError(describeError(failure));
      return;
    }
    onClose();
  }

  return (
    <Dialog
      open={open}
      title="Internet Download Manager Configuration"
      onClose={cancel}
      className="h-[min(36rem,calc(100dvh-2rem))] w-[min(52rem,calc(100vw-2rem))] max-w-none"
      bodyClassName="flex p-0"
      footer={
        <>
          <span className="mr-auto hidden self-center text-chrome-micro text-chrome-muted sm:inline">
            Preferences stay on this device.
          </span>
          {error ? (
            <span role="alert" className="mr-auto text-xs text-chrome-error">
              {error}
            </span>
          ) : null}
          <button
            type="button"
            onClick={cancel}
            className="rounded-chrome bg-chrome-panel px-3 py-1.5 text-xs text-chrome-menu-text transition-colors hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={save}
            className="rounded-chrome bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Save
          </button>
        </>
      }
    >
      <SettingsNav page={page} onSelect={setPage} />
      <div className="min-h-0 flex-1 overflow-y-auto">
        {page === "general" ? (
          <GeneralSettings settings={draft} onChange={update} />
        ) : null}
        {page === "save-to" ? (
          <SaveToSettings settings={draft} onChange={update} />
        ) : null}
        {page === "downloads" ? (
          <DownloadsSettings settings={draft} onChange={update} />
        ) : null}
        {page === "connection" ? (
          <ConnectionSettings settings={draft} onChange={update} />
        ) : null}
        {page === "proxy" ? (
          <ProxySettings settings={draft} onChange={update} />
        ) : null}
      </div>
    </Dialog>
  );
}
