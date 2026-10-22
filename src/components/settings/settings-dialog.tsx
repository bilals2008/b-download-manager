import { useEffect, useRef, useState } from "react";

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
import { applyFont, applySize, applyWeight } from "@/lib/fonts";
import { setMaxConnections, setMinimizeToTray, setProxy, setShowCompleteDialog, setSpeedLimit } from "@/lib/ipc";
import { DEFAULT_SETTINGS, loadSettings, saveSettings, type Settings } from "@/lib/settings";
import { applyTheme } from "@/lib/theme";

interface SettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

/** How long typing settles before the engine hears about it. */
const ENGINE_PUSH_DELAY = 300;

/** The settings the engine owns; only these are pushed over IPC. */
const ENGINE_KEYS: readonly (keyof Settings)[] = [
  "proxyUrl",
  "speedLimitEnabled",
  "speedLimitBytes",
  "maxConnections",
  "minimizeToTray",
  "showCompleteDialog",
];

/**
 * "Internet Download Manager Configuration" - the Options window. It opens on
 * the General page and writes every change as it is made: there is no Save to
 * press and nothing to cancel, which is what makes the appearance previews
 * honest.
 *
 * The settings the engine owns are pushed to it once typing settles, so a
 * proxy address is not sent on every keystroke.
 */
export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [page, setPage] = useState<SettingsPageId>("general");
  const [error, setError] = useState<string | null>(null);
  const pushTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (open) {
      setSettings(loadSettings());
      setError(null);
    }
  }, [open]);

  useEffect(
    () => () => {
      if (pushTimer.current) {
        clearTimeout(pushTimer.current);
      }
    },
    [],
  );

  function update<K extends keyof Settings>(key: K, value: Settings[K]) {
    const next = { ...settings, [key]: value };
    setSettings(next);
    saveSettings(next);

    // The appearance lands at once; the window is the preview.
    if (key === "theme") {
      applyTheme(next.theme);
    } else if (key === "uiFont") {
      applyFont(next.uiFont);
    } else if (key === "uiWeight") {
      applyWeight(next.uiWeight);
    } else if (key === "uiSize") {
      applySize(next.uiSize);
    }

    if (!ENGINE_KEYS.includes(key)) {
      return;
    }
    if (pushTimer.current) {
      clearTimeout(pushTimer.current);
    }
    pushTimer.current = setTimeout(() => void pushEngine(next), ENGINE_PUSH_DELAY);
  }

  async function pushEngine(next: Settings) {
    try {
      await setProxy(next.proxyUrl);
      await setSpeedLimit(next.speedLimitEnabled ? next.speedLimitBytes : 0);
      await setMaxConnections(next.maxConnections);
      await setMinimizeToTray(next.minimizeToTray);
      await setShowCompleteDialog(next.showCompleteDialog);
      setError(null);
    } catch (failure) {
      setError(describeError(failure));
    }
  }

  return (
    <Dialog
      open={open}
      title="Internet Download Manager Configuration"
      onClose={onClose}
      // The options window is the main window's sibling, not a popup: the same
      // canvas, the same two panes, so the two windows read as one product.
      className="h-[min(36rem,calc(100dvh-2rem))] w-[min(52rem,calc(100vw-2rem))] max-w-none bg-chrome-canvas"
      bodyClassName="flex gap-1.5 p-1.5"
      footer={
        <>
          <span className="mr-auto hidden self-center text-chrome-micro text-chrome-muted sm:inline">
            Changes apply as you make them.
          </span>
          {error ? (
            <span role="alert" className="mr-auto text-xs text-chrome-error">
              {error}
            </span>
          ) : null}
          <button
            type="button"
            onClick={onClose}
            className="rounded-chrome bg-chrome-panel px-3 py-1.5 text-xs text-chrome-menu-text transition-colors hover:bg-chrome-menu-hover focus-visible:ring-2 focus-visible:ring-chrome-focus focus-visible:ring-inset focus-visible:outline-none"
          >
            Close
          </button>
        </>
      }
    >
      <SettingsNav page={page} onSelect={setPage} />
      <div className="scrollbar-none min-h-0 flex-1 overflow-y-auto rounded-lg bg-chrome-panel">
        {page === "general" ? (
          <GeneralSettings settings={settings} onChange={update} />
        ) : null}
        {page === "save-to" ? (
          <SaveToSettings settings={settings} onChange={update} />
        ) : null}
        {page === "downloads" ? (
          <DownloadsSettings settings={settings} onChange={update} />
        ) : null}
        {page === "connection" ? (
          <ConnectionSettings settings={settings} onChange={update} />
        ) : null}
        {page === "proxy" ? (
          <ProxySettings settings={settings} onChange={update} />
        ) : null}
      </div>
    </Dialog>
  );
}
